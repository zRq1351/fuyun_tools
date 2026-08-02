use crate::core::app_state::{AppState, ForegroundTargetSnapshot, OverlayLifecycleRecord};
use crate::core::config::{CLIPBOARD_WINDOW_BOTTOM_EXTRA_MARGIN, CTRL_KEY};
use crate::core::error_codes::AppErrorKind;
use crate::sync::{lock_arc_mutex, Mutex};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use std::sync::{Arc, Condvar, LazyLock, Mutex as StdMutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_positioner::{Position, WindowExt};
#[cfg(target_os = "windows")]
use windows::core::BOOL;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::GetCurrentProcessId;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, KEYEVENTF_KEYUP, VK_CONTROL, VK_LCONTROL, VK_RCONTROL,
};
#[cfg(target_os = "windows")]
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetSystemMetrics, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, SetForegroundWindow, ShowWindow,
    SystemParametersInfoW, SM_CYSCREEN, SPI_GETWORKAREA, SW_RESTORE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

pub static ENIGO_INSTANCE: LazyLock<Arc<Mutex<Option<enigo::Enigo>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(None)));
static WINDOW_VISIBILITY_NOTIFY: LazyLock<Arc<(StdMutex<u64>, Condvar)>> =
    LazyLock::new(|| Arc::new((StdMutex::new(0), Condvar::new())));

pub fn destroy_window_by_label(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        log::info!("[窗口销毁] 强制销毁: {}", label);
        let _ = window.destroy();
        if let Some(state) = app.try_state::<Arc<Mutex<AppState>>>() {
            let mut guard = lock_arc_mutex(state.inner());
            if guard.active_overlay_window.as_deref() == Some(label) {
                guard.active_overlay_window = None;
            }
            match label {
                "clipboard" => guard.is_visible = false,
                "image_clipboard" => guard.is_image_visible = false,
                _ => {}
            }
        }
    } else {
        log::warn!("[窗口销毁] 未找到窗口: {}", label);
    }
}

fn notify_window_visibility_changed() {
    let (lock, cvar) = &**WINDOW_VISIBILITY_NOTIFY;
    let mut seq = lock.lock().unwrap_or_else(|poisoned| {
        log::error!("窗口可见性通知锁中毒，尝试恢复");
        poisoned.into_inner()
    });
    *seq = seq.wrapping_add(1);
    cvar.notify_all();
}

fn set_active_overlay_window(app_handle: &AppHandle, label: Option<&str>) {
    let Some(state) = app_handle.try_state::<Arc<Mutex<AppState>>>() else {
        return;
    };
    let mut guard = lock_arc_mutex(state.inner());
    guard.active_overlay_window = label.map(|value| value.to_string());
}

fn emit_overlay_window_lifecycle(app_handle: &AppHandle, label: &str, action: &str, focused: bool) {
    if let Some(state) = app_handle.try_state::<Arc<Mutex<AppState>>>() {
        let mut guard = lock_arc_mutex(state.inner());
        let record = OverlayLifecycleRecord {
            label: label.to_string(),
            action: action.to_string(),
            focused,
            occurred_at: now_ms(),
        };
        guard.last_overlay_lifecycle = Some(record.clone());
    guard.overlay_lifecycle_history.push_back(record);
    if guard.overlay_lifecycle_history.len() > 6 {
        guard.overlay_lifecycle_history.pop_front();
    }
    }
    if let Err(e) = app_handle.emit(
        "overlay-window-lifecycle",
        serde_json::json!({
            "label": label,
            "action": action,
            "focused": focused,
        }),
    ) {
        log::warn!("发送覆盖窗口生命周期事件失败: {}", e);
    }
}

fn show_overlay_window(
    app_handle: &AppHandle,
    label: &str,
    window: &tauri::WebviewWindow,
    focus: bool,
) -> bool {
    if window.show().is_err() {
        return false;
    }
    if focus {
        let _ = window.set_focus();
        set_active_overlay_window(app_handle, Some(label));
    } else {
        set_active_overlay_window(app_handle, Some(label));
    }
    emit_overlay_window_lifecycle(app_handle, label, "shown", focus);
    true
}

fn hide_overlay_window(app_handle: &AppHandle, label: &str, window: &tauri::WebviewWindow) {
    let _ = window.hide();
    let should_clear = app_handle
        .try_state::<Arc<Mutex<AppState>>>()
        .map(|state| {
            let guard = lock_arc_mutex(state.inner());
            guard.active_overlay_window.as_deref() == Some(label)
        })
        .unwrap_or(false);
    if should_clear {
        set_active_overlay_window(app_handle, None);
    }
    emit_overlay_window_lifecycle(app_handle, label, "hidden", false);
}

pub fn bind_overlay_window_events(
    window: &tauri::WebviewWindow,
    app_handle: AppHandle,
    label: impl Into<String>,
) {
    let label = label.into();
    let window_clone = window.clone();
    // 判断是否为结果窗口（翻译/解释/自定义提示词）
    let is_result_window = label.starts_with("result_");
    
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::CloseRequested { api, .. } => {
            if !is_result_window {
                api.prevent_close();
                hide_overlay_window(&app_handle, &label, &window_clone);
            }
        }
        tauri::WindowEvent::Destroyed => {
            let (should_clear, visibility_cleared) = app_handle
                .try_state::<Arc<Mutex<AppState>>>()
                .map(|state| {
                    let mut guard = lock_arc_mutex(state.inner());
                    let clear = guard.active_overlay_window.as_deref() == Some(label.as_str());
                    if clear {
                        guard.active_overlay_window = None;
                    }
                    let cleared = match label.as_str() {
                        "clipboard" => { guard.is_visible = false; true }
                        "image_clipboard" => { guard.is_image_visible = false; true }
                        _ => false,
                    };
                    let record = OverlayLifecycleRecord {
                        label: label.clone(),
                        action: "destroyed".to_string(),
                        focused: false,
                        occurred_at: now_ms(),
                    };
                    guard.last_overlay_lifecycle = Some(record.clone());
                    guard.overlay_lifecycle_history.push_back(record);
                    if guard.overlay_lifecycle_history.len() > 6 {
                        guard.overlay_lifecycle_history.pop_front();
                    }
                    (clear, cleared)
                })
                .unwrap_or((false, false));
            if visibility_cleared {
                notify_window_visibility_changed();
            }
            if should_clear {
                let _ = app_handle.emit(
                    "overlay-window-lifecycle",
                    serde_json::json!({
                        "label": label,
                        "action": "destroyed",
                        "focused": false,
                    }),
                );
            }
        }
        _ => {}
    });
}

