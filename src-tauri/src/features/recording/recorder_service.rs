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
    list_audio_processes, start_microphone_wav_with_device, start_process_loopback_wavs,
    start_system_loopback_wav_with_device,
};
use crate::features::recording::state::RecordingPhase;
use crate::features::recording::types::{
    AudioInputDevice, AudioProcessItem, RecordingRegressionReport, RecordingRuntimeState, RecordingSessionInfo,
    RecordingStopResult, SessionRequest, StartRecordingRequest,
};
use crate::features::recording::wgc_capture::start_window_capture_to_mp4;
use crate::sync::Mutex;
use std::fs;
use std::io::{BufRead, BufReader, Write};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::{ChildStderr, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
#[cfg(target_os = "windows")]
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
#[cfg(target_os = "windows")]
use winapi::um::processthreadsapi::{OpenThread, ResumeThread, SuspendThread};
#[cfg(target_os = "windows")]
use winapi::um::tlhelp32::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32};
#[cfg(target_os = "windows")]
use winapi::um::winnt::THREAD_SUSPEND_RESUME;
#[cfg(target_os = "windows")]
use winapi::um::winuser::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
static LAST_OPEN_FOLDER_MS: AtomicU64 = AtomicU64::new(0);

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
    let wgc_running = runtime
        .wgc_thread
        .as_ref()
        .map(|t| !t.is_finished())
        .unwrap_or(false);
    if runtime.process.is_none() && !wgc_running {
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

fn parse_region_target(target_id: &str) -> Option<(i32, i32, u32, u32)> {
    let parts: Vec<&str> = target_id.split(',').map(|s| s.trim()).collect();
    if parts.len() != 4 {
        return None;
    }
    let x = parts[0].parse::<i32>().ok()?;
    let y = parts[1].parse::<i32>().ok()?;
    let width = parts[2].parse::<u32>().ok()?;
    let height = parts[3].parse::<u32>().ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some((x, y, width, height))
}

#[cfg(target_os = "windows")]
fn normalize_region_to_virtual_screen(x: i32, y: i32, width: u32, height: u32) -> Option<(i32, i32, u32, u32)> {
    let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let vw = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let vh = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    if vw <= 0 || vh <= 0 {
        return None;
    }
    let min_x = vx as i64;
    let min_y = vy as i64;
    let max_x = min_x + vw as i64;
    let max_y = min_y + vh as i64;
    let raw_x = x as i64;
    let raw_y = y as i64;
    let raw_w = width as i64;
    let raw_h = height as i64;
    let clamped_x = raw_x.max(min_x).min(max_x - 1);
    let clamped_y = raw_y.max(min_y).min(max_y - 1);
    let available_w = (max_x - clamped_x).max(1);
    let available_h = (max_y - clamped_y).max(1);
    let clamped_w = raw_w.max(1).min(available_w);
    let clamped_h = raw_h.max(1).min(available_h);
    Some((clamped_x as i32, clamped_y as i32, clamped_w as u32, clamped_h as u32))
}

#[cfg(not(target_os = "windows"))]
fn normalize_region_to_virtual_screen(x: i32, y: i32, width: u32, height: u32) -> Option<(i32, i32, u32, u32)> {
    Some((x, y, width.max(1), height.max(1)))
}

fn push_stderr_tail(runtime: &mut crate::features::recording::state::RecordingRuntime, line: &str) {
    let text = line.trim();
    if text.is_empty() {
        return;
    }
    runtime.ffmpeg_stderr_tail.push_back(text.to_string());
    while runtime.ffmpeg_stderr_tail.len() > 6 {
        runtime.ffmpeg_stderr_tail.pop_front();
    }
}

fn build_exit_error_with_stderr(status_text: String, runtime: &crate::features::recording::state::RecordingRuntime) -> String {
    if runtime.ffmpeg_stderr_tail.is_empty() {
        return format!("录制进程异常退出: {}", status_text);
    }
    let tail = runtime
        .ffmpeg_stderr_tail
        .iter()
        .rev()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" | ");
    format!("录制进程异常退出: {}；stderr: {}", status_text, tail)
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
    system_segments: &[crate::features::recording::state::AudioSegment],
    mic_segments: &[crate::features::recording::state::AudioSegment],
    system_switch_points_ms: &[u64],
    mic_switch_points_ms: &[u64],
) -> Result<(), AppError> {
    let expected_system_count = system_segments.len();
    let expected_mic_count = mic_segments.len();
    let is_valid_audio_segment = |seg: &crate::features::recording::state::AudioSegment| {
        if !seg.path.exists() {
            return false;
        }
        // WAV 至少应包含基础头；过滤零字节/损坏片段，避免 ffmpeg 合成直接失败。
        fs::metadata(&seg.path)
            .map(|meta| meta.is_file() && meta.len() > 44)
            .unwrap_or(false)
    };
    let valid_system = system_segments
        .iter()
        .filter(|s| is_valid_audio_segment(s))
        .cloned()
        .collect::<Vec<_>>();
    let valid_mic = mic_segments
        .iter()
        .filter(|s| is_valid_audio_segment(s))
        .cloned()
        .collect::<Vec<_>>();
    let has_system = !valid_system.is_empty();
    let has_mic = !valid_mic.is_empty();
    if !has_system && !has_mic {
        if expected_system_count > 0 || expected_mic_count > 0 {
            log::warn!(
                "音频片段全部无效，跳过音频合成。system: {}/{}，mic: {}/{}",
                valid_system.len(),
                expected_system_count,
                valid_mic.len(),
                expected_mic_count
            );
        }
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
    let mut input_index = 1usize;
    let mut sys_inputs: Vec<(usize, u64)> = Vec::new();
    let mut mic_inputs: Vec<(usize, u64)> = Vec::new();
    for seg in &valid_system {
        cmd.arg("-i").arg(&seg.path);
        sys_inputs.push((input_index, seg.start_ms));
        input_index += 1;
    }
    for seg in &valid_mic {
        cmd.arg("-i").arg(&seg.path);
        mic_inputs.push((input_index, seg.start_ms));
        input_index += 1;
    }
    let sys_delay = sys_inputs.first().map(|(_, d)| *d).unwrap_or(0);
    let mic_delay = mic_inputs.first().map(|(_, d)| *d).unwrap_or(0);
    let switch_meta = |points: &[u64]| {
        if points.is_empty() {
            "none".to_string()
        } else {
            points.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",")
        }
    };
    cmd.arg("-metadata")
        .arg(format!("fy_sys_delay_ms={}", sys_delay))
        .arg("-metadata")
        .arg(format!("fy_mic_delay_ms={}", mic_delay))
        .arg("-metadata")
        .arg(format!("fy_sys_switch_points_ms={}", switch_meta(system_switch_points_ms)))
        .arg("-metadata")
        .arg(format!("fy_mic_switch_points_ms={}", switch_meta(mic_switch_points_ms)));
    let mut filter_parts: Vec<String> = Vec::new();
    let mut sys_labels: Vec<String> = Vec::new();
    for (idx, (input, delay)) in sys_inputs.iter().enumerate() {
        let label = format!("sys{}", idx);
        filter_parts.push(format!(
            "[{i}:a]aresample=async=1:first_pts=0,adelay={d}|{d}[{l}]",
            i = input,
            d = delay,
            l = label
        ));
        sys_labels.push(format!("[{}]", label));
    }
    let sys_out = if !sys_labels.is_empty() {
        if sys_labels.len() == 1 {
            filter_parts.push(format!(
                "{}aresample=async=1:first_pts=0[sysa]",
                sys_labels[0]
            ));
        } else {
            filter_parts.push(format!(
                "{}amix=inputs={}:duration=longest:normalize=0,aresample=async=1:first_pts=0[sysa]",
                sys_labels.join(""),
                sys_labels.len()
            ));
        }
        Some("[sysa]")
    } else {
        None
    };
    let mut mic_labels: Vec<String> = Vec::new();
    for (idx, (input, delay)) in mic_inputs.iter().enumerate() {
        let label = format!("mic{}", idx);
        filter_parts.push(format!(
            "[{i}:a]aresample=async=1:first_pts=0,adelay={d}|{d}[{l}]",
            i = input,
            d = delay,
            l = label
        ));
        mic_labels.push(format!("[{}]", label));
    }
    let mic_out = if !mic_labels.is_empty() {
        if mic_labels.len() == 1 {
            filter_parts.push(format!(
                "{}aresample=async=1:first_pts=0[mica]",
                mic_labels[0]
            ));
        } else {
            filter_parts.push(format!(
                "{}amix=inputs={}:duration=longest:normalize=0,aresample=async=1:first_pts=0[mica]",
                mic_labels.join(""),
                mic_labels.len()
            ));
        }
        Some("[mica]")
    } else {
        None
    };
    let audio_map_label = if let (Some(sys), Some(mic)) = (sys_out, mic_out) {
        filter_parts.push(format!(
            "{}{}amix=inputs=2:duration=longest:normalize=0,aresample=async=1:first_pts=0[aout]",
            sys, mic
        ));
        "[aout]"
    } else if sys_out.is_some() {
        filter_parts.push("[sysa]aresample=async=1:first_pts=0[aout]".to_string());
        "[aout]"
    } else if mic_out.is_some() {
        filter_parts.push("[mica]aresample=async=1:first_pts=0[aout]".to_string());
        "[aout]"
    } else {
        return Ok(());
    };
    cmd.arg("-filter_complex")
        .arg(filter_parts.join(";"))
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg(audio_map_label);
    cmd.arg("-c:v")
        .arg("copy")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("192k")
        .arg(&merged_path);
    let output = cmd
        .output()
        .map_err(|e| AppError::new(ErrorCode::SystemError, "执行系统音频合成失败").with_details(e.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let details = if stderr.is_empty() {
            format!("ffmpeg exit status: {}", output.status)
        } else {
            format!("ffmpeg exit status: {}；stderr: {}", output.status, stderr)
        };
        return Err(AppError::new(ErrorCode::SystemError, "系统音频合成失败").with_details(details));
    }
    if video_path.exists() {
        let _ = fs::remove_file(video_path);
    }
    fs::rename(&merged_path, video_path)
        .map_err(|e| AppError::new(ErrorCode::IoError, "写入合成文件失败").with_details(e.to_string()))?;
    for seg in &valid_system {
        let _ = fs::remove_file(&seg.path);
    }
    for seg in &valid_mic {
        let _ = fs::remove_file(&seg.path);
    }
    Ok(())
}

fn validate_video_input_for_merge(
    ffmpeg_path: &std::path::Path,
    video_path: &PathBuf,
) -> Result<(), AppError> {
    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(video_path)
        .arg("-f")
        .arg("null")
        .arg("-");
    let output = cmd
        .output()
        .map_err(|e| AppError::new(ErrorCode::SystemError, "校验录制视频失败").with_details(e.to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let details = if stderr.is_empty() {
        format!("ffmpeg exit status: {}", output.status)
    } else {
        format!("ffmpeg exit status: {}；stderr: {}", output.status, stderr)
    };
    Err(AppError::new(ErrorCode::SystemError, "录制视频文件无效，无法合成音频").with_details(details))
}

fn is_benign_wgc_stop_error(details: &str) -> bool {
    let lower = details.to_lowercase();
    lower.contains("already stopped")
        || lower.contains("already stopped the capture")
        || lower.contains("capture has been closed")
        || lower.contains("operation is not valid in the current state")
}

fn ensure_system_audio_capture_started(
    app: &AppHandle,
    runtime: &mut crate::features::recording::state::RecordingRuntime,
    output_dir: &PathBuf,
    session_id: &str,
    emit_error_on_fail: bool,
) -> Result<(), String> {
    if runtime.system_audio_thread.is_some() {
        return Ok(());
    }
    let enabled_flag = runtime
        .system_audio_enabled_flag
        .clone()
        .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    runtime.system_audio_enabled_flag = Some(enabled_flag.clone());
    let start_ms = runtime
        .started_instant
        .map(|it| it.elapsed().as_millis() as u64)
        .unwrap_or(0);
    let seg_idx = runtime.system_audio_segments.len();
    if !runtime.system_audio_process_ids.is_empty() {
        let process_ids = runtime.system_audio_process_ids.clone();
        let output_paths = process_ids
            .iter()
            .enumerate()
            .map(|(idx, pid)| {
                output_dir.join(format!(
                    "{}.sys.proc{}.seg{}.wav",
                    session_id, pid, seg_idx + idx
                ))
            })
            .collect::<Vec<_>>();
        let first_try = start_process_loopback_wavs(process_ids, output_paths.clone(), enabled_flag.clone());
        return match first_try {
            Ok(handle) => {
                runtime.system_audio_stop_flag = Some(handle.stop_flag.clone());
                runtime.system_audio_thread = handle.join;
                runtime.system_audio_stream_start_ms = Some(start_ms);
                for p in output_paths {
                    runtime.system_audio_segments.push(crate::features::recording::state::AudioSegment {
                        path: p,
                        start_ms,
                    });
                }
                Ok(())
            }
            Err(e) => {
                if emit_error_on_fail {
                    emit_recording_error(app, Some(session_id), AUDIO_DEVICE_NOT_FOUND, e.as_str());
                }
                Err(e)
            }
        };
    }
    let sys_wav = if seg_idx == 0 {
        output_dir.join(format!("{}.sys.wav", session_id))
    } else {
        output_dir.join(format!("{}.sys.{}.wav", session_id, seg_idx))
    };
    let first_try = start_system_loopback_wav_with_device(runtime.system_audio_device_id.clone(), sys_wav.clone(), enabled_flag.clone());
    let start_result = match first_try {
        Ok(h) => Ok(h),
        Err(first_err) => {
            if runtime.system_audio_device_id.is_some() {
                runtime.system_audio_device_id = None;
                start_system_loopback_wav_with_device(None, sys_wav.clone(), enabled_flag)
                    .map_err(|second_err| format!("{}；回退默认设备失败: {}", first_err, second_err))
            } else {
                Err(first_err)
            }
        }
    };
    match start_result {
        Ok(handle) => {
            runtime.system_audio_wav_path = Some(sys_wav);
            runtime.system_audio_stop_flag = Some(handle.stop_flag.clone());
            runtime.system_audio_thread = handle.join;
            runtime.system_audio_stream_start_ms = Some(start_ms);
            runtime.system_audio_segments.push(crate::features::recording::state::AudioSegment {
                path: runtime.system_audio_wav_path.clone().expect("set"),
                start_ms,
            });
            Ok(())
        }
        Err(e) => {
            if emit_error_on_fail {
                emit_recording_error(app, Some(session_id), AUDIO_DEVICE_NOT_FOUND, e.as_str());
            }
            Err(e)
        }
    }
}

fn ensure_mic_capture_started(
    app: &AppHandle,
    runtime: &mut crate::features::recording::state::RecordingRuntime,
    output_dir: &PathBuf,
    session_id: &str,
    emit_error_on_fail: bool,
) -> Result<(), String> {
    if runtime.mic_audio_thread.is_some() {
        return Ok(());
    }
    let enabled_flag = runtime
        .mic_audio_enabled_flag
        .clone()
        .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    runtime.mic_audio_enabled_flag = Some(enabled_flag.clone());
    let start_ms = runtime
        .started_instant
        .map(|it| it.elapsed().as_millis() as u64)
        .unwrap_or(0);
    let seg_idx = runtime.mic_audio_segments.len();
    let mic_wav = if seg_idx == 0 {
        output_dir.join(format!("{}.mic.wav", session_id))
    } else {
        output_dir.join(format!("{}.mic.{}.wav", session_id, seg_idx))
    };
    let first_try = start_microphone_wav_with_device(runtime.mic_audio_device_id.clone(), mic_wav.clone(), enabled_flag.clone());
    let start_result = match first_try {
        Ok(h) => Ok(h),
        Err(first_err) => {
            if runtime.mic_audio_device_id.is_some() {
                runtime.mic_audio_device_id = None;
                start_microphone_wav_with_device(None, mic_wav.clone(), enabled_flag)
                    .map_err(|second_err| format!("{}；回退默认设备失败: {}", first_err, second_err))
            } else {
                Err(first_err)
            }
        }
    };
    match start_result {
        Ok(handle) => {
            runtime.mic_audio_wav_path = Some(mic_wav);
            runtime.mic_audio_stop_flag = Some(handle.stop_flag.clone());
            runtime.mic_audio_thread = handle.join;
            runtime.mic_audio_stream_start_ms = Some(start_ms);
            runtime.mic_audio_segments.push(crate::features::recording::state::AudioSegment {
                path: runtime.mic_audio_wav_path.clone().expect("set"),
                start_ms,
            });
            Ok(())
        }
        Err(e) => {
            if emit_error_on_fail {
                emit_recording_error(app, Some(session_id), AUDIO_DEVICE_NOT_FOUND, e.as_str());
            }
            Err(e)
        }
    }
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

pub fn list_audio_process_items() -> Result<Vec<AudioProcessItem>, AppError> {
    Ok(list_audio_processes()
        .into_iter()
        .map(|p| AudioProcessItem {
            pid: p.pid,
            name: p.name,
        })
        .collect::<Vec<_>>())
}

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
                push_stderr_tail(&mut runtime, &line);
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
                        let err_msg = build_exit_error_with_stderr(status.to_string(), &runtime);
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
        let mut rollback_starting = |public_message: &str, details: String| -> AppError {
            runtime.reset_to_idle();
            runtime.last_error = Some(details.clone());
            emit_recording_state_changed(app, None, runtime.phase.as_str(), 0);
            AppError::new(ErrorCode::ValidationError, public_message).with_details(details)
        };

        let target_type = request
            .target_type
            .clone()
            .unwrap_or_else(|| "screen".to_string())
            .to_lowercase();
        let target_id = request.target_id.clone().unwrap_or_default();
        let mut window_wgc_handle = None;
        let mut args: Vec<String> = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "warning".into(),
            "-y".into(),
        ];
        match target_type.as_str() {
            "window" => {
                if target_id.trim().is_empty() {
                    return Err(rollback_starting("窗口录制目标不能为空", "target_id is empty".to_string()));
                }
                let handle = start_window_capture_to_mp4(
                    target_id.trim(),
                    tmp_path.clone(),
                    fps,
                    video_bitrate,
                    capture_cursor,
                )
                    .map_err(|e| rollback_starting("启动窗口源录制失败", e))?;
                window_wgc_handle = Some(handle);
            }
            "region" => {
                let (x, y, width, height) = parse_region_target(&target_id).ok_or_else(|| {
                    rollback_starting("区域录制参数无效", format!("target_id={}", target_id))
                })?;
                let (x, y, width, height) = normalize_region_to_virtual_screen(x, y, width, height)
                    .ok_or_else(|| rollback_starting("区域录制参数无效", "virtual screen unavailable".to_string()))?;
                args.push("-f".into());
                args.push("gdigrab".into());
                args.push("-framerate".into());
                args.push(format!("{}", fps));
                args.push("-draw_mouse".into());
                args.push(if capture_cursor { "1".into() } else { "0".into() });
                args.push("-offset_x".into());
                args.push(x.to_string());
                args.push("-offset_y".into());
                args.push(y.to_string());
                args.push("-video_size".into());
                args.push(format!("{}x{}", width, height));
                args.push("-i".into());
                args.push("desktop".into());
            }
            _ => {
                args.push("-f".into());
                args.push("gdigrab".into());
                args.push("-framerate".into());
                args.push(format!("{}", fps));
                args.push("-draw_mouse".into());
                args.push(if capture_cursor { "1".into() } else { "0".into() });
                args.push("-i".into());
                args.push("desktop".into());
            }
        }
        let (child_opt, stderr_opt) = if target_type != "window" {
            // 删除 ffmpeg 系统音频输入路径，改为 Rust 原生 WASAPI 录制（后处理合成）
            args.push("-map".to_string());
            args.push("0:v:0".to_string());
            // yuv420p + libx264 要求偶数宽高，区域框选常出现奇数尺寸，统一做偶数对齐避免 -22
            args.push("-vf".to_string());
            args.push("scale=trunc(iw/2)*2:trunc(ih/2)*2".to_string());
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
            (Some(child), stderr)
        } else {
            (None, None)
        };
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
        runtime.system_audio_device_id = system_audio_device_id.clone();
        runtime.system_audio_process_ids = request
            .system_audio_process_ids
            .clone()
            .unwrap_or_default()
            .into_iter()
            .filter(|pid| *pid > 0)
            .collect::<Vec<_>>();
        runtime.mic_audio_device_id = request.microphone_device_id.clone();
        runtime.system_audio_enabled_flag = Some(Arc::new(AtomicBool::new(capture_system_audio)));
        runtime.mic_audio_enabled_flag = Some(Arc::new(AtomicBool::new(capture_microphone)));
        runtime.system_audio_ever_enabled = capture_system_audio;
        runtime.mic_audio_ever_enabled = capture_microphone;
        runtime.system_audio_stream_start_ms = None;
        runtime.mic_audio_stream_start_ms = None;
        runtime.system_audio_switch_points_ms.clear();
        runtime.mic_audio_switch_points_ms.clear();
        runtime.system_audio_segments.clear();
        runtime.mic_audio_segments.clear();
        if capture_system_audio {
            runtime.system_audio_switch_points_ms.push(0);
        }
        if capture_microphone {
            runtime.mic_audio_switch_points_ms.push(0);
        }
        // 系统音频允许静音常驻采集；麦克风未开启时不占用设备，避免系统显示“麦克风正在使用”
        let _ = ensure_system_audio_capture_started(app, &mut runtime, &output_dir, &session_id, capture_system_audio);
        if capture_microphone {
            let _ = ensure_mic_capture_started(app, &mut runtime, &output_dir, &session_id, true);
        }
        runtime.process = child_opt;
        if let Some(handle) = window_wgc_handle {
            runtime.wgc_stop_flag = Some(handle.stop_flag);
            runtime.wgc_thread = Some(handle.join);
        }
        let started_at_ms = runtime.started_at_ms;
        emit_recording_state_changed(app, Some(&session_id), runtime.phase.as_str(), 0);
        drop(runtime);
        if let Some(stderr) = stderr_opt {
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

    let mut fatal_error: Option<AppError> = None;
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

    if let Some(flag) = runtime.wgc_stop_flag.as_ref() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
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
    if let Some(join) = runtime.wgc_thread.take() {
        match join.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if is_benign_wgc_stop_error(&e) {
                    log::warn!("窗口录制停止返回可忽略状态: {}", e);
                } else if fatal_error.is_none() {
                    fatal_error = Some(AppError::new(ErrorCode::SystemError, "窗口录制停止失败").with_details(e));
                }
            }
            Err(_) => {
                if fatal_error.is_none() {
                    fatal_error = Some(AppError::new(ErrorCode::SystemError, "窗口录制线程异常退出"));
                }
            }
        }
    }
    runtime.wgc_stop_flag = None;

    let output_tmp = runtime.output_path_tmp.clone();
    let output_final = runtime.output_path_final.clone();
    let output_final_for_result = output_final.clone();
    if let (Some(output_tmp), Some(output_final)) = (output_tmp.as_ref(), output_final.as_ref()) {
        if output_final.exists() {
            let _ = fs::remove_file(output_final);
        }
        if let Err(e) = fs::rename(output_tmp, output_final) {
            if fatal_error.is_none() {
                fatal_error =
                    Some(AppError::new(ErrorCode::IoError, "重命名录制文件失败").with_details(e.to_string()));
            }
        } else if let Err(e) = validate_video_input_for_merge(&ffmpeg_path, output_final) {
            if fatal_error.is_none() {
                fatal_error = Some(e);
            }
        }
    } else if fatal_error.is_none() {
        fatal_error = Some(AppError::new(ErrorCode::SystemError, "录制输出路径不存在"));
    }

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
    let sys_segments = if runtime.system_audio_ever_enabled {
        runtime.system_audio_segments.clone()
    } else {
        Vec::new()
    };
    let mic_segments = if runtime.mic_audio_ever_enabled {
        runtime.mic_audio_segments.clone()
    } else {
        Vec::new()
    };
    if fatal_error.is_none() {
        if let Some(output_final) = output_final.as_ref() {
            if let Err(e) = merge_system_audio_into_video(
                &ffmpeg_path,
                output_final,
                &sys_segments,
                &mic_segments,
                &runtime.system_audio_switch_points_ms,
                &runtime.mic_audio_switch_points_ms,
            ) {
                let detail = e.details.clone().unwrap_or_default();
                let msg = if detail.is_empty() {
                    format!("音频合成失败，已保留视频文件: {}", e.message)
                } else {
                    format!("音频合成失败，已保留视频文件: {}；{}", e.message, detail)
                };
                runtime.last_error = Some(msg.clone());
                emit_recording_error(app, Some(session_id.as_str()), RECORDING_PROCESS_EXITED, &msg);
            }
        } else if fatal_error.is_none() {
            fatal_error = Some(AppError::new(ErrorCode::SystemError, "录制输出路径不存在"));
        }
    }
    // 统一清理录制过程产生的音频片段，避免 stop 后残留 *.sys.wav / *.mic.wav。
    let mut audio_segment_paths = HashSet::<PathBuf>::new();
    for seg in &runtime.system_audio_segments {
        audio_segment_paths.insert(seg.path.clone());
    }
    for seg in &runtime.mic_audio_segments {
        audio_segment_paths.insert(seg.path.clone());
    }
    for path in audio_segment_paths {
        let _ = fs::remove_file(path);
    }

    if let Some(paused_at) = runtime.paused_at_instant {
        runtime.paused_total_ms = runtime
            .paused_total_ms
            .saturating_add(paused_at.elapsed().as_millis() as u64);
        runtime.paused_at_instant = None;
    }

    let mut success_result: Option<RecordingStopResult> = None;
    if fatal_error.is_none() {
        if let Some(output_final) = output_final_for_result.as_ref() {
            let duration_ms = runtime.snapshot().elapsed_ms;
            let file_size_bytes = fs::metadata(output_final).map(|m| m.len()).unwrap_or(0);
            let result = RecordingStopResult {
                session_id: session_id.clone(),
                output_path: output_final.to_string_lossy().to_string(),
                duration_ms,
                file_size_bytes,
            };
            emit_recording_finished(app, &result);
            success_result = Some(result);
        } else {
            fatal_error = Some(AppError::new(ErrorCode::SystemError, "录制输出路径不存在"));
        }
    }

    runtime.reset_to_idle();
    emit_recording_state_changed(app, None, runtime.phase.as_str(), 0);

    if let Some(err) = fatal_error {
        return Err(err);
    }
    success_result.ok_or_else(|| AppError::new(ErrorCode::SystemError, "录制停止失败"))
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
    if let Some(flag) = runtime.wgc_stop_flag.take() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(join) = runtime.wgc_thread.take() {
        let _ = join.join();
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
    if runtime.process.is_none() && runtime.wgc_thread.is_some() {
        return Err(AppError::new(
            ErrorCode::ValidationError,
            "窗口源录制暂不支持暂停，请直接停止录制",
        ));
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
    if runtime.process.is_none() && runtime.wgc_thread.is_some() {
        return Err(AppError::new(
            ErrorCode::ValidationError,
            "窗口源录制暂不支持暂停恢复，请继续录制或停止",
        ));
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

pub fn update_audio_capture(
    app: &AppHandle,
    state_arc: Arc<Mutex<SharedAppState>>,
    capture_system_audio: Option<bool>,
    system_audio_device_id: Option<String>,
    capture_microphone: Option<bool>,
    microphone_device_id: Option<String>,
) -> Result<(), AppError> {
    let runtime_arc = {
        let state_guard = lock_arc_mutex(&state_arc);
        state_guard.recording_runtime.clone()
    };
    let mut runtime = lock_arc_mutex(&runtime_arc);
    if runtime.phase != RecordingPhase::Recording && runtime.phase != RecordingPhase::Paused {
        return Err(AppError::new(ErrorCode::ValidationError, "当前没有正在进行的录制任务"));
    }
    let session_id = runtime
        .session_id
        .clone()
        .ok_or_else(|| AppError::new(ErrorCode::ValidationError, "录制会话不存在"))?;
    let output_dir = runtime
        .output_path_tmp
        .as_ref()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .ok_or_else(|| AppError::new(ErrorCode::ValidationError, "录制输出目录不存在"))?;

    let current_sys_enabled = runtime
        .system_audio_enabled_flag
        .as_ref()
        .map(|f| f.load(Ordering::SeqCst))
        .unwrap_or(false);
    let current_mic_enabled = runtime
        .mic_audio_enabled_flag
        .as_ref()
        .map(|f| f.load(Ordering::SeqCst))
        .unwrap_or(false);
    let requested_sys_device = system_audio_device_id
        .as_ref()
        .map(|id| id.trim().to_string())
        .map(|id| if id.is_empty() { None } else { Some(id) })
        .unwrap_or_else(|| runtime.system_audio_device_id.clone());
    let requested_mic_device = microphone_device_id
        .as_ref()
        .map(|id| id.trim().to_string())
        .map(|id| if id.is_empty() { None } else { Some(id) })
        .unwrap_or_else(|| runtime.mic_audio_device_id.clone());
    let should_enable_sys = capture_system_audio.unwrap_or(current_sys_enabled);
    let should_enable_mic = capture_microphone.unwrap_or(current_mic_enabled);
    let elapsed_now_ms = runtime.snapshot().elapsed_ms;
    let sys_device_changed = requested_sys_device != runtime.system_audio_device_id;
    let mic_device_changed = requested_mic_device != runtime.mic_audio_device_id;

    if let Some(v) = capture_system_audio {
        if v != current_sys_enabled {
            runtime.system_audio_switch_points_ms.push(elapsed_now_ms);
            if runtime.system_audio_switch_points_ms.len() > 64 {
                let _ = runtime.system_audio_switch_points_ms.remove(0);
            }
        }
        if let Some(flag) = runtime.system_audio_enabled_flag.as_ref() {
            flag.store(v, Ordering::SeqCst);
        }
        if v {
            runtime.system_audio_ever_enabled = true;
        }
    }
    if let Some(v) = capture_microphone {
        if v != current_mic_enabled {
            runtime.mic_audio_switch_points_ms.push(elapsed_now_ms);
            if runtime.mic_audio_switch_points_ms.len() > 64 {
                let _ = runtime.mic_audio_switch_points_ms.remove(0);
            }
        }
        if let Some(flag) = runtime.mic_audio_enabled_flag.as_ref() {
            flag.store(v, Ordering::SeqCst);
        }
        if v {
            runtime.mic_audio_ever_enabled = true;
        }
    }
    runtime.system_audio_device_id = requested_sys_device;
    runtime.mic_audio_device_id = requested_mic_device;
    if sys_device_changed && runtime.system_audio_thread.is_some() {
        if let Some(flag) = runtime.system_audio_stop_flag.take() {
            flag.store(true, Ordering::SeqCst);
        }
        if let Some(join) = runtime.system_audio_thread.take() {
            let _ = join.join();
        }
        runtime.system_audio_stream_start_ms = Some(elapsed_now_ms);
    }
    if mic_device_changed && runtime.mic_audio_thread.is_some() {
        if let Some(flag) = runtime.mic_audio_stop_flag.take() {
            flag.store(true, Ordering::SeqCst);
        }
        if let Some(join) = runtime.mic_audio_thread.take() {
            let _ = join.join();
        }
        runtime.mic_audio_stream_start_ms = Some(elapsed_now_ms);
    }
    if runtime.system_audio_thread.is_none() {
        ensure_system_audio_capture_started(app, &mut runtime, &output_dir, &session_id, should_enable_sys).map_err(|e| {
            AppError::new(ErrorCode::SystemError, format!("开启系统音频失败: {}", e)).with_details(e)
        })?;
    }
    if !should_enable_mic && runtime.mic_audio_thread.is_some() {
        if let Some(flag) = runtime.mic_audio_stop_flag.take() {
            flag.store(true, Ordering::SeqCst);
        }
        if let Some(join) = runtime.mic_audio_thread.take() {
            let _ = join.join();
        }
        runtime.mic_audio_stream_start_ms = None;
    }
    if should_enable_mic && runtime.mic_audio_thread.is_none() {
        ensure_mic_capture_started(app, &mut runtime, &output_dir, &session_id, true).map_err(|e| {
            AppError::new(ErrorCode::SystemError, format!("开启麦克风失败: {}", e)).with_details(e)
        })?;
    }
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
                target_x: None,
                target_y: None,
                target_width: None,
                target_height: None,
                capture_cursor: Some(true),
                capture_system_audio: Some(false),
                system_audio_device_id: None,
                system_audio_process_ids: None,
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

pub fn open_recording_folder(app: &AppHandle, state_arc: Arc<Mutex<SharedAppState>>) -> Result<(), AppError> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = state_arc;
        return Err(AppError::new(ErrorCode::SystemError, "当前平台暂不支持打开录制目录"));
    }
    #[cfg(target_os = "windows")]
    {
        let now_ms = now_unix_ms() as u64;
        let last_ms = LAST_OPEN_FOLDER_MS.load(Ordering::Relaxed);
        if last_ms > 0 && now_ms.saturating_sub(last_ms) < 1200 {
            return Ok(());
        }
        LAST_OPEN_FOLDER_MS.store(now_ms, Ordering::Relaxed);
        let output_dir = {
            let state_guard = lock_arc_mutex(&state_arc);
            resolve_output_dir(&state_guard, None)?
        };
        fs::create_dir_all(&output_dir)
            .map_err(|e| AppError::new(ErrorCode::IoError, "创建录制目录失败").with_details(e.to_string()))?;
        let output_dir_string = output_dir.to_string_lossy().to_string();
        app.opener()
            .open_path(output_dir_string, None::<&str>)
            .map_err(|e| AppError::new(ErrorCode::SystemError, "打开录制目录失败").with_details(e.to_string()))?;
        Ok(())
    }
}
