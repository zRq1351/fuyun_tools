use crate::core::app_state::SharedAppState;
use crate::core::error::{AppError, ErrorCode};
use crate::features::recording::audio_device::list_microphones;
use crate::features::recording::error_codes::{
    AUDIO_DEVICE_LOST, AUDIO_DEVICE_NOT_FOUND, MAX_DURATION_REACHED, RECORDING_PROCESS_EXITED,
    RECORDING_START_FAILED,
};
use crate::features::recording::events::{
    emit_recording_device_list, emit_recording_error, emit_recording_finished, emit_recording_state_changed,
    emit_recording_stats_updated,
};
use crate::features::recording::ffmpeg_runner::{build_output_paths, resolve_ffmpeg_path};
use crate::features::recording::native_wasapi::{
    start_microphone_wav_with_device, start_system_loopback_wav_with_device,
};
use crate::features::recording::state::RecordingPhase;
use crate::features::recording::types::{
    AudioInputDevice, RecordingRegressionReport, RecordingRuntimeState, RecordingSessionInfo,
    RecordingStopResult, SessionRequest, StartRecordingRequest,
};
use crate::sync::Mutex;
use std::fs;
use std::io::{BufRead, BufReader, Write};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{ChildStderr, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
#[cfg(target_os = "windows")]
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
#[cfg(target_os = "windows")]
use winapi::um::processthreadsapi::{OpenThread, ResumeThread, SuspendThread};
#[cfg(target_os = "windows")]
use winapi::um::tlhelp32::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, THREADENTRY32, TH32CS_SNAPTHREAD};
#[cfg(target_os = "windows")]
use winapi::um::winnt::THREAD_SUSPEND_RESUME;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn lock_arc_mutex<T>(mutex: &Arc<Mutex<T>>) -> crate::sync::MutexGuard<'_, T> {
    mutex.lock().expect("infallible mutex lock failed")
}

fn suppress_console_window(command: &mut Command) -> &mut Command {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn normalize_runtime_state(runtime: &mut crate::features::recording::state::RecordingRuntime) {
    if let Some(process) = runtime.process.as_mut() {
        if let Ok(Some(_)) = process.try_wait() {
            runtime.process = None;
        }
    }
    if runtime.process.is_none() {
        match runtime.phase {
            RecordingPhase::Idle => {}
            RecordingPhase::Starting
            | RecordingPhase::Recording
            | RecordingPhase::Paused
            | RecordingPhase::Stopping
            | RecordingPhase::Error => runtime.reset_to_idle(),
        }
    }
}

// check_system_audio_capability removed in native WASAPI mode

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(target_os = "windows")]
fn set_process_threads_suspended(process_id: u32, suspend: bool) -> Result<(), AppError> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(AppError::new(ErrorCode::SystemError, "获取线程快照失败"));
        }
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        let mut ok = Thread32First(snapshot, &mut entry) != 0;
        let mut first_error: Option<String> = None;
        while ok {
            if entry.th32OwnerProcessID == process_id {
                let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                if !thread.is_null() {
                    let ret = if suspend {
                        SuspendThread(thread)
                    } else {
                        ResumeThread(thread)
                    };
                    if ret == u32::MAX && first_error.is_none() {
                        first_error = Some(format!(
                            "{}线程失败，thread_id={}",
                            if suspend { "暂停" } else { "恢复" },
                            entry.th32ThreadID
                        ));
                    }
                    let _ = CloseHandle(thread);
                }
            }
            ok = Thread32Next(snapshot, &mut entry) != 0;
        }
        let _ = CloseHandle(snapshot);
        if let Some(details) = first_error {
            return Err(AppError::new(
                ErrorCode::SystemError,
                if suspend { "暂停录制失败" } else { "恢复录制失败" },
            )
            .with_details(details));
        }
        Ok(())
    }
}