pub fn show_overlay_window_by_label(
    app_handle: &AppHandle,
    label: &str,
    focus: bool,
) -> Result<(), String> {
    ensure_window_for_label(app_handle, label)?;
    let window = app_handle
        .get_webview_window(label)
        .ok_or_else(|| format!("窗口不存在: {}", label))?;
    if show_overlay_window(app_handle, label, &window, focus) {
        Ok(())
    } else {
        Err(format!("显示窗口失败: {}", label))
    }
}

pub fn hide_overlay_window_by_label(app_handle: &AppHandle, label: &str) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window(label) {
        hide_overlay_window(app_handle, label, &window);
    }
    Ok(())
}

pub fn focus_overlay_window_by_label(app_handle: &AppHandle, label: &str) -> Result<(), String> {
    let window = app_handle
        .get_webview_window(label)
        .ok_or_else(|| format!("窗口不存在: {}", label))?;
    window
        .set_focus()
        .map_err(|e| format!("设置窗口焦点失败 {}: {}", label, e))?;
    set_active_overlay_window(app_handle, Some(label));
    emit_overlay_window_lifecycle(app_handle, label, "focused", true);
    Ok(())
}

pub fn bind_standard_window_close_to_hide(window: &tauri::WebviewWindow) {
    let window_clone = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window_clone.hide();
        }
    });
}

pub fn show_standard_window_by_label(app_handle: &AppHandle, label: &str) -> Result<(), String> {
    ensure_window_for_label(app_handle, label)?;
    let window = app_handle
        .get_webview_window(label)
        .ok_or_else(|| format!("窗口不存在: {}", label))?;
    window
        .show()
        .map_err(|e| format!("显示窗口失败 {}: {}", label, e))?;
    window
        .set_focus()
        .map_err(|e| format!("设置窗口焦点失败 {}: {}", label, e))?;
    Ok(())
}

/// 清理ENIGO实例资源
pub fn cleanup_enigo_instance() {
    let mut enigo_guard = lock_arc_mutex(&ENIGO_INSTANCE);
    *enigo_guard = None;
    log::info!("已清理ENIGO实例资源");
}

#[cfg(target_os = "windows")]
fn release_ctrl_key_winapi() {
    const FUYUN_MARKER: usize = 0x46555955;
    unsafe {
        keybd_event(VK_CONTROL.0 as u8, 0, KEYEVENTF_KEYUP, FUYUN_MARKER);
        keybd_event(VK_LCONTROL.0 as u8, 0, KEYEVENTF_KEYUP, FUYUN_MARKER);
        keybd_event(VK_RCONTROL.0 as u8, 0, KEYEVENTF_KEYUP, FUYUN_MARKER);
    }
}

#[cfg(not(target_os = "windows"))]
fn release_ctrl_key_winapi() {}

pub fn release_ctrl_key_with_fallback(enigo: &mut enigo::Enigo) -> Result<(), String> {
    use enigo::{Direction, Keyboard};
    let enigo_result = enigo
        .key(CTRL_KEY, Direction::Release)
        .map_err(|e| AppErrorKind::SelectionCtrlReleaseFailed.to_frontend_json_with_details(format!("{}", e)));
    // Windows 下额外补发通用/左右 Ctrl 的 keyup，尽量消除偶发“Ctrl 卡住”。
    release_ctrl_key_winapi();
    enigo_result
}

