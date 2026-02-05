use crate::ui::window_manager::ENIGO_INSTANCE;
use crate::utils::clipboard::ClipboardManager;
use enigo::{Enigo, Key, Keyboard, Settings};
use log;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

/// 划词捕获最大重试时长
const CAPTURE_RETRY_MAX_DURATION: Duration = Duration::from_millis(2000);
const CAPTURE_RETRY_INTERVAL: Duration = Duration::from_millis(50);
const INITIAL_DELAY: Duration = Duration::from_millis(50);

use crate::core::app_state::AppState as SharedAppState;
use crate::core::config::CTRL_KEY;
use tauri::Manager;

pub fn get_selected_text_with_app(
    app_handle: &AppHandle,
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
) -> Option<String> {
    get_selected_text_windows(app_handle, clipboard_manager)
}

fn get_selected_text_windows(
    app_handle: &AppHandle,
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
) -> Option<String> {
    let state_manager = app_handle.state::<Arc<Mutex<SharedAppState>>>();

    {
        let mut state = state_manager.lock().unwrap();
        state.is_processing_selection = true;
    }

    let original_content =
        get_current_clipboard_content_with_manager(&clipboard_manager, app_handle);

    // 初始化enigo
    let mut enigo_guard = ENIGO_INSTANCE.lock().unwrap();
    if enigo_guard.is_none() {
        *enigo_guard = Some(Enigo::new(&Settings::default()).expect("未能初始化enigo"));
    }

    crate::features::mouse_listener::reset_ctrl_key_state();

    // 发送Ctrl+C模拟按键
    if let Some(ref mut enigo) = *enigo_guard {
        let _ = enigo.key(CTRL_KEY, enigo::Direction::Press);
        let _ = enigo.key(Key::Unicode('c'), enigo::Direction::Click);
        thread::sleep(Duration::from_millis(100));
        let _ = enigo.key(CTRL_KEY, enigo::Direction::Release);
    }

    log::info!("已发送Ctrl+C模拟按键");

    thread::sleep(INITIAL_DELAY);
    crate::features::mouse_listener::reset_ctrl_key_state();

    // 使用重试机制捕获剪贴板内容
    let new_content = retry_capture_clipboard_content(
        &clipboard_manager,
        app_handle,
        &original_content,
    );

    // 安全地恢复原始剪贴板内容
    if let Some(ref original) = original_content {
        safe_restore_clipboard_content(&clipboard_manager, app_handle, original, &new_content);
    }

    {
        let mut state = state_manager.lock().unwrap();
        state.is_processing_selection = false;
    }

    match &new_content {
        Some(content) => {
            log::info!("成功捕获选中文本，长度: {}", content.len());
            new_content
        },
        None => {
            log::warn!("未能捕获选中文本");
            None
        }
    }
}

fn get_current_clipboard_content_with_manager(
    clipboard_manager: &Arc<Mutex<ClipboardManager>>,
    app_handle: &AppHandle,
) -> Option<String> {
    let content = {
        let manager = clipboard_manager.lock().unwrap();
        manager.get_content(app_handle)
    };

    match &content {
        Some(text) => log::debug!("从剪贴板读取内容: {}", text),
        None => log::debug!("剪贴板中没有文本内容"),
    }

    content
}

/// 重试机制捕获剪贴板内容
fn retry_capture_clipboard_content(
    clipboard_manager: &Arc<Mutex<ClipboardManager>>,
    app_handle: &AppHandle,
    original_content: &Option<String>,
) -> Option<String> {
    let start_time = std::time::Instant::now();
    let mut attempts = 0;

    while start_time.elapsed() < CAPTURE_RETRY_MAX_DURATION {
        attempts += 1;
        thread::sleep(CAPTURE_RETRY_INTERVAL);

        let current_content = get_current_clipboard_content_with_manager(clipboard_manager, app_handle);

        // 检查是否有新的内容
        if let Some(ref current) = current_content {
            if let Some(ref original) = original_content {
                if current != original {
                    log::info!("第{}次尝试成功捕获内容，耗时: {:?}", 
                              attempts, start_time.elapsed());
                    return current_content;
                }
            } else {
                // 原始内容为空，只要捕获到非空内容就算成功
                if !current.is_empty() {
                    log::info!("第{}次尝试成功捕获新内容，耗时: {:?}", 
                              attempts, start_time.elapsed());
                    return current_content;
                }
            }
        }
    }

    log::warn!("重试{}次后仍未捕获到新内容，总耗时: {:?}", 
               attempts, start_time.elapsed());
    None
}

/// 安全地恢复原始剪贴板内容
fn safe_restore_clipboard_content(
    clipboard_manager: &Arc<Mutex<ClipboardManager>>,
    app_handle: &AppHandle,
    original_content: &str,
    captured_content: &Option<String>,
) {
    // 在恢复前再次检查当前剪贴板内容
    let current_content = get_current_clipboard_content_with_manager(clipboard_manager, app_handle);

    // 只有当当前内容与我们捕获的内容相同时才恢复
    if let Some(ref captured) = captured_content {
        if let Some(ref current) = current_content {
            if current == captured {
                let result = {
                    let manager = clipboard_manager.lock().unwrap();
                    manager.set_clipboard_content(app_handle, original_content)
                };

                match result {
                    Ok(()) => log::debug!("已安全恢复原始剪贴板内容"),
                    Err(e) => log::error!("恢复剪贴板内容失败: {}", e),
                }
            } else {
                log::info!("检测到剪贴板在捕获后被用户更改，已放弃恢复原始内容以避免覆盖用户操作");
            }
        } else {
            // 当前剪贴板为空，可能是用户清空了，也放弃恢复
            log::info!("当前剪贴板为空，已放弃恢复原始内容");
        }
    } else {
        // 没有捕获到内容，不需要恢复
        log::debug!("未捕获到内容，无需恢复");
    }
}