fn resolve_output_dir(state: &SharedAppState, request_output_dir: Option<String>) -> Result<PathBuf, AppError> {
    if let Some(dir) = request_output_dir {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    if !state.settings.recording_output_dir.trim().is_empty() {
        return Ok(PathBuf::from(state.settings.recording_output_dir.trim()));
    }
    let mut base = std::env::current_exe()
        .map_err(|e| AppError::new(ErrorCode::IoError, format!("读取程序路径失败: {}", e)))?;
    let _ = base.pop();
    Ok(base.join("recordings"))
}

fn merge_system_audio_into_video(
    ffmpeg_path: &std::path::Path,
    video_path: &PathBuf,
    system_wav_path: Option<&PathBuf>,
    mic_wav_path: Option<&PathBuf>,
) -> Result<(), AppError> {
    let has_system = system_wav_path.map(|p| p.exists()).unwrap_or(false);
    let has_mic = mic_wav_path.map(|p| p.exists()).unwrap_or(false);
    if !has_system && !has_mic {
        return Ok(());
    }
    let merged_path = video_path.with_extension("merged.tmp.mp4");
    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y")
        .arg("-i")
        .arg(video_path);
    if has_system {
        cmd.arg("-i").arg(system_wav_path.expect("checked"));
    }
    if has_mic {
        cmd.arg("-i").arg(mic_wav_path.expect("checked"));
    }
    if has_system && has_mic {
        cmd.arg("-filter_complex")
            .arg("[1:a][2:a]amix=inputs=2:duration=longest[aout]")
            .arg("-map")
            .arg("0:v:0")
            .arg("-map")
            .arg("[aout]");
    } else if has_system {
        cmd.arg("-map").arg("0:v:0").arg("-map").arg("1:a:0");
    } else if has_mic {
        cmd.arg("-map").arg("0:v:0").arg("-map").arg("1:a:0");
    } else {
        return Ok(());
    }
    cmd.arg("-c:v")
        .arg("copy")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("192k")
        .arg(&merged_path);
    let status = cmd
        .status()
        .map_err(|e| AppError::new(ErrorCode::SystemError, "执行系统音频合成失败").with_details(e.to_string()))?;
    if !status.success() {
        return Err(AppError::new(ErrorCode::SystemError, "系统音频合成失败"));
    }
    if video_path.exists() {
        let _ = fs::remove_file(video_path);
    }
    fs::rename(&merged_path, video_path)
        .map_err(|e| AppError::new(ErrorCode::IoError, "写入合成文件失败").with_details(e.to_string()))?;
    if let Some(p) = system_wav_path {
        let _ = fs::remove_file(p);
    }
    if let Some(p) = mic_wav_path {
        let _ = fs::remove_file(p);
    }
    Ok(())
}

pub fn list_audio_devices(app: &AppHandle) -> Result<Vec<AudioInputDevice>, AppError> {
    let dummy = std::path::Path::new("");
    let devices = list_microphones(dummy)
        .map_err(|e| AppError::new(ErrorCode::SystemError, "读取麦克风设备失败").with_details(e))?;
    emit_recording_device_list(app, devices.clone());
    Ok(devices)
}

pub fn list_system_output_devices(_app: &AppHandle) -> Result<Vec<AudioInputDevice>, AppError> {
    let ffmpeg_path = std::path::Path::new(""); // 未使用，仅为兼容签名
    let outs = crate::features::recording::audio_device::list_system_audio_sources(ffmpeg_path)
        .map_err(|e| AppError::new(ErrorCode::SystemError, "读取系统输出设备失败").with_details(e))?;
    Ok(outs)
}
// list_input_devices removed in native WASAPI mode

fn cleanup_stale_tmp_files(output_dir: &PathBuf) {
    if let Ok(entries) = fs::read_dir(output_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !file_name.ends_with(".tmp.mp4") {
                continue;
            }
            let _ = fs::remove_file(path);
        }
    }
}

fn map_ffmpeg_error(line: &str) -> Option<(&'static str, String)> {
    let lower = line.to_lowercase();
    if lower.contains("error opening input file default")
        || lower.contains("error opening input files")
        && lower.contains("invalid argument")
    {
        return Some((
            AUDIO_DEVICE_NOT_FOUND,
            "系统音频设备不可用，请在录屏控制台关闭“系统音频”后重试".to_string(),
        ));
    }
    if lower.contains("device not found")
        || lower.contains("could not find audio device")
        || lower.contains("audio device")
        && (lower.contains("failed") || lower.contains("invalid"))
    {
        return Some((AUDIO_DEVICE_LOST, "音频设备不可用或已断开".to_string()));
    }
    if lower.contains("immediate exit requested")
        || lower.contains("conversion failed")
        || lower.contains("i/o error")
        || lower.contains("error")
    {
        return Some((RECORDING_PROCESS_EXITED, line.trim().to_string()));
    }
    None
}

