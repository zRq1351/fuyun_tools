//! 投屏窗口：普通桌面窗口（非 overlay），按手机竖屏比例初始化。

use tauri::{AppHandle, Manager};

const LABEL: &str = "mirror";

/// 创建（如不存在）并显示投屏窗口。
pub fn show_mirror_window(app: &AppHandle) -> Result<(), String> {
    let window = ensure_mirror_window(app)?;
    if window.is_minimized().unwrap_or(false) {
        let _ = window.unminimize();
    }
    window
        .show()
        .map_err(|e| format!("显示投屏窗口失败: {}", e))?;
    window
        .set_focus()
        .map_err(|e| format!("聚焦投屏窗口失败: {}", e))?;
    Ok(())
}

pub fn ensure_mirror_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(existing) = app.get_webview_window(LABEL) {
        return Ok(existing);
    }
    let window =
        tauri::WebviewWindowBuilder::new(app, LABEL, tauri::WebviewUrl::App("mirror.html".into()))
            .title("手机投屏")
            .visible(false)
            .decorations(false) // 无边框：前端工具栏承担标题栏（拖拽/最小化/最大化/关闭）
            .resizable(true)
            .minimizable(true)
            .maximizable(true)
            .inner_size(432.0, 912.0)
            .min_inner_size(288.0, 288.0) // 允许横屏（视频全屏时窗口自动横竖切换）
            .build()
            .map_err(|e| format!("创建投屏窗口失败: {}", e))?;
    bind_mirror_window_events(&window, app.clone());
    Ok(window)
}

/// 用户关闭投屏窗口 → 停止会话（下次开启重新建会话）。
fn bind_mirror_window_events(window: &tauri::WebviewWindow, app: AppHandle) {
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            on_mirror_window_destroyed(&app);
        }
    });
}

/// 投屏窗口被用户关闭时，停止会话（下次开启重新建会话）。
pub fn on_mirror_window_destroyed(app: &AppHandle) {
    super::commands::stop_session(app);
}
