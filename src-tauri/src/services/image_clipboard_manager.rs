use crate::core::app_state::AppState;
use crate::services::clipboard_wakeup::{ClipboardWakeBackend, WakeSignal};
use crate::utils::image_clipboard::ImageClipboardManager;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const IMAGE_CLIPBOARD_EVENT_SETTLE_MS: u64 = 120;
static IMAGE_CLIPBOARD_PROCESSING: AtomicBool = AtomicBool::new(false);

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

pub fn start_image_clipboard_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    thread::spawn(move || {
        let mut last_signature = String::new();
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
            #[cfg(target_os = "windows")]
            std::thread::sleep(Duration::from_millis(IMAGE_CLIPBOARD_EVENT_SETTLE_MS));

            let should_skip = {
                let state_guard = state.lock().unwrap();
                state_guard.is_updating_clipboard
                    || state_guard.is_processing_selection
                    || state_guard.is_visible
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

                if signature != last_signature {
                    let manager = {
                        let state_guard = state.lock().unwrap();
                        let manager_guard = state_guard.image_clipboard_manager.lock().unwrap();
                        manager_guard.clone()
                    };
                    for (rgba, width, height) in images {
                        manager.add_rgba_image(rgba, width, height);
                        let _ = app_handle.emit("image-history-updated", serde_json::json!({}));
                    }
                    last_signature = signature;
                }
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

fn build_fast_signature(images: &[(Vec<u8>, u32, u32)]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    images.len().hash(&mut hasher);
    for (rgba, width, height) in images {
        width.hash(&mut hasher);
        height.hash(&mut hasher);
        rgba.len().hash(&mut hasher);
        if !rgba.is_empty() {
            let step = (rgba.len() / 16).max(1);
            let mut idx = 0usize;
            while idx < rgba.len() {
                rgba[idx].hash(&mut hasher);
                idx += step;
            }
            rgba[rgba.len() - 1].hash(&mut hasher);
        }
    }
    format!("{:x}", hasher.finish())
}
