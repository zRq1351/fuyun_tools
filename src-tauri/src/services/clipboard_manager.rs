use crate::core::app_state::AppState;
use crate::services::clipboard_poller::ClipboardPoller;
use crate::sync::{lock_arc_mutex, Mutex};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter};

fn read_clipboard_text(app_handle: &AppHandle) -> Option<String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    match crate::services::clipboard_access_guard::with_clipboard_access_lock(|| {
        app_handle.clipboard().read_text()
    }) {
        Ok(content) => Some(content),
        Err(e) => {
            let msg = e.to_string();
            // Only log non-empty clipboard errors (empty clipboard is expected)
            if !msg.contains("Clipboard") && !msg.contains("empty") {
                log::debug!("获取剪贴板内容失败: {}", msg);
            }
            None
        }
    }
}

static CLIPBOARD_LISTENER_RUNNING: AtomicBool = AtomicBool::new(false);
static CLIPBOARD_STOP_TX: OnceLock<std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>> =
    OnceLock::new();

static POLLER: ClipboardPoller = ClipboardPoller::new(&CLIPBOARD_LISTENER_RUNNING, &CLIPBOARD_STOP_TX);

/// 启动剪贴板监听器
pub fn start_clipboard_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    let state_for_event = state.clone();
    let app_for_event = app_handle.clone();

    POLLER.start(
        move || {
            // on_event: 检测到剪贴板唤醒事件时处理
            let current_content = read_clipboard_text(&app_for_event);
            if let Some(content) = current_content {
                if !content.is_empty() {
                    add_to_clipboard_history(&app_for_event, content, state_for_event.clone());
                }
            }
        },
        move || {
            // on_poll: 每次轮询检查是否应该处理
            let state_guard = lock_arc_mutex(&state);
            if !state_guard.settings.text_clipboard_enabled {
                return false;
            }
            if state_guard.is_updating_clipboard {
                return false;
            }
            let is_processing_selection = state_guard.is_processing_selection;
            if is_processing_selection {
                let allow = crate::features::text_selection::should_allow_clipboard_listener();
                return allow;
            }
            true
        },
    );
}

pub fn stop_clipboard_listener() {
    POLLER.stop();
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

    // Single lock acquisition for all state reads to reduce contention
    let (manager_arc, should_emit, allow) = {
        let state_guard = lock_arc_mutex(&state);
        let is_processing = state_guard.is_processing_selection;
        let allow = is_processing
            && crate::features::text_selection::should_allow_clipboard_listener();

        // Skip if processing selection and manual copy not detected
        if is_processing && !allow {
            log::debug!("正在进行划词操作且未检测到手动复制，跳过添加到历史记录");
            return;
        }

        (
            state_guard.clipboard_manager.clone(),
            state_guard.is_visible,
            allow,
        )
    };

    if allow {
        log::debug!("检测到划词期间的手动复制，允许添加到历史记录");
        // Clear flag outside lock to avoid potential deadlock
        crate::features::text_selection::clear_manual_copy_flag();
    }

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
        if let Err(e) = app_handle.emit("clipboard-history-item-updated", payload) {
            log::warn!("发送剪贴板历史更新事件失败: {}", e);
        }
    }
}