fn release_ctrl_key_once(enigo: &mut enigo::Enigo) -> Result<(), String> {
    release_ctrl_key_with_fallback(enigo)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForegroundWindowInfo {
    pub title: String,
    pub pid: u32,
    pub hwnd: isize,
}

pub fn remember_external_foreground_window(app_handle: &AppHandle) {
    let Some(state) = app_handle.try_state::<Arc<Mutex<AppState>>>() else {
        return;
    };
    let (is_fuyun, info) = foreground_window_info();
    if is_fuyun || info.pid == 0 {
        return;
    }
    let mut guard = lock_arc_mutex(state.inner());
    guard.last_external_foreground = Some(ForegroundTargetSnapshot {
        title: info.title,
        pid: info.pid,
        hwnd: info.hwnd,
    });
}

pub fn clear_external_foreground_snapshot(app_handle: &AppHandle) {
    let Some(state) = app_handle.try_state::<Arc<Mutex<AppState>>>() else {
        return;
    };
    let mut guard = lock_arc_mutex(state.inner());
    guard.last_external_foreground = None;
}

pub fn force_release_ctrl_key() -> Result<(), String> {
    use enigo::{Enigo, Settings};
    let mut last_error = None;
    for attempt in 0..2 {
        let release_result = {
            let mut enigo_guard = lock_arc_mutex(&ENIGO_INSTANCE);
            if enigo_guard.is_none() {
                *enigo_guard = Some(
                    Enigo::new(&Settings::default())
                        .map_err(|e| format!("初始化输入器失败: {}", e))?,
                );
            }
            let enigo = enigo_guard
                .as_mut()
                .ok_or_else(|| "输入器不可用".to_string())?;
            release_ctrl_key_once(enigo)
        };
        match release_result {
            Ok(_) => return Ok(()),
            Err(e) => {
                last_error = Some(e);
                if attempt < 1 {
                    thread::sleep(Duration::from_millis(8));
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| AppErrorKind::SelectionCtrlReleaseFailed.to_frontend_json()))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// 显示剪贴板窗口
pub fn show_clipboard_window(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    remember_external_foreground_window(&app_handle);
    if ensure_clipboard_window(&app_handle).is_err() {
        return;
    }
    let (selected_index, bottom_offset, manager_arc) = {
        let state_guard = lock_arc_mutex(&state);
        if state_guard.is_visible {
            return;
        }
        (
            state_guard.selected_index,
            state_guard.settings.clipboard_bottom_offset,
            state_guard.clipboard_manager.clone(),
        )
    };

    let (history_items, categories, category_list, pinned_items) = {
        let manager = lock_arc_mutex(&manager_arc);
        let history = manager.get_history();
        let items: Vec<serde_json::Value> = history
            .iter()
            .map(|content| {
                serde_json::json!({
                    "id": crate::utils::database::stable_history_item_id(content),
                    "content": content,
                })
            })
            .collect();
        (
            items,
            manager.get_categories(),
            manager.get_category_list(),
            manager.get_pinned_items(),
        )
    };

    let app_handle_clone = app_handle.clone();
    let history_clone = history_items.clone();
    let categories_clone = categories.clone();
    let category_list_clone = category_list.clone();
    let pinned_items_clone = pinned_items.clone();
    let state_clone = state.clone();
    thread::spawn(move || {
        if let Some(window) = app_handle_clone.get_webview_window("clipboard") {
            set_window_position(&window, bottom_offset);
            if show_overlay_window(&app_handle_clone, "clipboard", &window, true) {
                {
                    let mut guard = lock_arc_mutex(&state_clone);
                    guard.is_visible = true;
                    notify_window_visibility_changed();
                }
                let payload = serde_json::json!({
                    "history": history_clone,
                    "categories": categories_clone,
                    "category_list": category_list_clone,
                    "pinned_items": pinned_items_clone,
                    "bottomOffset": bottom_offset,
                    "selectedIndex": selected_index
                });
                let _ = app_handle_clone.emit("show-window", payload);
            }
        }
    });
}

pub fn show_image_clipboard_window(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    remember_external_foreground_window(&app_handle);
    if ensure_image_clipboard_window(&app_handle).is_err() {
        return;
    }
    let (already_visible, selected_index, bottom_offset, manager_arc, should_sync_history) = {
        let mut state_guard = lock_arc_mutex(&state);
        let already_visible = state_guard.is_image_visible;
        let should_sync_history = !already_visible && state_guard.image_history_dirty;
        if should_sync_history {
            state_guard.image_history_dirty = false;
        }
        (
            already_visible,
            state_guard.image_selected_index,
            state_guard.settings.clipboard_bottom_offset,
            state_guard.image_clipboard_manager.clone(),
            should_sync_history,
        )
    };
    let snapshot_payload = if should_sync_history {
        let manager = {
            let guard = lock_arc_mutex(&manager_arc);
            guard.clone()
        };
        Some(serde_json::json!({
            "history": manager.get_history_preview(),
            "categories": manager.get_categories(),
            "category_list": manager.get_category_list(),
            "image_tags": manager.get_image_tags(),
            "pinned_items": manager.get_pinned_items(),
            "is_full_snapshot": true
        }))
    } else {
        None
    };

    let app_handle_clone = app_handle.clone();
    let snapshot_payload_clone = snapshot_payload.clone();
    let state_clone = state.clone();
    thread::spawn(move || {
        if let Some(window) = app_handle_clone.get_webview_window("image_clipboard") {
            set_window_position(&window, bottom_offset);
            if already_visible
                || show_overlay_window(&app_handle_clone, "image_clipboard", &window, true)
            {
                if !already_visible {
                    set_active_overlay_window(&app_handle_clone, Some("image_clipboard"));
                    let mut guard = lock_arc_mutex(&state_clone);
                    guard.is_image_visible = true;
                    notify_window_visibility_changed();
                }
                let mut payload = serde_json::json!({
                    "bottomOffset": bottom_offset,
                    "selectedIndex": selected_index
                });
                if let Some(snapshot) = snapshot_payload_clone {
                    if let Some(payload_obj) = payload.as_object_mut() {
                        if let Some(snapshot_obj) = snapshot.as_object() {
                            for (key, value) in snapshot_obj {
                                payload_obj.insert(key.clone(), value.clone());
                            }
                        }
                    }
                }
                let _ = app_handle_clone.emit("show-image-window", payload);
            }
        }
    });
}

/// 隐藏剪贴板窗口（通用实现）
fn hide_clipboard_window_impl(
    app_handle: AppHandle,
    state: Arc<Mutex<AppState>>,
    label: &str,
    is_visible_getter: impl Fn(&AppState) -> bool,
    set_visible: impl Fn(&mut AppState, bool),
    set_selected_index: impl Fn(&mut AppState, usize),
) {
    let is_visible = {
        let state_guard = lock_arc_mutex(&state);
        is_visible_getter(&state_guard)
    };

    if !is_visible {
        return;
    }

    if let Some(window) = app_handle.get_webview_window(label) {
        hide_overlay_window(&app_handle, label, &window);
    }
    {
        let mut state_guard = lock_arc_mutex(&state);
        set_visible(&mut state_guard, false);
        set_selected_index(&mut state_guard, 0);
    }
    notify_window_visibility_changed();
}

/// 隐藏剪贴板窗口
pub fn hide_clipboard_window(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    hide_clipboard_window_impl(
        app_handle, state, "clipboard",
        |s| s.is_visible,
        |s, v| s.is_visible = v,
        |s, v| s.selected_index = v,
    );
}

pub fn hide_image_clipboard_window(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    hide_clipboard_window_impl(
        app_handle, state, "image_clipboard",
        |s| s.is_image_visible,
        |s, v| s.is_image_visible = v,
        |s, v| s.image_selected_index = v,
    );
}

pub fn wait_for_window_hidden(
    _app_handle: &AppHandle,
    state: &Arc<Mutex<AppState>>,
    window_label: &str,
    timeout: Duration,
) -> Result<(), String> {
    let hidden = match window_label {
        "clipboard" => {
            let state_guard = lock_arc_mutex(state);
            !state_guard.is_visible
        }
        "image_clipboard" => {
            let state_guard = lock_arc_mutex(state);
            !state_guard.is_image_visible
        }
        _ => true,
    };
    if hidden {
        return Ok(());
    }
    let start = std::time::Instant::now();
    let (lock, cvar) = &**WINDOW_VISIBILITY_NOTIFY;
    let mut seq = lock.lock().unwrap_or_else(|poisoned| {
        log::error!("等待窗口隐藏时通知锁中毒，尝试恢复");
        poisoned.into_inner()
    });
    loop {
        let hidden = match window_label {
            "clipboard" => {
                let state_guard = lock_arc_mutex(state);
                !state_guard.is_visible
            }
            "image_clipboard" => {
                let state_guard = lock_arc_mutex(state);
                !state_guard.is_image_visible
            }
            _ => true,
        };
        if hidden {
            return Ok(());
        }
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Err(AppErrorKind::SelectionWaitHideTimeout.to_frontend_json_with_details(window_label.to_string()));
        }
        let remain = timeout.saturating_sub(elapsed);
        let wait_dur = std::cmp::min(remain, Duration::from_millis(80));
        match cvar.wait_timeout(seq, wait_dur) {
            Ok((next_seq, _)) => {
                seq = next_seq;
            }
            Err(poisoned) => {
                log::error!("等待窗口隐藏时条件变量异常（锁中毒），尝试恢复");
                let (next_seq, _) = poisoned.into_inner();
                seq = next_seq;
            }
        }
    }
}

pub fn show_image_preview_window(
    app_handle: AppHandle,
    request_id: String,
    image_path: String,
) -> Result<(), String> {
    let window = ensure_image_preview_window(&app_handle)?;
    prepare_image_preview_window(&window)?;

    let ext = std::path::Path::new(&image_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "gif" => "image/gif",
        _ => "image/png",
    };
    let base64 = std::fs::read(&image_path)
        .map(|bytes| BASE64.encode(&bytes))
        .unwrap_or_default();

    let payload = serde_json::json!({
        "request_id": request_id,
        "image_path": image_path,
        "base64": base64,
        "mime": mime,
        "is_final": true
    });
    let _ = window.set_always_on_top(false);
    let _ = show_overlay_window(&app_handle, "image_preview", &window, true);
    let _ = app_handle.emit("show-image-preview", payload.clone());
    if let Ok(payload_str) = serde_json::to_string(&payload) {
        let script = format!(
            "window.__IMAGE_PREVIEW_PAYLOAD__ = {payload_str};"
        );
        let _ = window.eval(&script);
    }
    Ok(())
}

fn prepare_image_preview_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    if let Some(monitor) = window.current_monitor().map_err(|e| e.to_string())? {
        let monitor_pos = monitor.position();
        let monitor_size = monitor.size();
        let scale = monitor.scale_factor();
        let mut preview_width = ((monitor_size.width as f64) * 0.86) as u32;
        let mut preview_height = ((monitor_size.height as f64) * 0.86) as u32;
        preview_width = preview_width.max(760).min(monitor_size.width);
        preview_height = preview_height.max(520).min(monitor_size.height);
        let target_x = monitor_pos.x + ((monitor_size.width - preview_width) / 2) as i32;
        let target_y = monitor_pos.y + ((monitor_size.height - preview_height) / 2) as i32;
        let _ = window.set_size(tauri::LogicalSize::new(preview_width as f64 / scale, preview_height as f64 / scale));
        let _ = window.set_position(tauri::PhysicalPosition::new(target_x, target_y));
    }
    Ok(())
}

pub fn hide_image_preview_window(app_handle: AppHandle) {
    if let Some(window) = app_handle.get_webview_window("image_preview") {
        hide_overlay_window(&app_handle, "image_preview", &window);
    }
}

pub fn show_text_preview_window(
    app_handle: AppHandle,
    text: String,
    item_id: Option<String>,
) -> Result<(), String> {
    let window = ensure_text_preview_window(&app_handle)?;
    
    prepare_image_preview_window(&window)?;

    let payload = serde_json::json!({
        "text": text,
        "item_id": item_id,
    });
    let _ = window.set_always_on_top(false);
    let _ = show_overlay_window(&app_handle, "text_preview", &window, true);
    let _ = app_handle.emit("show-text-preview", payload.clone());
    if let Ok(payload_str) = serde_json::to_string(&payload) {
        let script = format!(
            "window.__TEXT_PREVIEW_PAYLOAD__ = {payload_str};"
        );
        let _ = window.eval(&script);
    }
    Ok(())
}

pub fn hide_text_preview_window(app_handle: AppHandle) {
    if let Some(window) = app_handle.get_webview_window("text_preview") {
        hide_overlay_window(&app_handle, "text_preview", &window);
    }
}

/// 设置窗口位置和大小
pub fn set_window_position(window: &tauri::WebviewWindow, bottom_offset: i32) {
    if let Ok(Some(monitor)) = window.current_monitor() {
        let monitor_position = monitor.position();
        let screen_size = monitor.size();
        let scale_factor = monitor.scale_factor();
        let taskbar_safe_offset = get_taskbar_safe_offset() + bottom_offset.max(0);

        // 使用逻辑宽度（物理宽度 / 缩放因子）
        let window_width = (screen_size.width as f64 / scale_factor) as u32;
        let window_height = 360u32;

        let _ = window.set_size(tauri::LogicalSize::new(window_width, window_height));

        // 使用物理坐标计算位置
        let target_x = monitor_position.x;
        let target_y = monitor_position.y + screen_size.height as i32
            - (window_height as f64 * scale_factor) as i32
            - taskbar_safe_offset;
        let _ = window.set_position(tauri::PhysicalPosition::new(target_x, target_y));
    }
}

/// 获取任务栏安全偏移量（物理像素）
#[cfg(target_os = "windows")]
fn get_taskbar_safe_offset() -> i32 {
    unsafe {
        let mut work_area: RECT = std::mem::zeroed();
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work_area as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).is_ok() {
            let screen_height = GetSystemMetrics(SM_CYSCREEN);
            // Tauri 是 per-monitor DPI-aware，SPI_GETWORKAREA 返回物理像素
            return (screen_height - work_area.bottom).max(0);
        }
    }
    CLIPBOARD_WINDOW_BOTTOM_EXTRA_MARGIN
}

/// 获取任务栏安全偏移量
#[cfg(not(target_os = "windows"))]
fn get_taskbar_safe_offset() -> i32 {
    CLIPBOARD_WINDOW_BOTTOM_EXTRA_MARGIN
}

/// 通用的 overlay 窗口创建工厂函数
/// 如果窗口已存在则返回 (existing, false)，否则创建新窗口返回 (new, true)
pub fn ensure_overlay_window(
    app: &AppHandle,
    label: &str,
    html_file: &str,
    title: &str,
    inner_size: Option<(f64, f64)>,
) -> Result<(tauri::WebviewWindow, bool), String> {
    if let Some(existing) = app.get_webview_window(label) {
        return Ok((existing, false));
    }
    let mut builder = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App(html_file.into()),
    )
    .title(title)
    .visible(false)
    .resizable(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .always_on_top(true)
        .skip_taskbar(true)
        .accept_first_mouse(true);

    if let Some((w, h)) = inner_size {
        builder = builder.inner_size(w, h);
    }

    let window = builder
        .build()
        .map_err(|e| format!("创建{}窗口失败: {}", title, e))?;

    bind_overlay_window_events(&window, app.clone(), label);
    Ok((window, true))
}

/// 打开划词工具栏
fn ensure_selection_toolbar_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let label = "selection_toolbar";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok(existing);
    }
    if !is_window_feature_enabled(app, label) {
        return Err(AppErrorKind::SelectionFeatureDisabled.to_frontend_json());
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("selection_toolbar.html".into()),
    )
    .visible(false)
    .resizable(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .accept_first_mouse(true)
    .build()
    .map_err(|e| format!("创建划词工具栏窗口失败: {}", e))?;

    bind_overlay_window_events(&window, app.clone(), label);
    Ok(window)
}

