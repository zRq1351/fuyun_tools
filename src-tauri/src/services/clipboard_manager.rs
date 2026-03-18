use crate::core::app_state::AppState;
use crate::services::clipboard_wakeup::{ClipboardWakeBackend, WakeSignal};
use crate::sync::Mutex;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

fn lock_state<'a>(state: &'a Arc<Mutex<AppState>>) -> crate::sync::MutexGuard<'a, AppState> {
    match state.lock() {
        Ok(guard) => guard,
        Err(e) => match e {},
    }
}

/// 启动剪贴板监听器
pub fn start_clipboard_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    thread::spawn(move || {
        let mut last_content = String::new();
        let mut wake_backend = ClipboardWakeBackend::new();

        loop {
            let signal = wake_backend.wait_with_signal(Duration::from_secs(24 * 60 * 60));
            #[cfg(target_os = "windows")]
            if !matches!(signal, WakeSignal::Event) {
                continue;
            }

            let (is_updating, manager_arc) = {
                let state_guard = lock_state(&state);
                (
                    state_guard.is_updating_clipboard
                        || state_guard.is_processing_selection
                        || state_guard.is_visible
                        || state_guard.is_image_visible,
                    state_guard.clipboard_manager.clone(),
                )
            };

            if is_updating {
                continue;
            }

            let current_content = {
                let manager = match manager_arc.lock() {
                    Ok(guard) => guard,
                    Err(e) => match e {},
                };
                manager.get_content(&app_handle)
            };

            if let Some(current_content) = current_content {
                if !current_content.is_empty() && current_content != last_content {
                    add_to_clipboard_history(&app_handle, current_content.clone(), state.clone());
                    last_content = current_content.clone();
                    log::info!("检测到剪贴板内容变化，已添加到历史记录");
                }
            }
        }
    });
}

/// 添加到剪贴板历史记录
pub fn add_to_clipboard_history(app_handle: &AppHandle, content: String, state: Arc<Mutex<AppState>>) {
    if content.trim().is_empty() {
        return;
    }

    let should_skip = {
        let state_guard = lock_state(&state);
        state_guard.is_processing_selection
    };

    if should_skip {
        log::debug!("正在进行划词操作，跳过添加到历史记录");
        return;
    }

    let manager_arc = {
        let state_guard = lock_state(&state);
        state_guard.clipboard_manager.clone()
    };

    {
        let manager = match manager_arc.lock() {
            Ok(guard) => guard,
            Err(e) => match e {},
        };
        manager.add_to_history(content);
    }

    let payload = {
        let manager = match manager_arc.lock() {
            Ok(guard) => guard,
            Err(e) => match e {},
        };
        serde_json::json!({
            "history": manager.get_history(),
            "categories": manager.get_categories(),
            "category_list": manager.get_category_list(),
            "pinned_items": manager.get_pinned_items()
        })
    };
    let _ = app_handle.emit("clipboard-history-payload-updated", payload);
}
