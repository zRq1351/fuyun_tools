use crate::core::app_state::AppState;
use crate::features::screenshot::capture;
use crate::services::clipboard_wakeup::subscribe_clipboard_wake_events;
use crate::sync::Mutex;
use crate::utils::image_clipboard::ImageClipboardManager;
use parking_lot::Mutex as ParkingMutex;
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::sync::{Arc, Condvar, LazyLock, Mutex as StdMutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// 待处理图片任务
#[derive(Clone)]
struct PendingImageTask {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    source_blob: Option<(Vec<u8>, String)>,
    allow_when_screenshot: bool,
    enqueued_at: Instant,
}

/// 待处理图片队列
static PENDING_IMAGE_QUEUE: LazyLock<ParkingMutex<VecDeque<PendingImageTask>>> =
    LazyLock::new(|| ParkingMutex::new(VecDeque::new()));

/// 队列通知机制：用于在有新任务时唤醒处理线程
static QUEUE_NOTIFY: LazyLock<Arc<(StdMutex<u64>, Condvar)>> =
    LazyLock::new(|| Arc::new((StdMutex::new(0), Condvar::new())));

/// 队列最大容量，防止内存溢出
const MAX_QUEUE_SIZE: usize = 20;
const IMAGE_QUEUE_WORKER_COUNT: usize = 2;

struct ImageQueueMetrics {
    enqueued: AtomicU64,
    dequeued: AtomicU64,
    dropped_full: AtomicU64,
    dropped_duplicate: AtomicU64,
    dropped_screenshot: AtomicU64,
    queue_wait_ms_total: AtomicU64,
    queue_len_high_watermark: AtomicUsize,
}

static IMAGE_QUEUE_METRICS: LazyLock<ImageQueueMetrics> = LazyLock::new(|| ImageQueueMetrics {
    enqueued: AtomicU64::new(0),
    dequeued: AtomicU64::new(0),
    dropped_full: AtomicU64::new(0),
    dropped_duplicate: AtomicU64::new(0),
    dropped_screenshot: AtomicU64::new(0),
    queue_wait_ms_total: AtomicU64::new(0),
    queue_len_high_watermark: AtomicUsize::new(0),
});

static IMAGE_WORKERS_STARTED: AtomicBool = AtomicBool::new(false);
static IMAGE_LISTENER_RUNNING: AtomicBool = AtomicBool::new(false);

fn image_listener_stop_tx() -> &'static StdMutex<Option<Sender<()>>> {
    static STOP_TX: OnceLock<StdMutex<Option<Sender<()>>>> = OnceLock::new();
    STOP_TX.get_or_init(|| StdMutex::new(None))
}

fn lock_state<'a>(state: &'a Arc<Mutex<AppState>>) -> crate::sync::MutexGuard<'a, AppState> {
    state.lock().unwrap_or_else(|e| { log::error!("Mutex poisoned: {:?}", e); e.into_inner() })
}

fn observe_queue_len(len: usize) {
    let watermark = &IMAGE_QUEUE_METRICS.queue_len_high_watermark;
    loop {
        let current = watermark.load(Ordering::Relaxed);
        if len <= current {
            break;
        }
        if watermark
            .compare_exchange(current, len, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            break;
        }
    }
}

fn maybe_log_queue_metrics() {
    let enqueued = IMAGE_QUEUE_METRICS.enqueued.load(Ordering::Relaxed);
    if enqueued == 0 || enqueued % 40 != 0 {
        return;
    }
    let dequeued = IMAGE_QUEUE_METRICS.dequeued.load(Ordering::Relaxed);
    let dropped_full = IMAGE_QUEUE_METRICS.dropped_full.load(Ordering::Relaxed);
    let dropped_duplicate = IMAGE_QUEUE_METRICS
        .dropped_duplicate
        .load(Ordering::Relaxed);
    let dropped_screenshot = IMAGE_QUEUE_METRICS
        .dropped_screenshot
        .load(Ordering::Relaxed);
    let wait_total = IMAGE_QUEUE_METRICS
        .queue_wait_ms_total
        .load(Ordering::Relaxed);
    let avg_wait = if dequeued > 0 {
        wait_total as f64 / dequeued as f64
    } else {
        0.0
    };
    let high_watermark = IMAGE_QUEUE_METRICS
        .queue_len_high_watermark
        .load(Ordering::Relaxed);
    log::info!(
        "[队列指标] 入队={}, 出队={}, 满队丢弃={}, 重复丢弃={}, 截图丢弃={}, 平均排队等待={:.1}ms, 队列高水位={}",
        enqueued,
        dequeued,
        dropped_full,
        dropped_duplicate,
        dropped_screenshot,
        avg_wait,
        high_watermark
    );
}

