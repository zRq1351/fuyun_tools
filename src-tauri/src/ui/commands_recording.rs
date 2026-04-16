use crate::core::app_state::SharedAppState;
use crate::core::error::to_frontend_error_string;
use crate::core::perf_metrics::record_perf_metric;
use crate::features::recording::ffmpeg_runner::resolve_ffmpeg_path;
use crate::features::recording::recorder_service;
use crate::features::recording::types::{
    AudioInputDevice, AudioProcessItem, RecordingRegressionReport, RecordingRuntimeState, RecordingSessionInfo, RecordingStopResult,
    SessionRequest, StartRecordingRequest,
};
use crate::sync::Mutex;
use crate::ui::window_manager::{bind_overlay_window_events, show_overlay_window_by_label};
use crate::utils::utils_helpers::load_settings;
use futures_util::StreamExt;
use serde::Deserialize;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl};
use tauri_plugin_positioner::WindowExt;

async fn run_blocking_command<T, F>(task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|e| format!("录屏任务执行失败: {}", e))?
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeRecordingToolbarRequest {
    pub open_select: bool,
    pub open_overlay: bool,
    #[serde(default)]
    pub compact_mode: bool,
    #[serde(default = "default_layout_mode")]
    pub layout_mode: String,
    #[serde(default)]
    pub recenter: bool,
    #[serde(default)]
    pub capsule_content_height: Option<u32>,
    #[serde(default)]
    pub capsule_content_width: Option<u32>,
}

fn default_layout_mode() -> String {
    "capsule".to_string()
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

fn normalize_sha256_hex(raw: &str) -> Option<String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(value)
    } else {
        None
    }
}

fn split_download_url_and_sha256(raw: &str) -> Result<(String, Option<String>), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("下载地址不能为空".to_string());
    }
    if let Some((url, fragment)) = trimmed.split_once("#sha256=") {
        let expected = normalize_sha256_hex(fragment)
            .ok_or_else(|| "下载地址中的 sha256 参数格式无效（应为64位十六进制）".to_string())?;
        return Ok((url.trim().to_string(), Some(expected)));
    }
    Ok((trimmed.to_string(), None))
}

fn is_trusted_download_host(host: &str) -> bool {
    matches!(
        host,
        "gitee.com" | "github.com" | "objects.githubusercontent.com" | "aka.ms"
    )
}

fn validate_download_url_policy(url: &str, expected_sha256: Option<&str>) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("下载地址无效: {}", e))?;
    if parsed.scheme() != "https" {
        return Err("下载地址必须使用 HTTPS".to_string());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "下载地址缺少主机名".to_string())?
        .to_ascii_lowercase();
    if expected_sha256.is_none() && !is_trusted_download_host(&host) {
        return Err(format!(
            "未提供 sha256 时，仅允许可信下载域名；当前域名不受信任: {}",
            host
        ));
    }
    Ok(())
}

fn compute_file_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("读取下载文件失败: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("读取下载文件失败: {}", e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        hex.push_str(format!("{:02x}", b).as_str());
    }
    Ok(hex)
}

fn verify_downloaded_exe_integrity(path: &Path, expected_sha256: Option<&str>) -> Result<(), String> {
    let mut header = [0u8; 2];
    let mut file = fs::File::open(path).map_err(|e| format!("读取下载文件失败: {}", e))?;
    file.read_exact(&mut header)
        .map_err(|e| format!("读取下载文件头失败: {}", e))?;
    if header != [b'M', b'Z'] {
        return Err("下载文件不是有效的 Windows 可执行文件".to_string());
    }
    if let Some(expected) = expected_sha256 {
        let actual = compute_file_sha256(path)?;
        if actual != expected {
            return Err(format!(
                "下载文件 SHA-256 校验失败，expected={}, actual={}",
                expected, actual
            ));
        }
    }
    Ok(())
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
    let raw_url = download_url
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .unwrap_or_else(get_default_ffmpeg_download_url);
    let (url, expected_sha256) = split_download_url_and_sha256(&raw_url)?;
    validate_download_url_policy(&url, expected_sha256.as_deref())?;
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
    verify_downloaded_exe_integrity(&tmp_path, expected_sha256.as_deref()).inspect_err(|_| {
        let _ = fs::remove_file(&tmp_path);
    })?;
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

    bind_overlay_window_events(&window, app.clone(), label);

    Ok((window, true))
}

