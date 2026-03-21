use crate::core::app_state::AppState;
use crate::services::clipboard_wakeup::{ClipboardWakeBackend, WakeSignal};
use crate::sync::Mutex;
use crate::utils::image_clipboard::ImageClipboardManager;
use parking_lot::Mutex as ParkingMutex;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, LazyLock, Mutex as StdMutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// 待处理图片任务
#[derive(Clone)]
struct PendingImageTask {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    source_blob: Option<(Vec<u8>, String)>,
}

/// 待处理图片队列
static PENDING_IMAGE_QUEUE: LazyLock<ParkingMutex<VecDeque<PendingImageTask>>> =
    LazyLock::new(|| ParkingMutex::new(VecDeque::new()));

/// 队列通知机制：用于在有新任务时唤醒处理线程
static QUEUE_NOTIFY: LazyLock<Arc<(StdMutex<bool>, Condvar)>> =
    LazyLock::new(|| Arc::new((StdMutex::new(false), Condvar::new())));

/// 队列最大容量，防止内存溢出
const MAX_QUEUE_SIZE: usize = 20;

fn lock_state<'a>(state: &'a Arc<Mutex<AppState>>) -> crate::sync::MutexGuard<'a, AppState> {
    match state.lock() {
        Ok(guard) => guard,
        Err(e) => match e {},
    }
}

pub fn emit_image_history_payload(app_handle: &AppHandle, state: Arc<Mutex<AppState>>) {
    let manager_arc = {
        let state_guard = lock_state(&state);
        state_guard.image_clipboard_manager.clone()
    };
    let manager = match manager_arc.lock() {
        Ok(guard) => guard,
        Err(e) => match e {},
    };
    let payload = serde_json::json!({
        "history": manager.get_history_preview(),
        "categories": manager.get_categories(),
        "category_list": manager.get_category_list(),
        "image_tags": manager.get_image_tags(),
        "pinned_items": manager.get_pinned_items()
    });
    let _ = app_handle.emit("image-history-payload-updated", payload);
}


// 快速去重：存储最近图片的采样数据用于快速比较
const SAMPLE_POINTS: usize = 10;
static RECENT_IMAGE_SAMPLES: LazyLock<ParkingMutex<Vec<(u32, u32, [u8; SAMPLE_POINTS])>>> =
    LazyLock::new(|| ParkingMutex::new(Vec::new()));

// 快速采样：从 RGBA 数据中提取 10 个采样点
fn extract_sample_points(rgba: &[u8], width: u32, height: u32) -> [u8; SAMPLE_POINTS] {
    let mut sample = [0u8; SAMPLE_POINTS];
    if rgba.is_empty() || width == 0 || height == 0 {
        return sample;
    }

    let total_bytes = rgba.len();
    let step = (total_bytes / SAMPLE_POINTS).max(1);

    for i in 0..SAMPLE_POINTS {
        let idx = (i * step).min(total_bytes - 1);
        sample[i] = rgba[idx];
    }
    sample
}

// 快速去重检查：检查是否与最近的图片重复
fn is_duplicate_recent(width: u32, height: u32, rgba: &[u8]) -> bool {
    let sample = extract_sample_points(rgba, width, height);
    let recent = RECENT_IMAGE_SAMPLES.lock();

    // 只检查最近 3 张
    for (recent_width, recent_height, recent_sample) in recent.iter().rev().take(3) {
        if *recent_width == width && *recent_height == height {
            // 比较采样点
            if recent_sample == &sample {
                return true; // 重复
            }
        }
    }
    false
}

// 更新最近图片的采样缓存
fn update_recent_samples(width: u32, height: u32, rgba: &[u8]) {
    let sample = extract_sample_points(rgba, width, height);
    let mut recent = RECENT_IMAGE_SAMPLES.lock();

    // 添加新的采样
    recent.push((width, height, sample));

    // 只保留最近 5 张
    if recent.len() > 5 {
        recent.remove(0);
    }
}

/// 处理队列中的待处理图片任务
fn process_pending_queue(app_handle: &AppHandle, state: &Arc<Mutex<AppState>>) {
    log::info!("图片处理线程已启动，等待任务...");
    loop {
        let task = {
            let mut queue = PENDING_IMAGE_QUEUE.lock();
            queue.pop_front()
        };

        match task {
            Some(task) => {
                // 检查是否与最近图片重复
                if is_duplicate_recent(task.width, task.height, &task.rgba) {
                    log::trace!("队列中的图片与最近图片重复，跳过处理");
                    continue;
                }

                log::debug!("开始处理图片任务: {}x{}", task.width, task.height);

                let manager_arc = {
                    let state_guard = lock_state(state);
                    state_guard.image_clipboard_manager.clone()
                };

                let manager = match manager_arc.lock() {
                    Ok(guard) => guard,
                    Err(e) => match e {},
                };

                manager.add_rgba_image_with_source_blob(
                    task.rgba.clone(),
                    task.width,
                    task.height,
                    task.source_blob,
                );
                drop(manager);

                // 更新采样缓存
                update_recent_samples(task.width, task.height, &task.rgba);

                // 发送更新事件
                emit_image_history_payload(app_handle, state.clone());

                log::info!("图片处理完成");
            }
            None => {
                // 队列为空，等待通知而不是退出循环
                log::trace!("队列为空，等待新任务通知...");
                let (lock, cvar) = &**QUEUE_NOTIFY;
                let mut notified = lock.lock().unwrap();
                while !*notified {
                    notified = cvar.wait(notified).unwrap();
                }
                *notified = false;
                log::trace!("收到新任务通知，继续处理");
                continue;
            }
        }
    }
}

pub fn start_image_clipboard_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    // 启动独立的处理线程
    let app_handle_clone = app_handle.clone();
    let state_clone = state.clone();
    thread::spawn(move || {
        process_pending_queue(&app_handle_clone, &state_clone);
    });

    // 启动监听线程
    thread::spawn(move || {
        let mut wake_backend = ClipboardWakeBackend::new();

        loop {
            let signal = wake_backend.wait_with_signal(Duration::from_secs(24 * 60 * 60));
            #[cfg(target_os = "windows")]
            if !matches!(signal, WakeSignal::Event) {
                continue;
            }

            // 简化：所有图片都直接入队，由队列机制统一处理
            let image = ImageClipboardManager::read_clipboard_images_rgba(&app_handle);
            if let Ok(images) = image {
                let mut queue = PENDING_IMAGE_QUEUE.lock();
                for (rgba, width, height, source_blob) in images {
                    // 检查队列容量
                    if queue.len() >= MAX_QUEUE_SIZE {
                        log::warn!("待处理队列已满（{}），丢弃最早的图片", MAX_QUEUE_SIZE);
                        queue.pop_front();
                    }
                    queue.push_back(PendingImageTask {
                        rgba,
                        width,
                        height,
                        source_blob,
                    });
                    log::debug!("图片已加入待处理队列，当前队列长度: {}", queue.len());
                }
                drop(queue);  // 释放锁

                // 通知处理线程有新任务
                let (lock, cvar) = &**QUEUE_NOTIFY;
                let mut notified = lock.lock().unwrap();
                *notified = true;
                cvar.notify_one();
            }
        }
    });
}