pub fn emit_image_history_payload(app_handle: &AppHandle, state: Arc<Mutex<AppState>>) {
    let (manager_arc, should_emit) = {
        let mut state_guard = lock_state(&state);
        if !state_guard.is_image_visible {
            state_guard.image_history_dirty = true;
        }
        (
            state_guard.image_clipboard_manager.clone(),
            state_guard.is_image_visible,
        )
    };
    if !should_emit {
        return;
    }
    let manager = {
        let guard = manager_arc.lock().unwrap_or_else(|e| { log::error!("Mutex poisoned: {:?}", e); e.into_inner() });
        guard.clone()
    };
    let payload = serde_json::json!({
        "history": manager.get_history_preview(),
        "categories": manager.get_categories(),
        "category_list": manager.get_category_list(),
        "image_tags": manager.get_image_tags(),
        "pinned_items": manager.get_pinned_items()
    });
    let _ = app_handle.emit("image-history-payload-updated", payload);
    {
        let mut state_guard = lock_state(&state);
        state_guard.image_history_dirty = false;
    }
}

// 快速去重：存储最近图片的采样数据用于快速比较
const SAMPLE_POINTS: usize = 10;
type ImageSample = (u32, u32, [u8; SAMPLE_POINTS]);
static RECENT_IMAGE_SAMPLES: LazyLock<ParkingMutex<Vec<ImageSample>>> =
    LazyLock::new(|| ParkingMutex::new(Vec::new()));

// 快速采样：从 RGBA 数据中提取 10 个采样点
fn extract_sample_points(rgba: &[u8], width: u32, height: u32) -> [u8; SAMPLE_POINTS] {
    let mut sample = [0u8; SAMPLE_POINTS];
    if rgba.is_empty() || width == 0 || height == 0 {
        return sample;
    }

    let total_bytes = rgba.len();
    let step = (total_bytes / SAMPLE_POINTS).max(1);

    for (i, item) in sample.iter_mut().enumerate().take(SAMPLE_POINTS) {
        let idx = (i * step).min(total_bytes - 1);
        *item = rgba[idx];
    }
    sample
}

// 快速去重检查：仅作为粗筛提示，真正是否重复仍以后续完整签名判断为准。
fn matches_recent_sample(width: u32, height: u32, rgba: &[u8]) -> bool {
    let sample = extract_sample_points(rgba, width, height);
    let recent = RECENT_IMAGE_SAMPLES.lock();

    // 只检查最近 3 张
    for (idx, (recent_width, recent_height, recent_sample)) in
        recent.iter().rev().take(3).enumerate()
    {
        if *recent_width == width && *recent_height == height {
            // 比较采样点
            if recent_sample == &sample {
                log::info!(
                    "[重复检查] 图片 {}x{} 与最近第 {} 张图片采样命中，继续执行强签名校验",
                    width,
                    height,
                    idx + 1
                );
                return true;
            }
        }
    }
    log::debug!("[重复检查] 图片 {}x{} 未发现重复", width, height);
    false
}

// 更新最近图片的采样缓存
fn update_recent_samples_with_sample(width: u32, height: u32, sample: [u8; SAMPLE_POINTS]) {
    let mut recent = RECENT_IMAGE_SAMPLES.lock();

    // 添加新的采样
    recent.push((width, height, sample));

    // 只保留最近 5 张
    if recent.len() > 5 {
        recent.remove(0);
    }
}

/// 清理最近图片的采样缓存（在删除图片时调用）
pub fn clear_recent_samples() {
    let mut recent = RECENT_IMAGE_SAMPLES.lock();
    recent.clear();
    log::debug!("[缓存清理] 已清理最近图片采样缓存");
}