fn ensure_clipboard_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let label = "clipboard";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok(existing);
    }
    if !is_window_feature_enabled(app, label) {
        return Err(AppErrorKind::SelectionClipboardDisabled.to_frontend_json());
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("clipboard.html".into()),
    )
    .visible(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(true)
    .maximizable(false)
    .minimizable(false)
        .accept_first_mouse(true)
    .build()
    .map_err(|e| format!("创建剪贴板窗口失败: {}", e))?;
    bind_overlay_window_events(&window, app.clone(), label);
    Ok(window)
}

fn ensure_image_clipboard_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let label = "image_clipboard";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok(existing);
    }
    if !is_window_feature_enabled(app, label) {
        return Err(AppErrorKind::SelectionImageClipboardDisabled.to_frontend_json());
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("image_clipboard.html".into()),
    )
    .visible(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(true)
    .maximizable(false)
    .minimizable(false)
        .accept_first_mouse(true)
    .build()
    .map_err(|e| format!("创建图片剪贴板窗口失败: {}", e))?;
    bind_overlay_window_events(&window, app.clone(), label);
    Ok(window)
}

fn ensure_launcher_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let label = "launcher";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok(existing);
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("launcher.html".into()),
    )
    .title("启动器")
    .visible(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(true)
    .maximizable(false)
    .minimizable(false)
        .accept_first_mouse(true)
    .center()
    .min_inner_size(620.0, 480.0)
    .inner_size(800.0, 600.0)
    .build()
    .map_err(|e| format!("创建启动器窗口失败: {}", e))?;
    bind_overlay_window_events(&window, app.clone(), label);
    let app_handle_for_resize = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Resized(_) = event {
            let _ = app_handle_for_resize.emit("launcher-resizing", ());
        }
    });
    Ok(window)
}

