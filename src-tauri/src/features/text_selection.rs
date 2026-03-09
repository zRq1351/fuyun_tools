use crate::ui::window_manager::ENIGO_INSTANCE;
use crate::utils::clipboard::ClipboardManager;
use enigo::{Enigo, Key, Keyboard, Settings};
use log;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

#[cfg(target_os = "windows")]
use winapi::um::winuser::GetClipboardSequenceNumber;

/// 划词捕获最大重试时长
const CAPTURE_RETRY_MAX_DURATION: Duration = Duration::from_millis(600);
/// 轮询间隔，使用序列号检测时可以更频繁
const CAPTURE_RETRY_INTERVAL: Duration = Duration::from_millis(10);
/// 模拟按键后的初始等待时间
const INITIAL_DELAY: Duration = Duration::from_millis(10);

use crate::core::app_state::AppState as SharedAppState;
use crate::core::config::CTRL_KEY;
use tauri::Manager;

/// 获取选中的文本
pub fn get_selected_text_with_app(
    app_handle: &AppHandle,
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
) -> Option<String> {
    get_selected_text_windows(app_handle, clipboard_manager)
}

/// 获取当前剪贴板序列号 (Windows only)
#[cfg(target_os = "windows")]
fn get_clipboard_seq_num() -> Option<u32> {
    unsafe {
        let seq = GetClipboardSequenceNumber();
        if seq != 0 {
            Some(seq)
        } else {
            None
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn get_clipboard_seq_num() -> Option<u32> {
    None
}

/// Windows平台获取选中文本实现
fn get_selected_text_windows(
    app_handle: &AppHandle,
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
) -> Option<String> {
    let state_manager = app_handle.state::<Arc<Mutex<SharedAppState>>>();

    {
        let mut state = state_manager.lock().unwrap();
        if !state.settings.selection_enabled {
            return None;
        }
        state.is_processing_selection = true;
    }

    // 1. 获取原始剪贴板内容（用于后续恢复）
    let original_content =
        get_current_clipboard_content_with_manager(&clipboard_manager, app_handle);

    // 2. 获取当前剪贴板序列号
    let start_seq = get_clipboard_seq_num();

    // 3. 模拟 Ctrl+C
    let mut enigo_guard = ENIGO_INSTANCE.lock().unwrap();
    if enigo_guard.is_none() {
        *enigo_guard = Some(Enigo::new(&Settings::default()).expect("未能初始化enigo"));
    }

    crate::features::mouse_listener::reset_ctrl_key_state();

    if let Some(ref mut enigo) = *enigo_guard {
        let _ = enigo.key(CTRL_KEY, enigo::Direction::Press);
        let _ = enigo.key(Key::Unicode('c'), enigo::Direction::Click);
        // 减少按键保持时间，提高响应速度
        thread::sleep(Duration::from_millis(20));
        let _ = enigo.key(CTRL_KEY, enigo::Direction::Release);
    }

    log::info!("已发送Ctrl+C模拟按键");

    thread::sleep(INITIAL_DELAY);
    crate::features::mouse_listener::reset_ctrl_key_state();

    // 4. 等待剪贴板更新并获取新内容
    let new_content = wait_for_clipboard_update(
        &clipboard_manager,
        app_handle,
        start_seq,
        &original_content,
    );

    // 5. 恢复原始剪贴板内容
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

/// 使用管理器获取当前剪贴板内容
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

/// 等待剪贴板更新
///
/// 优先使用 Windows 剪贴板序列号检测变化，如果不支持则回退到内容轮询
fn wait_for_clipboard_update(
    clipboard_manager: &Arc<Mutex<ClipboardManager>>,
    app_handle: &AppHandle,
    start_seq: Option<u32>,
    original_content: &Option<String>,
) -> Option<String> {
    let start_time = std::time::Instant::now();
    let mut attempts = 0;

    // 如果能获取到序列号，使用序列号检测模式
    if let Some(initial_seq) = start_seq {
        log::info!("使用剪贴板序列号检测模式 (初始Seq: {})", initial_seq);
        while start_time.elapsed() < CAPTURE_RETRY_MAX_DURATION {
            thread::sleep(CAPTURE_RETRY_INTERVAL);

            if let Some(current_seq) = get_clipboard_seq_num() {
                if current_seq != initial_seq {
                    log::info!("检测到剪贴板序列号变化 ({} -> {})，耗时: {:?}", 
                              initial_seq, current_seq, start_time.elapsed());

                    // 序列号变化后，稍作等待确保写入完成
                    thread::sleep(Duration::from_millis(10));
                    return get_current_clipboard_content_with_manager(clipboard_manager, app_handle);
                }
            }
        }
        log::debug!("剪贴板序列号未变化，超时退出");
        return None;
    }

    // 回退模式：内容轮询（适用于无法获取序列号的情况）
    log::info!("回退到内容轮询检测模式");
    const SAME_CONTENT_THRESHOLD: Duration = Duration::from_millis(200);
    
    while start_time.elapsed() < CAPTURE_RETRY_MAX_DURATION {
        attempts += 1;
        thread::sleep(CAPTURE_RETRY_INTERVAL);

        let current_content = get_current_clipboard_content_with_manager(clipboard_manager, app_handle);

        if let Some(ref current) = current_content {
            if let Some(ref original) = original_content {
                if current != original {
                    log::info!("第{}次尝试成功捕获内容，耗时: {:?}", 
                              attempts, start_time.elapsed());
                    return current_content;
                } else if start_time.elapsed() > SAME_CONTENT_THRESHOLD {
                    // 如果超过阈值内容仍未变，假设用户复制了相同内容
                    log::info!("内容未变化，假设为相同内容复制，耗时: {:?}", start_time.elapsed());
                    return current_content;
                }
            } else {
                if !current.is_empty() {
                    log::info!("第{}次尝试成功捕获新内容，耗时: {:?}", 
                              attempts, start_time.elapsed());
                    return current_content;
                }
            }
        }
    }

    log::debug!("重试{}次后仍未捕获到新内容，总耗时: {:?}",
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
    let current_content = get_current_clipboard_content_with_manager(clipboard_manager, app_handle);

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
            log::info!("当前剪贴板为空，已放弃恢复原始内容");
        }
    } else {
        log::debug!("未捕获到内容，无需恢复");
    }
}