fn parse_u64_after(line: &str, marker: &str) -> Option<u64> {
    let idx = line.find(marker)?;
    let s = &line[idx + marker.len()..];
    let mut buf = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            buf.push(ch);
        } else if !buf.is_empty() {
            break;
        }
    }
    if buf.is_empty() {
        None
    } else {
        buf.parse::<u64>().ok()
    }
}

fn parse_kbits_after(line: &str, marker: &str) -> Option<u32> {
    let idx = line.find(marker)?;
    let s = &line[idx + marker.len()..];
    let mut buf = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            buf.push(ch);
        } else if !buf.is_empty() {
            break;
        }
    }
    if buf.is_empty() {
        return None;
    }
    let value = buf.parse::<f64>().ok()?;
    if value.is_finite() && value > 0.0 {
        Some(value.round() as u32)
    } else {
        None
    }
}

fn spawn_stderr_parser(
    app: AppHandle,
    runtime_arc: Arc<Mutex<crate::features::recording::state::RecordingRuntime>>,
    session_id: String,
    stderr: ChildStderr,
) {
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let mut emit_error_payload: Option<(&'static str, String)> = None;
            {
                let mut runtime = lock_arc_mutex(&runtime_arc);
                if runtime.session_id.as_deref() != Some(session_id.as_str()) {
                    break;
                }
                if let Some(drop) = parse_u64_after(&line, "drop=") {
                    runtime.dropped_video_frames = drop;
                }
                if let Some(v) = parse_kbits_after(&line, "bitrate=") {
                    runtime.video_bitrate_kbps = v;
                }
                if let Some(v) = parse_kbits_after(&line, "audio:") {
                    runtime.audio_bitrate_kbps = v;
                }
                if let Some((code, message)) = map_ffmpeg_error(&line) {
                    runtime.last_error = Some(message.clone());
                    emit_error_payload = Some((code, message));
                    if code == AUDIO_DEVICE_LOST {
                        if let Some(process) = runtime.process.as_mut() {
                            let _ = process.kill();
                        }
                    }
                }
            }
            if let Some((code, message)) = emit_error_payload {
                emit_recording_error(&app, Some(session_id.as_str()), code, message.as_str());
            }
        }
    });
}

fn spawn_stats_loop(app: AppHandle, runtime_arc: Arc<Mutex<crate::features::recording::state::RecordingRuntime>>) {
    thread::spawn(move || loop {
        let mut emit_error: Option<(&'static str, String, Option<String>)> = None;
        let mut emit_finished: Option<RecordingStopResult> = None;
        let (
            phase,
            session_id,
            fps,
            video_bitrate_kbps,
            audio_bitrate_kbps,
            dropped_video_frames,
            audio_buffer_level_ms,
            elapsed_ms,
        ) = {
            let mut runtime = lock_arc_mutex(&runtime_arc);
            let snapshot = runtime.snapshot();
            let mut phase = runtime.phase;
            let session_id = runtime.session_id.clone();
            if runtime.max_duration_ms > 0
                && (phase == RecordingPhase::Recording || phase == RecordingPhase::Paused)
                && snapshot.elapsed_ms >= runtime.max_duration_ms
                && !runtime.auto_stop_requested
            {
                runtime.auto_stop_requested = true;
                runtime.phase = RecordingPhase::Stopping;
                phase = RecordingPhase::Stopping;
                if let Some(process) = runtime.process.as_mut() {
                    if let Some(stdin) = process.stdin.as_mut() {
                        let _ = stdin.write_all(b"q\n");
                    }
                }
                emit_error = Some((
                    MAX_DURATION_REACHED,
                    "已达到最大录制时长，自动停止录制".to_string(),
                    session_id.clone(),
                ));
            }

            if let Some(process) = runtime.process.as_mut() {
                if let Ok(Some(status)) = process.try_wait() {
                    runtime.process = None;
                    if runtime.auto_stop_requested {
                        if let (Some(tmp), Some(final_path)) =
                            (runtime.output_path_tmp.clone(), runtime.output_path_final.clone())
                        {
                            if final_path.exists() {
                                let _ = fs::remove_file(&final_path);
                            }
                            if fs::rename(&tmp, &final_path).is_ok() {
                                let finished = RecordingStopResult {
                                    session_id: session_id.clone().unwrap_or_default(),
                                    output_path: final_path.to_string_lossy().to_string(),
                                    duration_ms: runtime.snapshot().elapsed_ms,
                                    file_size_bytes: fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0),
                                };
                                emit_finished = Some(finished);
                                runtime.reset_to_idle();
                                phase = RecordingPhase::Idle;
                            }
                        }
                    } else if phase != RecordingPhase::Idle && phase != RecordingPhase::Stopping {
                        let err_msg = format!("录制进程异常退出: {}", status);
                        runtime.last_error = Some(err_msg.clone());
                        runtime.phase = RecordingPhase::Error;
                        phase = RecordingPhase::Error;
                        if let Some(tmp) = runtime.output_path_tmp.clone() {
                            let _ = fs::remove_file(tmp);
                        }
                        emit_error = Some((RECORDING_PROCESS_EXITED, err_msg, session_id.clone()));
                    }
                }
            }

            (
                phase,
                session_id,
                runtime.fps,
                runtime.video_bitrate_kbps,
                runtime.audio_bitrate_kbps,
                runtime.dropped_video_frames,
                runtime.audio_buffer_level_ms,
                runtime.snapshot().elapsed_ms,
            )
        };
        if let Some((code, message, sid)) = emit_error {
            emit_recording_error(&app, sid.as_deref(), code, message.as_str());
        }
        if let Some(result) = emit_finished {
            emit_recording_finished(&app, &result);
        }
        if phase == RecordingPhase::Idle || phase == RecordingPhase::Error {
            break;
        }
        emit_recording_stats_updated(
            &app,
            session_id.as_deref(),
            fps,
            video_bitrate_kbps,
            audio_bitrate_kbps,
            dropped_video_frames,
            audio_buffer_level_ms,
        );
        emit_recording_state_changed(&app, session_id.as_deref(), phase.as_str(), elapsed_ms);
        thread::sleep(Duration::from_millis(500));
    });
}

