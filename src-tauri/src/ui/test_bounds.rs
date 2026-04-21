use tauri::{AppHandle, Manager, WebviewWindow};
pub fn test_bounds(window: &WebviewWindow) {
    let _ = window.set_bounds(tauri::PhysicalRect {
        x: 0,
        y: 0,
        width: 100,
        height: 100,
    });
}
