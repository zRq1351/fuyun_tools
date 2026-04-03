use crate::core::app_state::SharedAppState;
use crate::core::error::to_frontend_error_string;
use crate::features::recording::recorder_service;
use crate::features::recording::types::{
    AudioInputDevice, RecordingRegressionReport, RecordingRuntimeState, RecordingSessionInfo, RecordingStopResult,
    SessionRequest, StartRecordingRequest,
};
use crate::sync::Mutex;
use serde::Deserialize;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State, WebviewUrl};
use tauri_plugin_positioner::WindowExt;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeRecordingToolbarRequest {
    pub open_select: bool,
    pub open_overlay: bool,
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
pub async fn open_recording_window(app: AppHandle) -> Result<(), String> {
    let window = if let Some(existing) = app.get_webview_window("recording") {
        existing
    } else {
        tauri::WebviewWindowBuilder::new(&app, "recording", WebviewUrl::App("recording.html".into()))
            .title("录屏")
            .visible(false)
            .resizable(true)
            .inner_size(840.0, 620.0)
            .build()
            .map_err(|e| format!("创建录屏控制台窗口失败: {}", e))?
    };
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
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
            .inner_size(630.0, 70.0)
            .build()
            .map_err(|e| format!("创建录制工具栏窗口失败: {}", e))?
    };
    let _ = window.set_size(tauri::PhysicalSize::new(630, 70));
    // 首次创建时，定位到当前屏幕上方居中
    if created {
        if let (Ok(size), Ok(Some(monitor))) = (window.outer_size(), window.current_monitor()) {
            let mon_pos = monitor.position();
            let mon_size = monitor.size();
            let x = mon_pos.x + (mon_size.width as i32 - size.width as i32) / 2;
            let y = mon_pos.y + 16; // 距离屏幕顶端 16px
            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        } else {
            // 兜底：无法获取监视器信息时，使用插件器居中
            let _ = window.move_window(tauri_plugin_positioner::Position::TopCenter);
        }
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
    let width = 630;
    let height = if request.open_select {
        340
    } else if request.open_overlay {
        120
    } else {
        70
    };
    window
        .set_size(tauri::PhysicalSize::new(width, height))
        .map_err(|e| format!("调整录制工具栏窗口尺寸失败: {}", e))?;
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
    let current = recorder_service::get_recording_state(state.clone());
    if current.state == "recording" || current.state == "paused" {
        let _ = recorder_service::stop_recording(&app, state, SessionRequest { session_id: None });
    } else {
        let _ = show_recording_toolbar(app).await;
    }
}
