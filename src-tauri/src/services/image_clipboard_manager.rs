use crate::core::app_state::AppState;
use crate::services::clipboard_wakeup::{ClipboardWakeBackend, WakeSignal};
use crate::sync::Mutex;
use crate::utils::image_clipboard::ImageClipboardManager;
use parking_lot::Mutex as ParkingMutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

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

// 防抖机制：200ms 内的相同图片不重复处理
const DEBOUNCE_INTERVAL_MS: u64 = 200;
static LAST_IMAGE_TIME: AtomicU64 = AtomicU64::new(0);
static LAST_IMAGE_SIGNATURE: LazyLock<ParkingMutex<String>> = LazyLock::new(|| ParkingMutex::new(String::new()));

static IMAGE_CLIPBOARD_PROCESSING: AtomicBool = AtomicBool::new(false);

// 快速去重：存储最近图片的采样数据用于快速比较
const SAMPLE_POINTS: usize = 10;
static RECENT_IMAGE_SAMPLES: LazyLock<ParkingMutex<Vec<(u32, u32, [u8; SAMPLE_POINTS])>>> =
    LazyLock::new(|| ParkingMutex::new(Vec::new()));

#[cfg(target_os = "windows")]
fn read_clipboard_sequence_number() -> Option<u32> {
    use winapi::um::winuser::GetClipboardSequenceNumber;
    let seq = unsafe { GetClipboardSequenceNumber() };
    if seq == 0 {
        None
    } else {
        Some(seq)
    }
}

#[cfg(not(target_os = "windows"))]
fn read_clipboard_sequence_number() -> Option<u32> {
    None
}

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

// 防抖检查：200ms 内的相同图片不重复处理
fn should_process_with_debounce(signature: &str) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let last_time = LAST_IMAGE_TIME.load(Ordering::Relaxed);
    let mut last_sig = LAST_IMAGE_SIGNATURE.lock();

    // 200ms 内的相同图片 → 跳过
    if now.saturating_sub(last_time) < DEBOUNCE_INTERVAL_MS && last_sig.as_str() == signature {
        return false;
    }

    LAST_IMAGE_TIME.store(now, Ordering::Relaxed);
    *last_sig = signature.to_string();
    true
}

pub fn start_image_clipboard_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    thread::spawn(move || {
        let mut last_error = String::new();
        let mut last_clipboard_seq = read_clipboard_sequence_number();
        let mut wake_backend = ClipboardWakeBackend::new();

        loop {
            let signal = wake_backend.wait_with_signal(Duration::from_secs(24 * 60 * 60));
            #[cfg(target_os = "windows")]
            if !matches!(signal, WakeSignal::Event) {
                continue;
            }

            if IMAGE_CLIPBOARD_PROCESSING.swap(true, Ordering::SeqCst) {
                continue;
            }

            let (should_skip, manager_arc) = {
                let state_guard = lock_state(&state);
                (
                    state_guard.is_updating_clipboard
                        || state_guard.is_processing_selection
                        || state_guard.is_visible,
                    state_guard.image_clipboard_manager.clone(),
                )
            };

            if should_skip {
                IMAGE_CLIPBOARD_PROCESSING.store(false, Ordering::SeqCst);
                continue;
            }

            let current_clipboard_seq = read_clipboard_sequence_number();
            if let (Some(current), Some(last)) = (current_clipboard_seq, last_clipboard_seq) {
                if current == last {
                    IMAGE_CLIPBOARD_PROCESSING.store(false, Ordering::SeqCst);
                    continue;
                }
            }
            let mut should_advance_clipboard_seq = false;

            let image = ImageClipboardManager::read_clipboard_images_rgba(&app_handle);
            if let Ok(images) = image {
                should_advance_clipboard_seq = true;
                last_error.clear();
                let signature = build_fast_signature(&images);

                // 防抖检查：200ms 内的相同图片跳过
                if !should_process_with_debounce(&signature) {
                    IMAGE_CLIPBOARD_PROCESSING.store(false, Ordering::SeqCst);
                    continue;
                }

                // 处理图片
                let manager = match manager_arc.lock() {
                    Ok(guard) => guard,
                    Err(e) => match e {},
                };

                for (rgba, width, height, source_blob) in images {
                    // 快速去重：检查是否与最近图片重复
                    if !is_duplicate_recent(width, height, &rgba) {
                        manager.add_rgba_image_with_source_blob(rgba.clone(), width, height, source_blob);
                        // 更新采样缓存
                        update_recent_samples(width, height, &rgba);
                    }
                }
                drop(manager);

                emit_image_history_payload(&app_handle, state.clone());
            } else if let Err(e) = image {
                if e != last_error {
                    if e.contains("当前剪贴板不是位图格式") {
                        log::trace!("图片剪贴板监听读取提示: {}", e);
                        should_advance_clipboard_seq = true;
                    } else {
                        log::debug!("图片剪贴板监听读取失败: {}", e);
                    }
                    last_error = e;
                }
            }

            if should_advance_clipboard_seq && current_clipboard_seq.is_some() {
                last_clipboard_seq = current_clipboard_seq;
            }
            IMAGE_CLIPBOARD_PROCESSING.store(false, Ordering::SeqCst);
        }
    });
}

fn build_fast_signature(images: &[crate::utils::image_clipboard::ClipboardImagePayload]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    images.len().hash(&mut hasher);
    for (rgba, width, height, _) in images {
        width.hash(&mut hasher);
        height.hash(&mut hasher);
        rgba.len().hash(&mut hasher);
        if !rgba.is_empty() {
            // 优化：只采样 10 个点，而不是遍历整个数组
            let step = (rgba.len() / SAMPLE_POINTS).max(1);
            for i in 0..SAMPLE_POINTS {
                let idx = (i * step).min(rgba.len() - 1);
                rgba[idx].hash(&mut hasher);
            }
        }
    }
    format!("{:x}", hasher.finish())
}
