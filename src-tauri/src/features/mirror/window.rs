//! 投屏窗口（主窗口=纯画面）+ 控制条小窗口（贴在主窗口右侧，承载全部按钮）。

use tauri::{AppHandle, Manager};

const LABEL: &str = "mirror";
const CONTROLS_LABEL: &str = "mirror-controls";

/// 创建（如不存在）并显示投屏主窗口与控制条窗口。
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
    // 控制条窗口贴在主窗口右侧
    if let Ok(controls) = ensure_controls_window(app) {
        position_controls_window(app);
        let _ = controls.show();
    }
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
            .decorations(false) // 无边框：控制条小窗口承担最小化/关闭
            .resizable(true)
            .minimizable(true)
            .maximizable(true)
            .inner_size(432.0, 912.0)
            .min_inner_size(288.0, 288.0) // 允许横屏
            .build()
            .map_err(|e| format!("创建投屏窗口失败: {}", e))?;
    bind_mirror_window_events(&window, app.clone());
    Ok(window)
}

/// 控制条窗口：无边框透明，置顶不进任务栏，尺寸由主窗口位置/尺寸驱动。
fn ensure_controls_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(existing) = app.get_webview_window(CONTROLS_LABEL) {
        return Ok(existing);
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        CONTROLS_LABEL,
        tauri::WebviewUrl::App("mirror_controls.html".into()),
    )
    .title("")
    .visible(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .minimizable(false)
    .maximizable(false)
    .inner_size(56.0, 400.0)
    .build()
    .map_err(|e| format!("创建控制条窗口失败: {}", e))?;
    Ok(window)
}

/// 把控制条窗口贴到主窗口右缘外侧，高度随主窗口。
fn position_controls_window(app: &AppHandle) {
    let (Some(main), Some(controls)) = (
        app.get_webview_window(LABEL),
        app.get_webview_window(CONTROLS_LABEL),
    ) else {
        return;
    };
    let Ok(pos) = main.outer_position() else {
        return;
    };
    let Ok(size) = main.outer_size() else {
        return;
    };
    let scale = main.scale_factor().unwrap_or(1.0);
    let ctl_w = (56.0 * scale).round() as u32;
    let _ = controls.set_position(tauri::PhysicalPosition::new(
        pos.x + size.width as i32,
        pos.y,
    ));
    let _ = controls.set_size(tauri::PhysicalSize::new(ctl_w, size.height));
}

/// 主窗口事件：移动/缩放 → 跟随控制条；最小化 → 隐藏控制条；销毁 → 关闭控制条。
fn bind_mirror_window_events(window: &tauri::WebviewWindow, app: AppHandle) {
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::Moved(pos) => {
            // 最小化时 Windows 把窗口移到 -32000：此时隐藏控制条
            if pos.y < -20000 {
                if let Some(controls) = app.get_webview_window(CONTROLS_LABEL) {
                    let _ = controls.hide();
                }
            } else {
                position_controls_window(&app);
            }
        }
        tauri::WindowEvent::Resized(_) => {
            position_controls_window(&app);
        }
        tauri::WindowEvent::Focused(true) => {
            position_controls_window(&app);
            if let Some(controls) = app.get_webview_window(CONTROLS_LABEL) {
                if !controls.is_visible().unwrap_or(true) {
                    let _ = controls.show();
                }
            }
        }
        tauri::WindowEvent::Destroyed => {
            on_mirror_window_destroyed(&app);
        }
        _ => {}
    });
}

/// 投屏主窗口被用户关闭时：关闭控制条窗口并停止会话。
pub fn on_mirror_window_destroyed(app: &AppHandle) {
    if let Some(controls) = app.get_webview_window(CONTROLS_LABEL) {
        let _ = controls.close();
    }
    super::commands::stop_session(app);
}