pub fn start_recording(
    app: &AppHandle,
    state_arc: Arc<Mutex<SharedAppState>>,
    request: StartRecordingRequest,
) -> Result<RecordingSessionInfo, AppError> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, state_arc, request);
        return Err(AppError::new(ErrorCode::SystemError, "当前平台暂不支持录屏"));
    }
    #[cfg(target_os = "windows")]
    {
        let ffmpeg_path = resolve_ffmpeg_path()
            .map_err(|e| AppError::new(ErrorCode::SystemError, "未找到 ffmpeg").with_details(e))?;
        let (
            runtime_arc,
            settings_snapshot,
            output_dir,
            capture_cursor,
            capture_microphone,
            capture_system_audio,
            fps,
            video_bitrate,
            audio_bitrate,
            system_audio_device_id,
        ) = {
            let state_guard = lock_arc_mutex(&state_arc);
            if !state_guard.settings.recording_enabled {
                return Err(AppError::new(ErrorCode::ValidationError, "录屏功能已停用"));
            }
            let output_dir = resolve_output_dir(&state_guard, request.output_dir.clone())?;
            (
                state_guard.recording_runtime.clone(),
                state_guard.settings.clone(),
                output_dir,
                request.capture_cursor.unwrap_or(state_guard.settings.recording_capture_cursor),
                request.capture_microphone.unwrap_or(state_guard.settings.recording_capture_microphone),
                request
                    .capture_system_audio
                    .unwrap_or(state_guard.settings.recording_capture_system_audio),
                request.fps.unwrap_or(state_guard.settings.recording_default_fps).clamp(1, 120),
                request
                    .video_bitrate_kbps
                    .unwrap_or(state_guard.settings.recording_default_video_bitrate_kbps)
                    .clamp(500, 50000),
                request
                    .audio_bitrate_kbps
                    .unwrap_or(state_guard.settings.recording_default_audio_bitrate_kbps)
                    .clamp(32, 512),
                request.system_audio_device_id.clone(),
            )
        };

        fs::create_dir_all(&output_dir)
            .map_err(|e| AppError::new(ErrorCode::IoError, "创建录制目录失败").with_details(e.to_string()))?;
        cleanup_stale_tmp_files(&output_dir);
        let (tmp_path, final_path, session_id) = build_output_paths(&output_dir);
        let mut runtime = lock_arc_mutex(&runtime_arc);
        normalize_runtime_state(&mut runtime);
        if matches!(
            runtime.phase,
            RecordingPhase::Recording | RecordingPhase::Starting | RecordingPhase::Paused | RecordingPhase::Stopping
        ) {
            return Err(AppError::new(ErrorCode::ValidationError, "已有录制任务在运行"));
        }
        runtime.phase = RecordingPhase::Starting;
        runtime.last_error = None;
        emit_recording_state_changed(app, None, runtime.phase.as_str(), 0);

        let mut args: Vec<String> = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "warning".into(),
            "-y".into(),
            "-f".into(),
            "gdigrab".into(),
            "-framerate".into(),
            format!("{}", fps),
            "-draw_mouse".into(),
            if capture_cursor { "1".into() } else { "0".into() },
            "-i".into(),
            "desktop".into(),
        ];
        // 删除 ffmpeg 系统音频输入路径，改为 Rust 原生 WASAPI 录制（后处理合成）
        args.push("-map".to_string());
        args.push("0:v:0".to_string());
        args.extend_from_slice(&[
            "-c:v".to_string(),
            "libx264".to_string(),
            "-preset".to_string(),
            "veryfast".to_string(),
            "-pix_fmt".to_string(),
            "yuv420p".to_string(),
            "-b:v".to_string(),
            format!("{}k", video_bitrate),
        ]);
        args.push("-an".to_string());
        args.push(tmp_path.to_string_lossy().to_string());
        let mut command = Command::new(&ffmpeg_path);
        suppress_console_window(&mut command);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|e| {
            runtime.phase = RecordingPhase::Error;
            runtime.last_error = Some(e.to_string());
            emit_recording_error(app, None, RECORDING_START_FAILED, "录制进程启动失败");
            AppError::new(ErrorCode::SystemError, "启动录制失败").with_details(e.to_string())
        })?;
        let stderr = child.stderr.take();
        runtime.phase = RecordingPhase::Recording;
        runtime.session_id = Some(session_id.clone());
        runtime.started_at_ms = now_unix_ms();
        runtime.started_instant = Some(std::time::Instant::now());
        runtime.paused_at_instant = None;
        runtime.paused_total_ms = 0;
        runtime.max_duration_ms = (settings_snapshot.recording_max_duration_minutes as u64)
            .saturating_mul(60_000);
        runtime.auto_stop_requested = false;
        runtime.fps = fps;
        runtime.video_bitrate_kbps = video_bitrate;
        runtime.audio_bitrate_kbps = if capture_microphone || capture_system_audio {
            audio_bitrate
        } else {
            0
        };
        runtime.mic_enabled = capture_microphone;
        runtime.output_path_tmp = Some(tmp_path.clone());
        runtime.output_path_final = Some(final_path.clone());
        // 如果需要系统音频，启动 WASAPI 录制到 wav
        if capture_system_audio {
            let sys_wav = output_dir.join(format!("{}.sys.wav", session_id));
            match start_system_loopback_wav_with_device(system_audio_device_id, sys_wav.clone()) {
                Ok(handle) => {
                    runtime.system_audio_wav_path = Some(sys_wav);
                    runtime.system_audio_stop_flag = Some(handle.stop_flag.clone());
                    runtime.system_audio_thread = handle.join;
                }
                Err(e) => {
                    emit_recording_error(app, Some(&session_id), AUDIO_DEVICE_NOT_FOUND, e.as_str());
                }
            }
        }
        // 如果需要麦克风，启动 WASAPI 录制到 wav
        if capture_microphone {
            let mic_wav = output_dir.join(format!("{}.mic.wav", session_id));
            match start_microphone_wav_with_device(request.microphone_device_id.clone(), mic_wav.clone()) {
                Ok(handle) => {
                    runtime.mic_audio_wav_path = Some(mic_wav);
                    runtime.mic_audio_stop_flag = Some(handle.stop_flag.clone());
                    runtime.mic_audio_thread = handle.join;
                }
                Err(e) => {
                    emit_recording_error(app, Some(&session_id), AUDIO_DEVICE_NOT_FOUND, e.as_str());
                }
            }
        }
        runtime.process = Some(child);
        let started_at_ms = runtime.started_at_ms;
        emit_recording_state_changed(app, Some(&session_id), runtime.phase.as_str(), 0);
        drop(runtime);
        if let Some(stderr) = stderr {
            spawn_stderr_parser(app.clone(), runtime_arc.clone(), session_id.clone(), stderr);
        }
        spawn_stats_loop(app.clone(), runtime_arc.clone());

        Ok(RecordingSessionInfo {
            session_id,
            started_at_ms,
            output_path_tmp: tmp_path.to_string_lossy().to_string(),
        })
    }
}

