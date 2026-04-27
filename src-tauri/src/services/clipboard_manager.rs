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
    state.lock().unwrap()
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
        let mut missed_event = false;

        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }
            let timeout_ms = if missed_event { 50 } else { 250 };
            let has_event = match wake_rx.recv_timeout(Duration::from_millis(timeout_ms)) {
                Ok(_) => true,
                Err(mpsc::RecvTimeoutError::Timeout) => false,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };

            if has_event {
                missed_event = true;
            }

            let (is_updating, manager_arc) = {
                let state_guard = lock_state(&state);
                if !state_guard.settings.text_clipboard_enabled {
                    last_content.clear();
                    continue;
                }
                (
                    state_guard.is_updating_clipboard || state_guard.is_processing_selection,
                    state_guard.clipboard_manager.clone(),
                )
            };

            if is_updating {
                continue;
            }

            if !missed_event {
                continue;
            }

            missed_event = false;

            let current_content = {
                let manager = manager_arc.lock().unwrap();
                manager.get_content(&app_handle)
            };

            if let Some(current_content) = current_content {
                if !current_content.is_empty() && current_content != last_content {
                    add_to_clipboard_history(&app_handle, current_content.clone(), state.clone());
                    last_content = current_content;
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

pub fn set_clipboard_listener_enabled(
    app_handle: AppHandle,
    state: Arc<Mutex<AppState>>,
    enabled: bool,
) {
    if enabled {
        start_clipboard_listener(app_handle, state);
    } else {
        stop_clipboard_listener();
    }
}

/// 添加到剪贴板历史记录
pub fn add_to_clipboard_history(
    app_handle: &AppHandle,
    content: String,
    state: Arc<Mutex<AppState>>,
) {
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
        (
            state_guard.clipboard_manager.clone(),
            state_guard.is_visible,
        )
    };

    let payload = {
        let manager = manager_arc.lock().unwrap();
        manager.add_to_history(content);
        if !should_emit {
            None
        } else {
            let history_len = manager.get_history_len();
            let latest_item = manager.get_latest_item().unwrap_or_default();
            let is_pinned = if latest_item.is_empty() {
                false
            } else {
                let item_id = crate::utils::database::stable_history_item_id(&latest_item);
                manager.get_pinned_items().iter().any(|v| v == &item_id)
            };
            Some(serde_json::json!({
                "latest_item": latest_item,
                "latest_item_id": crate::utils::database::stable_history_item_id(&latest_item),
                "history_len": history_len,
                "is_pinned": is_pinned
            }))
        }
    };
    if let Some(payload) = payload {
        let _ = app_handle.emit("clipboard-history-item-updated", payload);
    }
}
