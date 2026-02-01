use crate::{ClipboardManager, ENIGO_INSTANCE};
use enigo::{Enigo, Key, Keyboard, Settings};
use log;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

pub use crate::AppState as SharedAppState;

use crate::config::CTRL_KEY;
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

    let mut enigo_guard = ENIGO_INSTANCE.lock().unwrap();
    if enigo_guard.is_none() {
        *enigo_guard = Some(Enigo::new(&Settings::default()).expect("未能初始化enigo"));
    }

    crate::mouse_listener::reset_ctrl_key_state();

    if let Some(ref mut enigo) = *enigo_guard {
        let _ = enigo.key(CTRL_KEY, enigo::Direction::Press);
        let _ = enigo.key(Key::Unicode('c'), enigo::Direction::Click);
        thread::sleep(Duration::from_millis(100));
        let _ = enigo.key(CTRL_KEY, enigo::Direction::Release);
    }

    log::info!("已发送Ctrl+C模拟按键");

    thread::sleep(Duration::from_millis(50));
    crate::mouse_listener::reset_ctrl_key_state();

    thread::sleep(Duration::from_millis(150));

    let new_content = get_current_clipboard_content_with_manager(&clipboard_manager, app_handle);

    if new_content == original_content {
        log::info!("剪贴板内容没有改变，取消获取选中文本");
        return None;
    }

    if let Some(ref original) = original_content {
        set_original_clipboard_content_back_with_manager(&clipboard_manager, app_handle, original);
    }

    {
        let mut state = state_manager.lock().unwrap();
        state.is_processing_selection = false;
    }

    log::info!("完成使用模拟Ctrl+C获取选中文本");
    new_content
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

fn set_original_clipboard_content_back_with_manager(
    clipboard_manager: &Arc<Mutex<ClipboardManager>>,
    app_handle: &AppHandle,
    content: &str,
) {
    let result = {
        let manager = clipboard_manager.lock().unwrap();
        manager.set_clipboard_content(app_handle, content)
    };

    match result {
        Ok(()) => log::debug!("已恢复原始剪贴板内容"),
        Err(e) => log::error!("恢复剪贴板内容失败: {}", e),
    }
}