fn ensure_screenshot_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let label = "screenshot";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok(existing);
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("screenshot.html".into()),
    )
    .visible(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
        .resizable(false)
    .maximizable(false)
    .minimizable(false)
        .accept_first_mouse(true)
    .build()
    .map_err(|e| format!("创建截图窗口失败: {}", e))?;
    bind_overlay_window_events(&window, app.clone(), label);
    Ok(window)
}

fn ensure_image_preview_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let label = "image_preview";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok(existing);
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("image_preview.html".into()),
    )
    .visible(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .resizable(false)
    .closable(true)
    .build()
    .map_err(|e| format!("创建图片预览窗口失败: {}", e))?;
    bind_overlay_window_events(&window, app.clone(), label);
    Ok(window)
}

fn ensure_text_preview_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let label = "text_preview";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok(existing);
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("text_preview.html".into()),
    )
    .visible(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .resizable(false)
    .closable(true)
    .build()
    .map_err(|e| format!("创建文本预览窗口失败: {}", e))?;
    bind_overlay_window_events(&window, app.clone(), label);
    Ok(window)
}

fn ensure_document_manager_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let label = "document_manager";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok(existing);
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("document_manager.html".into()),
    )
    .title("文档管理")
    .visible(false)
    .resizable(true)
    .decorations(true)
    .inner_size(1200.0, 780.0)
    .center()
    .build()
    .map_err(|e| format!("创建文档管理窗口失败: {}", e))?;
    bind_standard_window_close_to_hide(&window);
    Ok(window)
}

fn ensure_doc_manager_widget_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let label = "document_manager_widget";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok(existing);
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("document_manager_widget.html".into()),
    )
        .title("文档管理小部件")
        .visible(false)
        .resizable(true)
        .decorations(false)
        .shadow(false)
        .transparent(true)
        .skip_taskbar(true)
        .inner_size(380.0, 460.0)
        .build()
        .map_err(|e| format!("创建文档管理小部件窗口失败: {}", e))?;
    bind_overlay_window_events(&window, app.clone(), label);
    Ok(window)
}

pub fn show_doc_manager_widget_window(app: &AppHandle) -> Result<(), String> {
    let window = ensure_doc_manager_widget_window(app)?;
    position_widget_to_top_right(&window)?;
    if let Ok(true) = window.is_visible() {
        let _ = window.set_focus();
        return Ok(());
    }
    show_overlay_window(app, "document_manager_widget", &window, false);
    Ok(())
}

pub fn hide_doc_manager_widget_window(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("document_manager_widget") {
        hide_overlay_window(app, "document_manager_widget", &window);
    }
    Ok(())
}

fn position_widget_to_top_right(window: &tauri::WebviewWindow) -> Result<(), String> {
    let logical_w = 380.0;
    let margin = 0.0;
    let monitor = window.current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No monitor found".to_string())?;
    let scale = monitor.scale_factor();
    let mpos = monitor.position();
    let msize = monitor.size();
    let current_size = window.outer_size().map_err(|e| e.to_string())?;
    let logical_h = current_size.height as f64 / scale;
    window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(logical_w, logical_h)))
        .map_err(|e| format!("set_size: {}", e))?;
    let px = mpos.x + msize.width as i32 - (logical_w * scale) as i32 - (margin * scale) as i32;
    let py = mpos.y + (margin * scale) as i32;
    window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: px, y: py }))
        .map_err(|e| format!("set_position: {}", e))
}

