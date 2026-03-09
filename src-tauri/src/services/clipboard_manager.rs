use crate::core::app_state::AppState;
use crate::core::config::CLIPBOARD_POLL_INTERVAL;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

/// 启动剪贴板监听器
pub fn start_clipboard_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    thread::spawn(move || {
        let mut last_content = String::new();
        let mut check_interval = CLIPBOARD_POLL_INTERVAL;
        let mut last_check_time = std::time::Instant::now();

        loop {
            let elapsed = last_check_time.elapsed();
            if elapsed < check_interval {
                thread::sleep(check_interval - elapsed);
            }
            last_check_time = std::time::Instant::now();

            let (is_updating, should_skip) = {
                let state_guard = state.lock().unwrap();
                (
                    state_guard.is_updating_clipboard || state_guard.is_processing_selection,
                    state_guard.is_processing_selection,
                )
            };

            if is_updating {
                if should_skip {
                    thread::sleep(Duration::from_millis(200));
                }
                continue;
            }

            let current_content = {
                let state_guard = state.lock().unwrap();
                let manager = state_guard.clipboard_manager.lock().unwrap();
                manager.get_content(&app_handle)
            };

            if let Some(current_content) = current_content {
                if !current_content.is_empty() && current_content != last_content {
                    add_to_clipboard_history(current_content.clone(), state.clone());
                    last_content = current_content.clone();

                    check_interval = Duration::from_millis(50);
                    log::info!("检测到剪贴板内容变化，已添加到历史记录");
                } else {
                    check_interval = CLIPBOARD_POLL_INTERVAL;
                }
            } else {
                check_interval = CLIPBOARD_POLL_INTERVAL;
            }
        }
    });
}

/// 添加到剪贴板历史记录
pub fn add_to_clipboard_history(content: String, state: Arc<Mutex<AppState>>) {
    if content.trim().is_empty() {
        return;
    }

    let should_skip = {
        let state_guard = state.lock().unwrap();
        state_guard.is_processing_selection
    };

    if should_skip {
        log::debug!("正在进行划词操作，跳过添加到历史记录");
        return;
    }

    let manager_result = {
        let state_guard = state.lock().unwrap();
        state_guard.clipboard_manager.clone()
    };

    {
        let manager = manager_result.lock().unwrap();
        manager.add_to_history(content);
    }
}