#[tauri::command]
pub async fn start_recording(
    request: StartRecordingRequest,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<RecordingSessionInfo, String> {
    let started_at = Instant::now();
    let state_arc = state.inner().clone();
    let result = run_blocking_command(move || {
        recorder_service::start_recording(&app, state_arc, request).map_err(to_frontend_error_string)
    })
    .await;
    match &result {
        Ok(_) => record_perf_metric(
            "recording.start",
            "录屏开始耗时",
            started_at.elapsed().as_millis() as u64,
            true,
            None,
        ),
        Err(error) => record_perf_metric(
            "recording.start",
            "录屏开始耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        ),
    }
    result
}

#[tauri::command]
pub async fn stop_recording(
    request: SessionRequest,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<RecordingStopResult, String> {
    let started_at = Instant::now();
    let state_arc = state.inner().clone();
    let result = run_blocking_command(move || match recorder_service::stop_recording(&app, state_arc.clone(), request.clone()) {
        Ok(result) => {
            let auto_open_folder = {
                let guard = state_arc.lock().expect("infallible mutex lock failed");
                guard.settings.recording_auto_open_folder
            };
            if auto_open_folder {
                if let Err(e) = recorder_service::open_recording_folder(&app, state_arc.clone()) {
                    log::warn!("录制完成自动打开目录失败: {}", e);
                }
            }
            Ok(result)
        }
        Err(stop_err) => {
            let fallback_req = SessionRequest {
                session_id: request.session_id.clone(),
            };
            match recorder_service::cancel_recording(&app, state_arc.clone(), fallback_req) {
                Ok(()) => {
                    log::warn!("stop_recording 失败，已自动执行 cancel_recording 兜底清理");
                    Err(to_frontend_error_string(stop_err))
                }
                Err(cancel_err) => {
                    log::warn!(
                        "stop_recording 失败，且 cancel_recording 兜底清理也失败: {}",
                        cancel_err
                    );
                    let merged_err = stop_err.with_details(format!(
                        "自动兜底清理失败: {}",
                        cancel_err
                    ));
                    Err(to_frontend_error_string(merged_err))
                }
            }
        }
    })
    .await;
    match &result {
        Ok(_) => record_perf_metric(
            "recording.stop",
            "录屏停止耗时",
            started_at.elapsed().as_millis() as u64,
            true,
            None,
        ),
        Err(error) => record_perf_metric(
            "recording.stop",
            "录屏停止耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        ),
    }
    result
}

#[tauri::command]
pub async fn cancel_recording(
    request: SessionRequest,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    run_blocking_command(move || {
        recorder_service::cancel_recording(&app, state_arc, request).map_err(to_frontend_error_string)
    })
        .await
}

#[tauri::command]
pub async fn pause_recording(
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    run_blocking_command(move || recorder_service::pause_recording(&app, state_arc).map_err(to_frontend_error_string)).await
}

#[tauri::command]
pub async fn resume_recording(
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    run_blocking_command(move || recorder_service::resume_recording(&app, state_arc).map_err(to_frontend_error_string))
        .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecordingAudioCaptureRequest {
    pub capture_system_audio: Option<bool>,
    pub system_audio_device_id: Option<String>,
    pub capture_microphone: Option<bool>,
    pub microphone_device_id: Option<String>,
}

#[tauri::command]
pub async fn update_recording_audio_capture(
    request: UpdateRecordingAudioCaptureRequest,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    run_blocking_command(move || {
        recorder_service::update_audio_capture(
            &app,
            state_arc,
            request.capture_system_audio,
            request.system_audio_device_id,
            request.capture_microphone,
            request.microphone_device_id,
        )
        .map_err(to_frontend_error_string)
    })
        .await
}

#[tauri::command]
pub async fn get_recording_state(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<RecordingRuntimeState, String> {
    Ok(recorder_service::get_recording_state(state.inner().clone()))
}

#[tauri::command]
pub async fn get_recording_output_dir(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<String, String> {
    recorder_service::get_recording_output_dir(state.inner().clone()).map_err(to_frontend_error_string)
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

#[tauri::command]
pub async fn list_recording_audio_processes() -> Result<Vec<AudioProcessItem>, String> {
    recorder_service::list_audio_process_items().map_err(to_frontend_error_string)
}
// input device listing & capability/installer commands removed in native WASAPI mode

#[tauri::command]
pub async fn open_recording_folder(
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    recorder_service::open_recording_folder(&app, state.inner().clone()).map_err(to_frontend_error_string)
}

#[tauri::command]
pub async fn show_recording_toolbar(app: AppHandle) -> Result<(), String> {
    let (window, _created) = ensure_recording_toolbar_window(&app)?;
    let _ = window.set_size(tauri::PhysicalSize::new(530, 64));
    let _ = window.set_min_size::<tauri::Size>(None);
    let _ = window.set_max_size::<tauri::Size>(None);
    let _ = window.set_resizable(false);
    move_window_top_center(&window);
    let content_protected = load_settings()
        .map(|settings| settings.recording_toolbar_content_protected)
        .unwrap_or(false);
    let _ = window.set_content_protected(content_protected);
    show_overlay_window_by_label(&app, "recording_toolbar", true)?;
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
    let scale_factor = window.scale_factor().unwrap_or(1.0);
    let max_width_logical = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            let logical_w = ((monitor.size().width as f64) / scale_factor).floor() as u32;
            logical_w.saturating_sub(16)
        })
        .unwrap_or(1200);
    let max_height_logical = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            let logical_h = ((monitor.size().height as f64) / scale_factor).floor() as u32;
            logical_h.saturating_sub(16)
        })
        .unwrap_or(900);
    let is_capsule_layout = request.layout_mode.eq_ignore_ascii_case("capsule");
    let (width_logical, height_logical) = if is_capsule_layout {
        if request.open_overlay {
            let preferred_width = request.capsule_content_width.unwrap_or(400);
            let preferred_height = request.capsule_content_height.unwrap_or(730);
            (
                preferred_width.clamp(360, max_width_logical.max(360)),
                preferred_height.clamp(320, max_height_logical.max(320)),
            )
        } else {
            // 胶囊模式：增加宽度以容纳麦克风按钮（原180px + 麦克风按钮30px）
            (210, 40)
        }
    } else if request.compact_mode {
        if request.open_overlay {
            (400, 730)
        } else {
            // 紧凑模式：同样增加宽度以容纳麦克风按钮
            (210, 40)
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
    let target_size = tauri::LogicalSize::new(width_logical as f64, height_logical as f64);
    let need_resize = prev_size
        .as_ref()
        .map(|size| {
            let prev_w = ((size.width as f64) / scale_factor).round() as u32;
            let prev_h = ((size.height as f64) / scale_factor).round() as u32;
            prev_w != width_logical || prev_h != height_logical
        })
        .unwrap_or(true);

    if need_resize {
        window
            .set_size(target_size)
            .map_err(|e| format!("调整录制工具栏窗口尺寸失败: {}", e))?;
    }
    let _ = window.set_min_size::<tauri::Size>(None);
    let _ = window.set_max_size::<tauri::Size>(None);
    let _ = window.set_resizable(false);
    // Recenter only when caller explicitly asks for it.
    if request.recenter {
        move_window_top_center(&window);
    }
    Ok(())
}
#[tauri::command]
pub async fn run_recording_regression(
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<RecordingRegressionReport, String> {
    let state_arc = state.inner().clone();
    run_blocking_command(move || {
        recorder_service::run_recording_regression(&app, state_arc).map_err(to_frontend_error_string)
    })
        .await
}

pub async fn toggle_recording_from_shortcut(
    app: AppHandle,
    _state: Arc<Mutex<SharedAppState>>,
) {
    if let Ok((window, _created)) = ensure_recording_toolbar_window(&app) {
        // Set compact size and position before showing to avoid opening flicker/jump.
        let _ = window.set_size(tauri::LogicalSize::new(180.0, 40.0));
        move_window_top_center(&window);
        let _ = show_overlay_window_by_label(&app, "recording_toolbar", true);
    }
    let _ = app.emit("recording-toolbar-force-compact", ());
}

/// 切换麦克风状态的辅助函数（供快捷键调用）
/// `enable`: true=按下快捷键（启用麦克风），false=松开快捷键（禁用麦克风）
pub async fn toggle_microphone_from_shortcut(app: AppHandle, enable: bool) {
    use crate::features::recording::recorder_service;

    let key_state = if enable { "按下" } else { "释放" };
    log::info!("麦克风快捷键{}（目标状态：{}）", key_state, enable);

    let state_arc = {
        let app_state = app.state::<Arc<Mutex<crate::core::app_state::SharedAppState>>>();
        app_state.inner().clone()
    };

    // 获取当前录制状态
    let current_state = recorder_service::get_recording_state(state_arc.clone());

    // 只有在录制中或暂停时才能切换麦克风
    if current_state.state != "recording" && current_state.state != "paused" {
        log::warn!("无法切换麦克风：当前不在录制状态 (state={})", current_state.state);
        return;
    }

    // 获取录制运行时的麦克风设备ID
    let mic_device_id = {
        let state_guard = state_arc.lock().unwrap();
        let runtime = &state_guard.recording_runtime;
        let runtime_guard = runtime.lock().unwrap();
        runtime_guard.mic_audio_device_id.clone()
    };

    // 如果没有选择麦克风设备，无法切换
    if mic_device_id.is_none() {
        log::warn!("无法切换麦克风：未选择麦克风设备");
        return;
    }

    // 发送即时UI反馈事件
    if enable {
        // 按下快捷键：立即显示麦克风开启状态
        let _ = app.emit("recording-mic-key-pressed", serde_json::json!({}));
    } else {
        // 松开快捷键：立即显示麦克风关闭状态
        let _ = app.emit("recording-mic-key-released", serde_json::json!({}));
    }

    // 记录当前系统音频状态用于调试
    let (sys_audio_enabled, sys_audio_thread_exists) = {
        let state_guard = state_arc.lock().unwrap();
        let runtime = &state_guard.recording_runtime;
        let runtime_guard = runtime.lock().unwrap();

        let sys_enabled = runtime_guard.system_audio_enabled_flag
            .as_ref()
            .map(|f| f.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap_or(false);

        (
            sys_enabled,
            runtime_guard.system_audio_thread.is_some()
        )
    };

    log::info!("快捷键操作：麦克风{}，系统音频状态: {} (线程存在: {})", 
        if enable { "启用" } else { "禁用" }, 
        sys_audio_enabled, 
        sys_audio_thread_exists);

    // 只修改麦克风状态，保持系统音频状态不变（传入None让函数使用当前值）
    match recorder_service::update_audio_capture(
        &app,
        state_arc,
        None, // 不改变系统音频状态，使用当前值
        None, // 不改变系统音频设备，使用当前值
        Some(enable), // 启用或禁用麦克风
        mic_device_id.clone(),
    ) {
        Ok(_) => {
            log::info!("麦克风已{}", if enable { "启用" } else { "禁用" });
            let _ = app.emit("recording-mic-toggled", serde_json::json!({
                "enabled": enable
            }));
        }
        Err(e) => {
            log::error!("切换麦克风失败: {}", e);
            let _ = app.emit("recording-error", serde_json::json!({
                "message": format!("切换麦克风失败: {}", e),
                "code": "MIC_TOGGLE_FAILED"
            }));
        }
    }
}
