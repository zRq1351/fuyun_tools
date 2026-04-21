use tauri::{AppHandle, Manager, WebviewWindow};

fn check(window: &WebviewWindow) {
    let _ = window.set_bounds(tauri::WindowBounds::default());
}