fn is_window_feature_enabled(app: &AppHandle, label: &str) -> bool {
    let Some(state) = app.try_state::<Arc<Mutex<AppState>>>() else {
        return true;
    };
    let guard = lock_arc_mutex(state.inner());
    let enabled = match label {
        "clipboard" | "text_preview" => guard.settings.text_clipboard_enabled,
        "image_clipboard" | "image_preview" => guard.settings.image_clipboard_enabled,
        "screenshot" | "longshot_toolbar" | "longshot_border" => guard.settings.screenshot_enabled,
        "recording_toolbar" => guard.settings.recording_enabled,
        "launcher" => guard.settings.launcher_enabled,
        "document_manager" => guard.settings.doc_manager_enabled,
        "document_manager_widget" => guard.settings.doc_manager_widget_enabled,
        "selection_toolbar" => guard.settings.selection_enabled,
        "settings" => true,
        _ => true,
    };
    if !enabled {
        log::warn!("[窗口创建被拦截] 功能已禁用: {}", label);
    }
    enabled
}

pub fn ensure_window_for_label(app: &AppHandle, label: &str) -> Result<(), String> {
    if app.get_webview_window(label).is_some() {
        return Ok(());
    }
    if !is_window_feature_enabled(app, label) {
        return Ok(());
    }
    match label {
        "clipboard" => { ensure_clipboard_window(app)?; }
        "image_clipboard" => { ensure_image_clipboard_window(app)?; }
        "launcher" => { ensure_launcher_window(app)?; }
        "screenshot" => { ensure_screenshot_window(app)?; }
        "image_preview" => { ensure_image_preview_window(app)?; }
        "text_preview" => { ensure_text_preview_window(app)?; }
        "document_manager" => { ensure_document_manager_window(app)?; }
        "document_manager_widget" => { ensure_doc_manager_widget_window(app)?; }
        "selection_toolbar" => { ensure_selection_toolbar_window(app)?; }
        "recording_toolbar" => {
            if app.get_webview_window(label).is_none() {
                let (_, _) = ensure_overlay_window(app, label, "recording_toolbar.html", "录屏工具栏", Some((530.0, 64.0)))?;
            }
        }
        "longshot_toolbar" => {
            if app.get_webview_window(label).is_none() {
                let (window, _) = ensure_overlay_window(app, label, "longshot_toolbar.html", "长截图工具栏", Some((320.0, 180.0)))?;
                let _ = window.set_content_protected(true);
            }
        }
        "longshot_border" => {
            if app.get_webview_window(label).is_none() {
                let (window, _) = ensure_overlay_window(app, label, "longshot_border.html", "长截图边框", None)?;
                let _ = window.set_content_protected(true);
            }
        }
        _ => {}
    }
    Ok(())
}

fn show_selection_toolbar_internal(
    app_handle: AppHandle,
    selected_text: String,
    anchor_pos: Option<(i32, i32)>,
    ignore_setting: bool,
) {
    remember_external_foreground_window(&app_handle);
    if !ignore_setting {
        if let Some(state) = app_handle.try_state::<Arc<Mutex<AppState>>>() {
            let state_guard = lock_arc_mutex(state.inner());
            if !state_guard.settings.selection_enabled {
                return;
            }
        } else {
            return;
        }
    }

    let toolbar_window = match ensure_selection_toolbar_window(&app_handle) {
        Ok(w) => w,
        Err(e) => {
            log::error!("{}", e);
            return;
        }
    };

    set_toolbar_window(&toolbar_window, anchor_pos);
    let _ = toolbar_window.set_always_on_top(false);
    let _ = toolbar_window.set_always_on_top(true);
    if show_overlay_window(&app_handle, "selection_toolbar", &toolbar_window, false) {
        if let Err(e) = app_handle.emit("selected-text", selected_text.clone()) {
            log::error!("未能发送选择文本到前端:{}", e);
        }
    }
    if let Ok(payload) = serde_json::to_string(&selected_text) {
        let script = format!(
            "window.__SELECTION_TOOLBAR_TEXT__ = {payload}; window.dispatchEvent(new CustomEvent('selection-toolbar-text', {{ detail: {payload} }}));"
        );
        let _ = toolbar_window.eval(&script);
    }
}

pub fn show_selection_toolbar_impl(
    app_handle: AppHandle,
    selected_text: String,
    anchor_pos: Option<(i32, i32)>,
) {
    show_selection_toolbar_internal(app_handle, selected_text, anchor_pos, false);
}

pub fn show_selection_toolbar_force_impl(
    app_handle: AppHandle,
    selected_text: String,
    anchor_pos: Option<(i32, i32)>,
) {
    show_selection_toolbar_internal(app_handle, selected_text, anchor_pos, true);
}

/// 设置工具栏窗口位置
fn set_toolbar_window(window: &tauri::WebviewWindow, anchor_pos: Option<(i32, i32)>) {
    let initial_width = 64u32;
    let initial_height = 64u32;
    let logical_offset_x = 10f64;
    let logical_offset_y = 15f64;
    let _ = window.set_size(tauri::LogicalSize::new(initial_width, initial_height));
    if let Some((mx, my)) = anchor_pos {
        let scale_factor = window.scale_factor().unwrap_or(1.0);
        let physical_initial_width = (initial_width as f64 * scale_factor) as i32;
        let physical_initial_height = (initial_height as f64 * scale_factor) as i32;
        let offset_x = (logical_offset_x * scale_factor) as i32;
        let offset_y = (logical_offset_y * scale_factor) as i32;

        let mut x = mx + offset_x;
        let mut y = my + offset_y;
        let monitor_from_anchor = window
            .available_monitors()
            .ok()
            .and_then(|monitors| {
                monitors.into_iter().find(|monitor| {
                    let pos = monitor.position();
                    let size = monitor.size();
                    let min_x = pos.x;
                    let min_y = pos.y;
                    let max_x = pos.x + size.width as i32;
                    let max_y = pos.y + size.height as i32;
                    mx >= min_x && mx <= max_x && my >= min_y && my <= max_y
                })
            })
            .or_else(|| window.current_monitor().ok().flatten());
        if let Some(monitor) = monitor_from_anchor {
            let monitor_pos = monitor.position();
            let monitor_size = monitor.size();
            let min_x = monitor_pos.x;
            let min_y = monitor_pos.y;
            let max_x = monitor_pos.x + monitor_size.width as i32 - physical_initial_width;
            let max_y = monitor_pos.y + monitor_size.height as i32 - physical_initial_height;
            let below_y = my + offset_y;
            let above_y = my - physical_initial_height - offset_y;
            if below_y <= max_y {
                y = below_y;
            } else if above_y >= min_y {
                y = above_y;
            } else {
                y = below_y.clamp(min_y, max_y.max(min_y));
            }
            x = x.clamp(min_x, max_x.max(min_x));
            y = y.clamp(min_y, max_y.max(min_y));
        }
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    } else {
        let _ = window.move_window(Position::RightCenter);
    }
}

