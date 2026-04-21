use tauri::{AppHandle, Manager, WebviewWindow};

fn test(window: &WebviewWindow) {
    let _ = window.set_bounds(tauri::PhysicalRect {
        x: 0,
        y: 0,
        width: 100,
        height: 100,
    });
}
