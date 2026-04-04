use crate::core::app_state::SharedAppState;
use crate::core::error::to_frontend_error_string;
use crate::features::recording::recorder_service;
use crate::features::recording::types::{
    AudioInputDevice, RecordingRegressionReport, RecordingRuntimeState, RecordingSessionInfo, RecordingStopResult,
    SessionRequest, StartRecordingRequest,
};
use crate::sync::Mutex;
use crate::utils::utils_helpers::load_settings;
use serde::Deserialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl};
use tauri_plugin_positioner::WindowExt;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeRecordingToolbarRequest {
    pub open_select: bool,
    pub open_overlay: bool,
    #[serde(default)]
    pub compact_mode: bool,
}

fn move_window_top_center(window: &tauri::WebviewWindow) {
    if let (Ok(size), Ok(Some(monitor))) = (window.outer_size(), window.current_monitor()) {
        let mon_pos = monitor.position();
        let mon_size = monitor.size();
        let x = mon_pos.x + (mon_size.width as i32 - size.width as i32) / 2;
        let y = mon_pos.y;
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    } else {
        let _ = window.move_window(tauri_plugin_positioner::Position::TopCenter);
    }
}

#[tauri::command]
pub async fn start_recording(
    request: StartRecordingRequest,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<RecordingSessionInfo, String> {
    recorder_service::start_recording(&app, state.inner().clone(), request).map_err(to_frontend_error_string)
}

#[tauri::command]
pub async fn stop_recording(
    request: SessionRequest,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<RecordingStopResult, String> {
    recorder_service::stop_recording(&app, state.inner().clone(), request).map_err(to_frontend_error_string)
}

#[tauri::command]
pub async fn cancel_recording(
    request: SessionRequest,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    recorder_service::cancel_recording(&app, state.inner().clone(), request).map_err(to_frontend_error_string)
}

#[tauri::command]
pub async fn pause_recording(
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    recorder_service::pause_recording(&app, state.inner().clone()).map_err(to_frontend_error_string)
}

#[tauri::command]
pub async fn resume_recording(
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    recorder_service::resume_recording(&app, state.inner().clone()).map_err(to_frontend_error_string)
}

#[tauri::command]
pub async fn get_recording_state(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<RecordingRuntimeState, String> {
    Ok(recorder_service::get_recording_state(state.inner().clone()))
}

#[tauri::command]
pub async fn list_recording_audio_devices(
    app: AppHandle,
) -> Result<Vec<AudioInputDevice>, String> {
    recorder_service::list_audio_devices(&app).map_err(to_frontend_error_string)
}

#[tauri::command]
pub async fn list_recording_system_output_devices(
    app: AppHandle,
) -> Result<Vec<AudioInputDevice>, String> {
    // 复用 list_system_audio_sources（内部已切到 WASAPI 枚举）
    recorder_service::list_system_output_devices(&app).map_err(to_frontend_error_string)
}
// input device listing & capability/installer commands removed in native WASAPI mode

#[tauri::command]
pub async fn open_recording_folder(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    recorder_service::open_recording_folder(state.inner().clone()).map_err(to_frontend_error_string)
}

#[tauri::command]
pub async fn show_recording_toolbar(app: AppHandle) -> Result<(), String> {
    let label = "recording_toolbar";
    let created;
    let window = if let Some(existing) = app.get_webview_window(label) {
        created = false;
        existing
    } else {
        created = true;
        tauri::WebviewWindowBuilder::new(&app, label, WebviewUrl::App("recording_toolbar.html".into()))
            .title("录制工具栏")
            .visible(false)
            .resizable(false)
            .decorations(false)
            .shadow(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .inner_size(530.0, 64.0)
            .build()
            .map_err(|e| format!("创建录制工具栏窗口失败: {}", e))?
    };
    let _ = window.set_size(tauri::PhysicalSize::new(530, 64));
    move_window_top_center(&window);
    let content_protected = load_settings()
        .map(|settings| settings.recording_toolbar_content_protected)
        .unwrap_or(false);
    let _ = window.set_content_protected(content_protected);
    // 首次创建时，定位到当前屏幕上方居中
    if created {
        move_window_top_center(&window);
    }
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
pub async fn resize_recording_toolbar(
    request: ResizeRecordingToolbarRequest,
    app: AppHandle,
) -> Result<(), String> {
    let Some(window) = app.get_webview_window("recording_toolbar") else {
        return Ok(());
    };
    let prev_size = window.outer_size().ok();
    let was_compact = prev_size
        .as_ref()
        .map(|size| size.width <= 260)
        .unwrap_or(false);
    let (width, height) = if request.compact_mode {
        (170, 26)
    } else {
        let h = if request.open_select {
            340
        } else if request.open_overlay {
            120
        } else {
            64
        };
        (530, h)
    };
    let target_size = tauri::PhysicalSize::new(width, height);
    let need_resize = prev_size
        .as_ref()
        .map(|size| size.width != width as u32 || size.height != height as u32)
        .unwrap_or(true);

    if need_resize {
        window
            .set_size(target_size)
            .map_err(|e| format!("调整录制工具栏窗口尺寸失败: {}", e))?;
    }
    // 胶囊/工具栏形态切换时重新居中，保证两种形态都居中显示
    if was_compact != request.compact_mode {
        move_window_top_center(&window);
    }
    Ok(())
}
#[tauri::command]
pub async fn run_recording_regression(
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<RecordingRegressionReport, String> {
    recorder_service::run_recording_regression(&app, state.inner().clone()).map_err(to_frontend_error_string)
}

pub async fn toggle_recording_from_shortcut(
    app: AppHandle,
    state: Arc<Mutex<SharedAppState>>,
) {
    let current = recorder_service::get_recording_state(state);
    let _ = show_recording_toolbar(app.clone()).await;
    if current.state == "recording" || current.state == "paused" {
        let _ = app.emit("recording-toolbar-force-expand", ());
    }
}