/// 处理队列中的待处理图片任务
fn process_pending_queue(app_handle: &AppHandle, state: &Arc<Mutex<AppState>>, worker_id: usize) {
    log::info!("[处理线程-{}] 图片处理线程已启动，等待任务...", worker_id);
    loop {
        let task = {
            let mut queue = PENDING_IMAGE_QUEUE.lock();
            let task = queue.pop_front();
            if let Some(ref t) = task {
                let wait_ms = t.enqueued_at.elapsed().as_millis() as u64;
                IMAGE_QUEUE_METRICS.dequeued.fetch_add(1, Ordering::Relaxed);
                IMAGE_QUEUE_METRICS
                    .queue_wait_ms_total
                    .fetch_add(wait_ms, Ordering::Relaxed);
                log::info!(
                    "[处理线程-{}] 图片出队: {}x{}, 队列等待={}ms, 剩余队列长度: {}",
                    worker_id,
                    t.width,
                    t.height,
                    wait_ms,
                    queue.len()
                );
            }
            task
        };

        match task {
            Some(task) => {
                if capture::is_screenshot_in_progress() && !task.allow_when_screenshot {
                    IMAGE_QUEUE_METRICS
                        .dropped_screenshot
                        .fetch_add(1, Ordering::Relaxed);
                    log::info!(
                        "[处理线程-{}] 截图进行中，跳过图片任务: {}x{}",
                        worker_id,
                        task.width,
                        task.height
                    );
                    continue;
                }
                // 快速采样仅作粗筛，不能直接丢图；真正去重交给后续完整签名。
                if matches_recent_sample(task.width, task.height, &task.rgba) {
                    log::debug!(
                        "[处理线程-{}] 图片采样命中，进入强签名去重: {}x{}",
                        worker_id,
                        task.width,
                        task.height
                    );
                }

                log::info!(
                    "[处理线程-{}] 开始处理图片任务: {}x{}",
                    worker_id,
                    task.width,
                    task.height
                );

                let manager_arc = {
                    let state_guard = lock_state(state);
                    state_guard.image_clipboard_manager.clone()
                };

                let sample = extract_sample_points(&task.rgba, task.width, task.height);
                let PendingImageTask {
                    rgba,
                    width,
                    height,
                    source_blob,
                    ..
                } = task;
                let delta_item = {
                    let manager = match manager_arc.lock() {
                        Ok(guard) => guard,
                        Err(e) => {
                            log::error!("[处理线程-{}] 获取 manager 锁失败: {:?}", worker_id, e);
                            continue;
                        }
                    };
                    let manager = manager.clone();
                    manager.add_rgba_image_with_source_blob(rgba, width, height, source_blob);
                    let history_preview = manager.get_history_preview();
                    let pinned_set = manager
                        .get_pinned_items()
                        .into_iter()
                        .collect::<HashSet<_>>();
                    history_preview
                        .iter()
                        .find(|item| !pinned_set.contains(&item.id))
                        .cloned()
                        .or_else(|| history_preview.first().cloned())
                };
                log::info!(
                    "[处理线程-{}] 图片处理成功: {}x{}",
                    worker_id,
                    width,
                    height
                );

                // 更新采样缓存
                update_recent_samples_with_sample(width, height, sample);

                let is_image_visible = {
                    let state_guard = lock_state(state);
                    state_guard.is_image_visible
                };
                if is_image_visible {
                    if let Some(item) = delta_item {
                        let payload = serde_json::json!({ "item": item });
                        let _ = app_handle.emit("image-history-item-added", payload);
                    } else {
                        emit_image_history_payload(app_handle, state.clone());
                    }
                } else {
                    let mut state_guard = lock_state(state);
                    state_guard.image_history_dirty = true;
                }

                log::info!("[处理线程-{}] 图片处理流程完成", worker_id);
                maybe_log_queue_metrics();
            }
            None => {
                // 队列为空，等待通知而不是退出循环
                log::trace!("[处理线程-{}] 队列为空，等待新任务通知...", worker_id);
                let (lock, cvar) = &**QUEUE_NOTIFY;
                let mut notify_seq = match lock.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        log::error!("[处理线程-{}] 通知锁已中毒，尝试恢复继续处理", worker_id);
                        poisoned.into_inner()
                    }
                };
                let observed_seq = *notify_seq;
                while *notify_seq == observed_seq {
                    notify_seq = match cvar.wait(notify_seq) {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            log::error!(
                                "[处理线程-{}] 通知条件变量等待异常（锁中毒），尝试恢复继续处理",
                                worker_id
                            );
                            poisoned.into_inner()
                        }
                    };
                }
                log::trace!("[处理线程-{}] 收到新任务通知，继续处理", worker_id);
                continue;
            }
        }
    }
}

