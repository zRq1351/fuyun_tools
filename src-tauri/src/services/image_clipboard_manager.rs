use crate::core::app_state::AppState;
use crate::core::config::CLIPBOARD_POLL_INTERVAL;
use crate::utils::image_clipboard::ImageClipboardManager;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub fn start_image_clipboard_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    thread::spawn(move || {
        let mut last_signature = String::new();
        let mut last_error = String::new();
        let mut check_interval = CLIPBOARD_POLL_INTERVAL;
        let mut last_check_time = std::time::Instant::now();

        loop {
            let elapsed = last_check_time.elapsed();
            if elapsed < check_interval {
                thread::sleep(check_interval - elapsed);
            }
            last_check_time = std::time::Instant::now();

            let should_skip = {
                let state_guard = state.lock().unwrap();
                state_guard.is_updating_clipboard || state_guard.is_processing_selection
            };

            if should_skip {
                thread::sleep(Duration::from_millis(120));
                continue;
            }

            let image = ImageClipboardManager::read_clipboard_images_rgba(&app_handle);
            if let Ok(images) = image {
                last_error.clear();
                let signature = build_fast_signature(&images);

                if signature != last_signature {
                    let manager_arc = {
                        let state_guard = state.lock().unwrap();
                        state_guard.image_clipboard_manager.clone()
                    };
                    let manager = manager_arc.lock().unwrap();
                    for (rgba, width, height) in images {
                        manager.add_rgba_image(rgba, width, height);
                    }
                    let _ = app_handle.emit("image-history-updated", serde_json::json!({}));
                    last_signature = signature;
                    check_interval = Duration::from_millis(50);
                } else {
                    check_interval = CLIPBOARD_POLL_INTERVAL;
                }
            } else if let Err(e) = image {
                if e != last_error {
                    log::debug!("图片剪贴板监听读取失败: {}", e);
                    last_error = e;
                }
                check_interval = CLIPBOARD_POLL_INTERVAL;
            }
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
