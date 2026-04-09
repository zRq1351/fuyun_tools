use crate::core::app_state::AppState;
use crate::services::clipboard_wakeup::subscribe_clipboard_wake_events;
use crate::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

fn lock_state<'a>(state: &'a Arc<Mutex<AppState>>) -> crate::sync::MutexGuard<'a, AppState> {
    state.lock().expect("infallible mutex lock failed")
}

fn clipboard_listener_stop_tx() -> &'static std::sync::Mutex<Option<Sender<()>>> {
    static STOP_TX: OnceLock<std::sync::Mutex<Option<Sender<()>>>> = OnceLock::new();
    STOP_TX.get_or_init(|| std::sync::Mutex::new(None))
}

static CLIPBOARD_LISTENER_RUNNING: AtomicBool = AtomicBool::new(false);

/// 启动剪贴板监听器
pub fn start_clipboard_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    if CLIPBOARD_LISTENER_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    if let Ok(mut guard) = clipboard_listener_stop_tx().lock() {
        *guard = Some(stop_tx);
    }
    thread::spawn(move || {
        let mut last_content = String::new();
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

            let (is_updating, manager_arc) = {
                let state_guard = lock_state(&state);
                if !state_guard.settings.text_clipboard_enabled {
                    continue;
                }
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
                let manager = manager_arc
                    .lock()
                    .expect("infallible mutex lock failed");
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
        CLIPBOARD_LISTENER_RUNNING.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = clipboard_listener_stop_tx().lock() {
            *guard = None;
        }
    });
}

pub fn stop_clipboard_listener() {
    if let Ok(mut guard) = clipboard_listener_stop_tx().lock() {
        if let Some(tx) = guard.take() {
            let _ = tx.send(());
        }
    }
}

pub fn set_clipboard_listener_enabled(app_handle: AppHandle, state: Arc<Mutex<AppState>>, enabled: bool) {
    if enabled {
        start_clipboard_listener(app_handle, state);
    } else {
        stop_clipboard_listener();
    }
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

    let (manager_arc, should_emit) = {
        let state_guard = lock_state(&state);
        (state_guard.clipboard_manager.clone(), state_guard.is_visible)
    };

    let payload = {
        let manager = manager_arc
            .lock()
            .expect("infallible mutex lock failed");
        manager.add_to_history(content);
        if !should_emit {
            None
        } else {
            let history = manager.get_history();
            let latest_item = history.first().cloned().unwrap_or_default();
            let is_pinned = if latest_item.is_empty() {
                false
            } else {
                manager.get_pinned_items().iter().any(|v| v == &latest_item)
            };
            Some(serde_json::json!({
                "latest_item": latest_item,
                "history_len": history.len(),
                "is_pinned": is_pinned
            }))
        }
    };
    if let Some(payload) = payload {
        let _ = app_handle.emit("clipboard-history-item-updated", payload);
    }
}