pub fn start_image_clipboard_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    if IMAGE_WORKERS_STARTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        for worker_id in 0..IMAGE_QUEUE_WORKER_COUNT {
            let app_handle_clone = app_handle.clone();
            let state_clone = state.clone();
            thread::spawn(move || {
                process_pending_queue(&app_handle_clone, &state_clone, worker_id + 1);
            });
        }
    }

    if IMAGE_LISTENER_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    if let Ok(mut guard) = image_listener_stop_tx().lock() {
        *guard = Some(stop_tx);
    }

    // 启动监听线程
    thread::spawn(move || {
        let wake_rx = subscribe_clipboard_wake_events();

        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }
            match wake_rx.recv_timeout(Duration::from_millis(250)) {
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            let should_skip = {
                let state_guard = lock_state(&state);
                if !state_guard.settings.image_clipboard_enabled {
                    continue;
                }
                state_guard.is_updating_clipboard || state_guard.is_processing_selection
            };
            if should_skip {
                continue;
            }
            let screenshot_in_progress = capture::is_screenshot_in_progress();
            let allow_when_screenshot = if screenshot_in_progress {
                capture::take_allow_image_clipboard_once()
            } else {
                false
            };
            if screenshot_in_progress && !allow_when_screenshot {
                continue;
            }

            log::info!("[监听线程] 收到剪贴板变化事件");

            // 简化：所有图片都直接入队，由队列机制统一处理
            let image = ImageClipboardManager::read_clipboard_images_rgba(&app_handle);
            match image {
                Ok(images) => {
                    log::info!("[监听线程] 成功读取 {} 张图片", images.len());
                    let mut queue = PENDING_IMAGE_QUEUE.lock();
                    for (rgba, width, height, source_blob) in images {
                        // 检查队列容量
                        if queue.len() >= MAX_QUEUE_SIZE {
                            log::warn!(
                                "[监听线程] 待处理队列已满（{}），丢弃最早的图片",
                                MAX_QUEUE_SIZE
                            );
                            queue.pop_front();
                            IMAGE_QUEUE_METRICS
                                .dropped_full
                                .fetch_add(1, Ordering::Relaxed);
                        }
                        queue.push_back(PendingImageTask {
                            rgba,
                            width,
                            height,
                            source_blob,
                            allow_when_screenshot,
                            enqueued_at: Instant::now(),
                        });
                        IMAGE_QUEUE_METRICS.enqueued.fetch_add(1, Ordering::Relaxed);
                        observe_queue_len(queue.len());
                        log::info!(
                            "[监听线程] 图片入队: {}x{}, 当前队列长度: {}",
                            width,
                            height,
                            queue.len()
                        );
                    }
                    drop(queue); // 释放锁

                    // 通知处理线程有新任务
                    let (lock, cvar) = &**QUEUE_NOTIFY;
                    let mut notified = match lock.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            log::error!("[监听线程] 通知锁已中毒，尝试恢复继续通知");
                            poisoned.into_inner()
                        }
                    };
                    *notified = notified.wrapping_add(1);
                    cvar.notify_all();
                    log::info!("[监听线程] 已通知处理线程");
                    maybe_log_queue_metrics();
                }
                Err(e) => {
                    log::warn!("[监听线程] 读取剪贴板图片失败: {}", e);
                }
            }
        }
        IMAGE_LISTENER_RUNNING.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = image_listener_stop_tx().lock() {
            *guard = None;
        }
    });
}

pub fn stop_image_clipboard_listener() {
    if let Ok(mut guard) = image_listener_stop_tx().lock() {
        if let Some(tx) = guard.take() {
            let _ = tx.send(());
        }
    }
}

pub fn set_image_clipboard_listener_enabled(
    app_handle: AppHandle,
    state: Arc<Mutex<AppState>>,
    enabled: bool,
) {
    if enabled {
        start_image_clipboard_listener(app_handle, state);
    } else {
        stop_image_clipboard_listener();
    }
}