pub fn stop_recording(
    app: &AppHandle,
    state_arc: Arc<Mutex<SharedAppState>>,
    request: SessionRequest,
) -> Result<RecordingStopResult, AppError> {
    let ffmpeg_path = resolve_ffmpeg_path()
        .map_err(|e| AppError::new(ErrorCode::SystemError, "未找到 ffmpeg").with_details(e))?;
    let runtime_arc = {
        let state_guard = lock_arc_mutex(&state_arc);
        state_guard.recording_runtime.clone()
    };
    let mut runtime = lock_arc_mutex(&runtime_arc);
    if runtime.phase != RecordingPhase::Recording && runtime.phase != RecordingPhase::Paused {
        return Err(AppError::new(ErrorCode::ValidationError, "当前没有正在进行的录制任务"));
    }
    if let Some(ref expected) = request.session_id {
        if runtime.session_id.as_deref() != Some(expected.as_str()) {
            return Err(AppError::new(ErrorCode::ValidationError, "录制会话已变化，请刷新状态后重试"));
        }
    }
    let was_paused = runtime.phase == RecordingPhase::Paused;
    runtime.phase = RecordingPhase::Stopping;
    runtime.auto_stop_requested = false;
    let session_id = runtime.session_id.clone().unwrap_or_default();
    emit_recording_state_changed(
        app,
        Some(session_id.as_str()),
        runtime.phase.as_str(),
        runtime
            .started_instant
            .map(|it| it.elapsed().as_millis() as u64)
            .unwrap_or(0),
    );

    if let Some(process) = runtime.process.as_mut() {
        #[cfg(target_os = "windows")]
        if was_paused {
            let _ = set_process_threads_suspended(process.id(), false);
        }
        if let Some(stdin) = process.stdin.as_mut() {
            let _ = stdin.write_all(b"q\n");
        }
    }
    let mut exited = false;
    if let Some(process) = runtime.process.as_mut() {
        for _ in 0..80 {
            if let Ok(Some(_)) = process.try_wait() {
                exited = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        if !exited {
            let _ = process.kill();
            let _ = process.wait();
        }
    }

    let output_tmp = runtime
        .output_path_tmp
        .clone()
        .ok_or_else(|| AppError::new(ErrorCode::SystemError, "录制临时文件路径不存在"))?;
    let output_final = runtime
        .output_path_final
        .clone()
        .ok_or_else(|| AppError::new(ErrorCode::SystemError, "录制输出路径不存在"))?;
    if output_final.exists() {
        let _ = fs::remove_file(&output_final);
    }
    fs::rename(&output_tmp, &output_final)
        .map_err(|e| AppError::new(ErrorCode::IoError, "重命名录制文件失败").with_details(e.to_string()))?;

    if let Some(flag) = runtime.system_audio_stop_flag.take() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(join) = runtime.system_audio_thread.take() {
        let _ = join.join();
    }
    if let Some(flag) = runtime.mic_audio_stop_flag.take() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(join) = runtime.mic_audio_thread.take() {
        let _ = join.join();
    }
    let _ = merge_system_audio_into_video(
        &ffmpeg_path,
        &output_final,
        runtime.system_audio_wav_path.as_ref(),
        runtime.mic_audio_wav_path.as_ref(),
    );

    if let Some(paused_at) = runtime.paused_at_instant {
        runtime.paused_total_ms = runtime
            .paused_total_ms
            .saturating_add(paused_at.elapsed().as_millis() as u64);
        runtime.paused_at_instant = None;
    }
    let duration_ms = runtime.snapshot().elapsed_ms;
    let file_size_bytes = fs::metadata(&output_final).map(|m| m.len()).unwrap_or(0);
    let result = RecordingStopResult {
        session_id: session_id.clone(),
        output_path: output_final.to_string_lossy().to_string(),
        duration_ms,
        file_size_bytes,
    };
    emit_recording_finished(app, &result);
    runtime.reset_to_idle();
    emit_recording_state_changed(app, None, runtime.phase.as_str(), 0);
    Ok(result)
}

pub fn cancel_recording(
    app: &AppHandle,
    state_arc: Arc<Mutex<SharedAppState>>,
    request: SessionRequest,
) -> Result<(), AppError> {
    let runtime_arc = {
        let state_guard = lock_arc_mutex(&state_arc);
        state_guard.recording_runtime.clone()
    };
    let mut runtime = lock_arc_mutex(&runtime_arc);
    if runtime.phase == RecordingPhase::Idle {
        return Ok(());
    }
    if let Some(ref expected) = request.session_id {
        if runtime.session_id.as_deref() != Some(expected.as_str()) {
            return Err(AppError::new(ErrorCode::ValidationError, "录制会话已变化，请刷新状态后重试"));
        }
    }
    if let Some(process) = runtime.process.as_mut() {
        let _ = process.kill();
        let _ = process.wait();
    }
    if let Some(flag) = runtime.system_audio_stop_flag.take() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(join) = runtime.system_audio_thread.take() {
        let _ = join.join();
    }
    if let Some(wav) = runtime.system_audio_wav_path.clone() {
        let _ = fs::remove_file(wav);
    }
    if let Some(flag) = runtime.mic_audio_stop_flag.take() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(join) = runtime.mic_audio_thread.take() {
        let _ = join.join();
    }
    if let Some(wav) = runtime.mic_audio_wav_path.clone() {
        let _ = fs::remove_file(wav);
    }
    if let Some(path) = runtime.output_path_tmp.clone() {
        let _ = fs::remove_file(path);
    }
    runtime.reset_to_idle();
    emit_recording_state_changed(app, None, runtime.phase.as_str(), 0);
    Ok(())
}

pub fn pause_recording(app: &AppHandle, state_arc: Arc<Mutex<SharedAppState>>) -> Result<(), AppError> {
    let runtime_arc = {
        let state_guard = lock_arc_mutex(&state_arc);
        state_guard.recording_runtime.clone()
    };
    let mut runtime = lock_arc_mutex(&runtime_arc);
    if runtime.phase != RecordingPhase::Recording {
        return Err(AppError::new(ErrorCode::ValidationError, "当前状态不允许暂停"));
    }
    if let Some(process) = runtime.process.as_mut() {
        #[cfg(target_os = "windows")]
        {
            set_process_threads_suspended(process.id(), true)?;
        }
        #[cfg(not(target_os = "windows"))]
        if let Some(stdin) = process.stdin.as_mut() {
            stdin
                .write_all(b"p\n")
                .map_err(|e| AppError::new(ErrorCode::SystemError, "暂停录制失败").with_details(e.to_string()))?;
        }
    }
    runtime.phase = RecordingPhase::Paused;
    runtime.paused_at_instant = Some(std::time::Instant::now());
    let snapshot = runtime.snapshot();
    emit_recording_state_changed(app, runtime.session_id.as_deref(), runtime.phase.as_str(), snapshot.elapsed_ms);
    Ok(())
}

pub fn resume_recording(app: &AppHandle, state_arc: Arc<Mutex<SharedAppState>>) -> Result<(), AppError> {
    let runtime_arc = {
        let state_guard = lock_arc_mutex(&state_arc);
        state_guard.recording_runtime.clone()
    };
    let mut runtime = lock_arc_mutex(&runtime_arc);
    if runtime.phase != RecordingPhase::Paused {
        return Err(AppError::new(ErrorCode::ValidationError, "当前状态不允许恢复"));
    }
    if let Some(process) = runtime.process.as_mut() {
        #[cfg(target_os = "windows")]
        {
            set_process_threads_suspended(process.id(), false)?;
        }
        #[cfg(not(target_os = "windows"))]
        if let Some(stdin) = process.stdin.as_mut() {
            stdin
                .write_all(b"p\n")
                .map_err(|e| AppError::new(ErrorCode::SystemError, "恢复录制失败").with_details(e.to_string()))?;
        }
    }
    if let Some(paused_at) = runtime.paused_at_instant {
        runtime.paused_total_ms = runtime
            .paused_total_ms
            .saturating_add(paused_at.elapsed().as_millis() as u64);
    }
    runtime.paused_at_instant = None;
    runtime.phase = RecordingPhase::Recording;
    let snapshot = runtime.snapshot();
    emit_recording_state_changed(app, runtime.session_id.as_deref(), runtime.phase.as_str(), snapshot.elapsed_ms);
    Ok(())
}

pub fn get_recording_state(state_arc: Arc<Mutex<SharedAppState>>) -> RecordingRuntimeState {
    let runtime_arc = {
        let state_guard = lock_arc_mutex(&state_arc);
        state_guard.recording_runtime.clone()
    };
    let runtime = lock_arc_mutex(&runtime_arc);
    runtime.snapshot()
}

pub fn get_recording_output_dir(state_arc: Arc<Mutex<SharedAppState>>) -> Result<String, AppError> {
    let output_dir = {
        let guard = lock_arc_mutex(&state_arc);
        resolve_output_dir(&guard, None)?
    };
    Ok(output_dir.to_string_lossy().to_string())
}

pub fn run_recording_regression(
    app: &AppHandle,
    state_arc: Arc<Mutex<SharedAppState>>,
) -> Result<RecordingRegressionReport, AppError> {
    {
        let runtime_arc = {
            let state_guard = lock_arc_mutex(&state_arc);
            state_guard.recording_runtime.clone()
        };
        let mut runtime = lock_arc_mutex(&runtime_arc);
        normalize_runtime_state(&mut runtime);
    }
    let current = get_recording_state(state_arc.clone());
    if current.state != "idle" {
        return Err(AppError::new(
            ErrorCode::ValidationError,
            format!("已有录制任务在运行（当前状态: {}），请先停止后再执行回归自测", current.state),
        ));
    }

    let execute = || -> Result<RecordingRegressionReport, AppError> {
        let mut steps = Vec::new();
        let session = start_recording(
            app,
            state_arc.clone(),
            StartRecordingRequest {
                target_type: Some("display".to_string()),
                target_id: None,
                capture_cursor: Some(true),
                capture_system_audio: Some(false),
                system_audio_device_id: None,
                capture_microphone: Some(false),
                microphone_device_id: None,
                fps: Some(20),
                video_bitrate_kbps: Some(3500),
                audio_bitrate_kbps: Some(128),
                output_dir: None,
                container: Some("mp4".to_string()),
                op_id: None,
            },
        )?;
        steps.push("start_recording:ok".to_string());
        thread::sleep(Duration::from_millis(1200));

        pause_recording(app, state_arc.clone())?;
        steps.push("pause_recording:ok".to_string());
        thread::sleep(Duration::from_millis(700));

        resume_recording(app, state_arc.clone())?;
        steps.push("resume_recording:ok".to_string());
        thread::sleep(Duration::from_millis(1200));

        let result = stop_recording(
            app,
            state_arc.clone(),
            SessionRequest {
                session_id: Some(session.session_id.clone()),
            },
        )?;
        steps.push("stop_recording:ok".to_string());

        let output = PathBuf::from(&result.output_path);
        let metadata = fs::metadata(&output).map_err(|e| {
            AppError::new(ErrorCode::IoError, "回归验证失败：录制文件不存在").with_details(e.to_string())
        })?;
        if metadata.len() == 0 {
            return Err(AppError::new(
                ErrorCode::ValidationError,
                "回归验证失败：录制文件大小为0",
            ));
        }
        steps.push("verify_output_file:ok".to_string());

        Ok(RecordingRegressionReport {
            success: true,
            session_id: Some(result.session_id),
            output_path: Some(result.output_path),
            duration_ms: result.duration_ms,
            file_size_bytes: result.file_size_bytes,
            steps,
            message: "录屏回归自测通过".to_string(),
        })
    };

    match execute() {
        Ok(report) => Ok(report),
        Err(e) => {
            let _ = cancel_recording(
                app,
                state_arc,
                SessionRequest {
                    session_id: None,
                },
            );
            Err(e)
        }
    }
}

pub fn open_recording_folder(state_arc: Arc<Mutex<SharedAppState>>) -> Result<(), AppError> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = state_arc;
        return Err(AppError::new(ErrorCode::SystemError, "当前平台暂不支持打开录制目录"));
    }
    #[cfg(target_os = "windows")]
    {
        let output_dir = {
            let state_guard = lock_arc_mutex(&state_arc);
            resolve_output_dir(&state_guard, None)?
        };
        fs::create_dir_all(&output_dir)
            .map_err(|e| AppError::new(ErrorCode::IoError, "创建录制目录失败").with_details(e.to_string()))?;
        Command::new("explorer.exe")
            .arg(output_dir)
            .spawn()
            .map_err(|e| AppError::new(ErrorCode::SystemError, "打开录制目录失败").with_details(e.to_string()))?;
        Ok(())
    }
}