/// 隐藏工具栏窗口
pub fn hide_selection_toolbar_impl(app_handle: AppHandle) {
    if let Some(toolbar_window) = app_handle.get_webview_window("selection_toolbar") {
        if let Ok(is_visible) = toolbar_window.is_visible() {
            if is_visible {
                if let Ok(has_focus) = toolbar_window.is_focused() {
                    if !has_focus {
                        hide_overlay_window(&app_handle, "selection_toolbar", &toolbar_window);
                    }
                }
            }
        }
    }
    // 清除去抖状态，允许下次选中文本时立即弹出工具栏
    crate::features::mouse_listener::clear_toolbar_debounce();
}

/// 检查并自动关闭划词工具栏
pub fn handle_selection_toolbar_autoclose(app_handle: &AppHandle, click_pos: Option<(i32, i32)>) {
    if let Some(window) = app_handle.get_webview_window("selection_toolbar") {
        if let Ok(true) = window.is_visible() {
            if let Some((mx, my)) = click_pos {
                if let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) {
                    let wx = pos.x;
                    let wy = pos.y;
                    let ww = size.width as i32;
                    let wh = size.height as i32;

                    if mx < wx || mx > wx + ww || my < wy || my > wy + wh {
                        hide_overlay_window(app_handle, "selection_toolbar", &window);
                    }
                }
            } else if let Ok(false) = window.is_focused() {
                hide_overlay_window(app_handle, "selection_toolbar", &window);
            }
        }
    }
}

/// 模拟粘贴操作
pub fn simulate_paste(app_handle: &AppHandle) -> Result<ForegroundWindowInfo, String> {
    use enigo::{Enigo, Settings};
    let target = wait_for_foreground_ready_for_paste(app_handle)?;

    {
        let mut enigo_guard = lock_arc_mutex(&ENIGO_INSTANCE);
        if enigo_guard.is_none() {
            *enigo_guard = Some(
                Enigo::new(&Settings::default())
                    .map_err(|e| format!("初始化粘贴输入器失败: {}", e))?,
            );
        }

        if let Some(ref mut enigo) = *enigo_guard {
            // 使用安全的粘贴执行函数
            execute_ctrl_v_with_safety(enigo)?;
        }
    }
    Ok(target)
}

/// 执行 Ctrl+V 操作，确保 Ctrl 键总是被正确释放
/// 使用 defer 模式，确保即使发生错误也能释放 Ctrl 键
fn execute_ctrl_v_with_safety(enigo: &mut enigo::Enigo) -> Result<(), String> {
    use enigo::{Direction, Key, Keyboard};

    thread::sleep(Duration::from_millis(100));

    enigo
        .key(CTRL_KEY, Direction::Press)
        .map_err(|e| format!("按下 Ctrl 失败: {}", e))?;

    thread::sleep(Duration::from_millis(100));
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| {
            let _ = release_ctrl_key_once(enigo);
            format!("发送 V 键失败: {}", e)
        })?;

    thread::sleep(Duration::from_millis(100));

    release_ctrl_key_once(enigo)?;

    log::info!("已发送 Ctrl+V 模拟按键");
    Ok(())
}

fn wait_for_foreground_ready_for_paste(
    app_handle: &AppHandle,
) -> Result<ForegroundWindowInfo, String> {
    let expected_target = app_handle
        .try_state::<Arc<Mutex<AppState>>>()
        .and_then(|state| {
            let guard = lock_arc_mutex(state.inner());
            guard.last_external_foreground.clone()
        });
    let mut stable_not_fuyun_count = 0usize;
    let mut stable_expected_count = 0usize;
    let mut last_pid = 0u32;
    let mut last_title = String::new();
    let mut interval = Duration::from_millis(8);
    let max_interval = Duration::from_millis(40);
    let mut restore_attempted = false;
    for _ in 0..24 {
        let (is_fuyun, info) = foreground_window_info();
        if !is_fuyun {
            if let Some(expected) = expected_target.as_ref() {
                if info.pid == expected.pid && info.title == expected.title {
                    stable_expected_count += 1;
                    if stable_expected_count >= 2 {
                        return Ok(info);
                    }
                } else {
                    stable_expected_count = 0;
                    if !restore_attempted {
                        let _ = try_restore_foreground_target(expected);
                        restore_attempted = true;
                    }
                }
            }
            if info.pid == last_pid && info.title == last_title {
                stable_not_fuyun_count += 1;
            } else {
                stable_not_fuyun_count = 1;
                last_pid = info.pid;
                last_title = info.title.clone();
            }
            if stable_not_fuyun_count >= 2 {
                return Ok(info);
            }
        } else {
            stable_not_fuyun_count = 0;
            stable_expected_count = 0;
        }
        thread::sleep(interval);
        interval = std::cmp::min(interval.saturating_mul(2), max_interval);
    }
    let (_, info) = foreground_window_info();
    if let Some(expected) = expected_target {
        Err(format!(
            "前台窗口未恢复到原目标窗口，期望: {} (pid={})，当前: {} (pid={})",
            expected.title, expected.pid, info.title, info.pid
        ))
    } else {
        Err(format!("前台窗口未就绪，当前窗口标题: {}", info.title))
    }
}

