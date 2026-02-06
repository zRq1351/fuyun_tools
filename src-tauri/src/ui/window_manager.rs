use crate::core::app_state::AppState;
use lazy_static::lazy_static;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_positioner::{Position, WindowExt};

lazy_static! {
    pub static ref ENIGO_INSTANCE: Arc<Mutex<Option<enigo::Enigo>>> = Arc::new(Mutex::new(None));
}

/// 清理ENIGO实例资源
pub fn cleanup_enigo_instance() {
    let mut enigo_guard = ENIGO_INSTANCE.lock().unwrap();
    *enigo_guard = None;
    log::info!("已清理ENIGO实例资源");
}

/// 显示剪贴板窗口
pub fn show_clipboard_window(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    {
        let state_guard = state.lock().unwrap();
        if state_guard.is_visible {
            return;
        }
    }

    {
        let mut state_guard = state.lock().unwrap();
        state_guard.is_visible = true;
    }

    let selected_index = {
        let state_guard = state.lock().unwrap();
        state_guard.selected_index
    };

    let history = {
        let state_guard = state.lock().unwrap();
        let manager = state_guard.clipboard_manager.lock().unwrap();
        manager.get_history()
    };

    if let Some(_window) = app_handle.get_webview_window("clipboard") {
        let app_handle_clone = app_handle.clone();
        let history_clone = history.clone();
        thread::spawn(move || {
            if let Some(window) = app_handle_clone.get_webview_window("clipboard") {
                set_window_position(&window);
                if window.show().is_ok() {
                    let _ = window.set_focus();
                    let payload = serde_json::json!({
                        "history": history_clone,
                        "selectedIndex": selected_index
                    });
                    let _ = app_handle_clone.emit("show-window", payload);
                }
            }
        });
    }
}

/// 隐藏剪贴板窗口
pub fn hide_clipboard_window(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    let is_visible = {
        let state_guard = state.lock().unwrap();
        state_guard.is_visible
    };

    if !is_visible {
        return;
    }

    if let Some(window) = app_handle.get_webview_window("clipboard") {
        let _ = window.hide();
    }
    {
        let mut state_guard = state.lock().unwrap();
        state_guard.is_visible = false;
        state_guard.selected_index = 0;
        state_guard.is_processing_selection = false;
    }
}

/// 设置窗口位置和大小
pub fn set_window_position(window: &tauri::WebviewWindow) {
    if let Some(monitor) = window.current_monitor().unwrap() {
        let screen_size = monitor.size();

        let window_width = screen_size.width;
        let window_height = 250u32;

        let _ = window.set_size(tauri::LogicalSize::new(window_width, window_height));

        let _ = window.move_window(Position::BottomLeft);
    }
}

/// 打开划词工具栏
pub fn show_selection_toolbar_impl(app_handle: AppHandle, selected_text: String) {
    if let Some(toolbar_window) = app_handle.get_webview_window("selection_toolbar") {
        set_toolbar_window(&toolbar_window);
        if toolbar_window.show().is_ok() {
            if let Err(e) = app_handle.emit("selected-text", selected_text) {
                log::error!("未能发送选择文本到前端:{}", e);
            }
        }
    }
}

/// 设置工具栏窗口位置
fn set_toolbar_window(window: &tauri::WebviewWindow) {
    let _ = window.set_size(tauri::LogicalSize::new(50, 130));
    let _ = window.move_window(Position::RightCenter);
}

/// 隐藏工具栏窗口
pub fn hide_selection_toolbar_impl(app_handle: AppHandle) {
    if let Some(toolbar_window) = app_handle.get_webview_window("selection_toolbar") {
        if let Ok(is_visible) = toolbar_window.is_visible() {
            if is_visible {
                if let Ok(has_focus) = toolbar_window.is_focused() {
                    if !has_focus {
                        let _ = toolbar_window.hide();
                    }
                }
            }
        }
    }
}

/// 模拟粘贴操作
pub fn simulate_paste() {
    use crate::core::config::CTRL_KEY;
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    {
        let mut enigo_guard = ENIGO_INSTANCE.lock().unwrap();
        if enigo_guard.is_none() {
            *enigo_guard = Some(Enigo::new(&Settings::default()).expect("未能初始化enigo"));
        }

        if let Some(ref mut enigo) = *enigo_guard {
            let _ = enigo.key(CTRL_KEY, Direction::Press);
            let _ = enigo.key(Key::Unicode('v'), Direction::Click);
            thread::sleep(Duration::from_millis(100));
            let _ = enigo.key(CTRL_KEY, Direction::Release);
        }
    }
}

/// 显示结果窗口
pub async fn show_result_window(
    title: String,
    content: String,
    window_type: String,
    original: String,
    app: AppHandle,
) -> Result<(), String> {
    let window_label = format!("result_{}", window_type);

    if let Some(existing_window) = app.get_webview_window(&window_label) {
        if let Ok(is_visible) = existing_window.is_visible() {
            if !is_visible {
                let _ = existing_window.show();
            }
        } else {
            let _ = existing_window.show();
        }

        let _ = existing_window.set_focus();

        let payload = serde_json::json!({
            "type": window_type.clone(),
            "original": original.clone(),
            "content": content.clone()
        });
        let script = format!("window.__INITIAL_DATA__ = {}; window.dispatchEvent(new Event('init-data'));", payload);
        let _ = existing_window.eval(&script);

        return Ok(());
    }

    // 创建新窗口
    let window = tauri::WebviewWindowBuilder::new(
        &app,
        &window_label,
        tauri::WebviewUrl::App("result_display.html".into()),
    )
        .title(&title)
        .visible(false)
        .inner_size(480.0, 300.0)
        .resizable(true)
        .decorations(true)
        .on_page_load(move |window, _| {
            let payload = serde_json::json!({
            "type": window_type.clone(),
            "original": original.clone(),
            "content": content.clone()
        });
            let script = format!("window.__INITIAL_DATA__ = {};", payload);
            let _ = window.eval(&script);
        })
        .build()
        .map_err(|e| format!("创建窗口失败: {}", e))?;

    let _ = window.move_window(Position::RightCenter);
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

/// 更新结果窗口
pub async fn update_result_window(
    content: String,
    window_type: String,
    app: AppHandle,
) -> Result<(), String> {
    let window_label = format!("result_{}", window_type);
    if let Some(window) = app.get_webview_window(&window_label) {
        let payload = serde_json::json!({
            "content": content
        });
        match window.emit("result-update", payload) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("发送数据失败: {}", e)),
        }
    } else {
        log::error!("{}窗口不存在", &window_type);
        Err("窗口不存在".to_string())
    }
}