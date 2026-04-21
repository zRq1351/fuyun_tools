use tauri::WebviewWindow;
fn dummy(w: &WebviewWindow) {
    if let Ok(hwnd) = w.hwnd() {
        let _ = hwnd.0;
    }
}