#[cfg(target_os = "windows")]
fn foreground_window_info() -> (bool, ForegroundWindowInfo) {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return (
                false,
                ForegroundWindowInfo {
                    title: "unknown".to_string(),
                    pid: 0,
                    hwnd: 0,
                },
            );
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut title_buffer = [0u16; 512];
        let title_len = GetWindowTextW(hwnd, &mut title_buffer);
        let title = if title_len > 0 {
            String::from_utf16_lossy(&title_buffer[..title_len as usize])
        } else {
            "untitled".to_string()
        };
        (
            pid != 0 && pid == GetCurrentProcessId(),
            ForegroundWindowInfo {
                title,
                pid,
                hwnd: hwnd.0 as isize,
            },
        )
    }
}

#[cfg(not(target_os = "windows"))]
fn foreground_window_info() -> (bool, ForegroundWindowInfo) {
    (
        false,
        ForegroundWindowInfo {
            title: "unknown".to_string(),
            pid: 0,
            hwnd: 0,
        },
    )
}

#[cfg(target_os = "windows")]
fn try_restore_foreground_target(target: &ForegroundTargetSnapshot) -> bool {
    unsafe {
        let hwnd = HWND(target.hwnd as *mut core::ffi::c_void);
        if hwnd.0.is_null() || IsWindow(Some(hwnd)) == BOOL(0) {
            return false;
        }
        if IsIconic(hwnd) != BOOL(0) {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd) != BOOL(0)
    }
}

#[cfg(not(target_os = "windows"))]
fn try_restore_foreground_target(_target: &ForegroundTargetSnapshot) -> bool {
    false
}

/// 显示结果窗口
pub async fn show_result_window(
    title: String,
    content: String,
    window_type: String,
    original: String,
    target_language: String,
    app: AppHandle,
    existing_window_label: Option<String>,
) -> Result<String, String> {
    remember_external_foreground_window(&app);

    // 如果提供了现有窗口标签，尝试复用该窗口
    if let Some(ref label) = existing_window_label {
        if let Some(window) = app.get_webview_window(label) {
            // 窗口存在，发送清理事件让它重新加载内容
            let _ = window.emit(
                "result-clean",
                serde_json::json!({
                    "type": window_type.clone(),
                    "opId": Option::<u64>::None,
                    "windowLabel": label.clone()
                }),
            );
            // 更新初始数据
            let payload = serde_json::json!({
                "type": window_type.clone(),
                "original": original.clone(),
                "content": content.clone(),
                "targetLanguage": target_language.clone()
            });
            let script = format!("window.__INITIAL_DATA__ = {}; window.dispatchEvent(new CustomEvent('init-data', {{ detail: window.__INITIAL_DATA__ }}));", payload);
            let _ = window.eval(&script);
            let _ = show_overlay_window(&app, label, &window, true);
            return Ok(label.clone());
        }
    }

    // 先销毁同类型旧结果窗口，避免每次请求累积隐藏窗口（webview 资源泄漏）
    let stale_prefix = format!("result_{}_", window_type);
    for label in app.webview_windows().keys() {
        if label.starts_with(&stale_prefix) {
            let _ = app.get_webview_window(label).map(|w| w.destroy());
        }
    }

    // 使用时间戳生成唯一窗口标签，确保每次都创建新窗口
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let window_label = format!("result_{}_{}", window_type, timestamp);

    let window = tauri::WebviewWindowBuilder::new(
        &app,
        &window_label,
        tauri::WebviewUrl::App("result_display.html".into()),
    )
    .title(&title)
    .visible(false)
    .inner_size(560.0, 360.0)
    .resizable(true)
    .decorations(false)
    .transparent(false)
    .shadow(true)
    .background_color(tauri::window::Color(250, 245, 235, 255))
    .always_on_top(true)
    .skip_taskbar(false)
    .on_page_load(move |window, _| {
        let payload = serde_json::json!({
            "type": window_type.clone(),
            "original": original.clone(),
            "content": content.clone(),
            "targetLanguage": target_language.clone()
        });
        let script = format!("window.__INITIAL_DATA__ = {};", payload);
        let _ = window.eval(&script);
    })
    .build()
    .map_err(|e| format!("创建窗口失败: {}", e))?;
    bind_overlay_window_events(&window, app.clone(), window_label.clone());

    position_result_window_near_toolbar(&window, &app);
    let _ = show_overlay_window(&app, &window_label, &window, true);
    Ok(window_label)
}

fn position_result_window_near_toolbar(window: &tauri::WebviewWindow, app: &AppHandle) {
    let Some(toolbar_window) = app.get_webview_window("selection_toolbar") else {
        let _ = window.move_window(Position::RightCenter);
        return;
    };

    let toolbar_pos = match toolbar_window.outer_position() {
        Ok(v) => v,
        Err(_) => {
            let _ = window.move_window(Position::RightCenter);
            return;
        }
    };
    let toolbar_size = match toolbar_window.outer_size() {
        Ok(v) => v,
        Err(_) => {
            let _ = window.move_window(Position::RightCenter);
            return;
        }
    };

    let result_size = window
        .outer_size()
        .unwrap_or(tauri::PhysicalSize::new(560, 360));

    let monitor = toolbar_window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        let _ = window.move_window(Position::RightCenter);
        return;
    };

    let gap = 12i32;
    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let min_x = monitor_pos.x;
    let min_y = monitor_pos.y;
    let max_x = monitor_pos.x + monitor_size.width as i32 - result_size.width as i32;
    let max_y = monitor_pos.y + monitor_size.height as i32 - result_size.height as i32;

    let mut x = toolbar_pos.x + (toolbar_size.width as i32 - result_size.width as i32) / 2;
    let below_y = toolbar_pos.y + toolbar_size.height as i32 + gap;
    let above_y = toolbar_pos.y - result_size.height as i32 - gap;
    let y = if below_y <= max_y {
        below_y
    } else if above_y >= min_y {
        above_y
    } else {
        below_y.clamp(min_y, max_y.max(min_y))
    };

    x = x.clamp(min_x, max_x.max(min_x));
    let clamped_y = y.clamp(min_y, max_y.max(min_y));
    let _ = window.set_position(tauri::PhysicalPosition::new(x, clamped_y));
}
