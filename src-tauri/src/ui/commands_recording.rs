use crate::core::app_state::SharedAppState;
use crate::core::error::to_frontend_error_string;
use crate::features::recording::ffmpeg_runner::resolve_ffmpeg_path;
use crate::features::recording::recorder_service;
use crate::features::recording::types::{
    AudioInputDevice, RecordingRegressionReport, RecordingRuntimeState, RecordingSessionInfo, RecordingStopResult,
    SessionRequest, StartRecordingRequest,
};
use crate::sync::Mutex;
use crate::utils::utils_helpers::load_settings;
use futures_util::StreamExt;
use serde::Deserialize;
use serde::Serialize;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingFfmpegStatus {
    pub exists: bool,
    pub ffmpeg_path: String,
    pub bin_dir: String,
    pub download_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingFfmpegDownloadProgress {
    phase: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    progress_percent: Option<u8>,
    message: String,
}

fn get_software_bin_dir() -> Result<PathBuf, String> {
    let mut exe_dir = env::current_exe().map_err(|e| format!("获取程序路径失败: {}", e))?;
    exe_dir.pop();
    Ok(exe_dir.join("bin"))
}

fn get_recording_ffmpeg_path() -> Result<PathBuf, String> {
    Ok(get_software_bin_dir()?.join("ffmpeg.exe"))
}

fn get_preferred_install_ffmpeg_path() -> Result<PathBuf, String> {
    // Dev mode: prefer src-tauri/bin/ffmpeg.exe so both check/start/download use one location.
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(manifest_dir).join("bin").join("ffmpeg.exe");
        return Ok(p);
    }
    // Production: install beside executable.
    get_recording_ffmpeg_path()
}

fn get_default_ffmpeg_download_url() -> String {
    load_settings()
        .map(|settings| settings.recording_ffmpeg_download_url.trim().to_string())
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| {
            "https://gitee.com/zrq1351/fuyun_tools/releases/download/v0.5.6/ffmpeg.exe".to_string()
        })
}

#[tauri::command]
pub async fn check_recording_ffmpeg() -> Result<RecordingFfmpegStatus, String> {
    let ffmpeg_path = resolve_ffmpeg_path().unwrap_or(get_preferred_install_ffmpeg_path()?);
    let bin_dir = ffmpeg_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or(get_software_bin_dir()?);
    Ok(RecordingFfmpegStatus {
        exists: ffmpeg_path.exists() && ffmpeg_path.is_file(),
        ffmpeg_path: ffmpeg_path.to_string_lossy().to_string(),
        bin_dir: bin_dir.to_string_lossy().to_string(),
        download_url: get_default_ffmpeg_download_url(),
    })
}

#[tauri::command]
pub async fn download_recording_ffmpeg(
    download_url: Option<String>,
    app: AppHandle,
) -> Result<RecordingFfmpegStatus, String> {
    let ffmpeg_path = get_preferred_install_ffmpeg_path()?;
    let bin_dir = ffmpeg_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or(get_software_bin_dir()?);
    let url = download_url
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .unwrap_or_else(get_default_ffmpeg_download_url);
    fs::create_dir_all(&bin_dir).map_err(|e| format!("创建目录失败: {}", e))?;

    let tmp_path = ffmpeg_path.with_extension("exe.tmp");
    if tmp_path.exists() {
        let _ = fs::remove_file(&tmp_path);
    }

    let _ = app.emit(
        "recording-ffmpeg-download-progress",
        RecordingFfmpegDownloadProgress {
            phase: "start".to_string(),
            downloaded_bytes: 0,
            total_bytes: None,
            progress_percent: Some(0),
            message: "开始下载 ffmpeg".to_string(),
        },
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("下载请求失败: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("下载 ffmpeg 失败，HTTP 状态: {}", response.status()));
    }
    let total_bytes = response.content_length();
    let mut downloaded_bytes: u64 = 0;
    let mut stream = response.bytes_stream();
    let mut file = fs::File::create(&tmp_path).map_err(|e| format!("创建临时文件失败: {}", e))?;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| format!("下载数据流失败: {}", e))?;
        file.write_all(&chunk)
            .map_err(|e| format!("写入临时文件失败: {}", e))?;
        downloaded_bytes = downloaded_bytes.saturating_add(chunk.len() as u64);
        let progress_percent = total_bytes.and_then(|total| {
            if total == 0 {
                None
            } else {
                Some(((downloaded_bytes.saturating_mul(100)) / total).min(100) as u8)
            }
        });
        let _ = app.emit(
            "recording-ffmpeg-download-progress",
            RecordingFfmpegDownloadProgress {
                phase: "downloading".to_string(),
                downloaded_bytes,
                total_bytes,
                progress_percent,
                message: "正在下载 ffmpeg".to_string(),
            },
        );
    }
    file.flush().map_err(|e| format!("刷新下载文件失败: {}", e))?;

    let metadata = fs::metadata(&tmp_path).map_err(|e| format!("读取下载文件失败: {}", e))?;
    if metadata.len() == 0 {
        let _ = fs::remove_file(&tmp_path);
        return Err("下载结果为空文件，请重试".to_string());
    }
    fs::rename(&tmp_path, &ffmpeg_path)
        .or_else(|_| {
            if ffmpeg_path.exists() {
                let _ = fs::remove_file(&ffmpeg_path);
            }
            fs::rename(&tmp_path, &ffmpeg_path)
        })
        .map_err(|e| format!("写入 ffmpeg 文件失败: {}", e))?;

    let _ = app.emit(
        "recording-ffmpeg-download-progress",
        RecordingFfmpegDownloadProgress {
            phase: "completed".to_string(),
            downloaded_bytes,
            total_bytes,
            progress_percent: Some(100),
            message: "ffmpeg 下载完成".to_string(),
        },
    );

    Ok(RecordingFfmpegStatus {
        exists: true,
        ffmpeg_path: ffmpeg_path.to_string_lossy().to_string(),
        bin_dir: bin_dir.to_string_lossy().to_string(),
        download_url: url,
    })
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

fn ensure_recording_toolbar_window(app: &AppHandle) -> Result<(tauri::WebviewWindow, bool), String> {
    let label = "recording_toolbar";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok((existing, false));
    }
    let window = tauri::WebviewWindowBuilder::new(app, label, WebviewUrl::App("recording_toolbar.html".into()))
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
        .map_err(|e| format!("创建录制工具栏窗口失败: {}", e))?;
    Ok((window, true))
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
    let (window, _created) = ensure_recording_toolbar_window(&app)?;
    let _ = window.set_size(tauri::PhysicalSize::new(530, 64));
    move_window_top_center(&window);
    let content_protected = load_settings()
        .map(|settings| settings.recording_toolbar_content_protected)
        .unwrap_or(false);
    let _ = window.set_content_protected(content_protected);
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
        if request.open_overlay {
            (430, 420)
        } else {
            (250, 40)
        }
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
    _state: Arc<Mutex<SharedAppState>>,
) {
    if let Ok((window, _created)) = ensure_recording_toolbar_window(&app) {
        // Set compact size and position before showing to avoid opening flicker/jump.
        let _ = window.set_size(tauri::PhysicalSize::new(250, 40));
        move_window_top_center(&window);
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app.emit("recording-toolbar-force-compact", ());
}
