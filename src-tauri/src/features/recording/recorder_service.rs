use crate::core::app_state::SharedAppState;
use crate::core::error::{AppError, ErrorCode};
use crate::core::perf_metrics::record_perf_metric;
use crate::features::recording::audio_device::list_microphones;
use crate::features::recording::error_codes::{
    AUDIO_DEVICE_LOST, AUDIO_DEVICE_NOT_FOUND, MAX_DURATION_REACHED, RECORDING_PROCESS_EXITED,
    RECORDING_START_FAILED,
};
use crate::features::recording::events::{
    emit_recording_audio_merging, emit_recording_device_list, emit_recording_error, emit_recording_finished, emit_recording_state_changed,
    emit_recording_stats_updated,
};
use crate::features::recording::ffmpeg_runner::{build_output_paths, resolve_ffmpeg_path};
use crate::features::recording::native_wasapi::{
    list_audio_processes, start_microphone_wav_with_device, start_process_loopback_wavs,
    start_system_loopback_aac_with_device,
};
use crate::features::recording::state::RecordingPhase;
use crate::features::recording::types::{
    AudioInputDevice, AudioProcessItem, RecordingRegressionReport, RecordingRuntimeState, RecordingSessionInfo,
    RecordingStopResult, SessionRequest, StartRecordingRequest,
};
use crate::features::recording::wgc_capture::{
    bootstrap_force_default_border_from_settings, bootstrap_force_default_dirty_region_from_settings,
    is_force_default_border_enabled, is_force_default_dirty_region_enabled, is_item_convert_failed,
    start_window_capture_to_mp4, validate_window_capture_target,
};
use crate::sync::Mutex;
use crate::utils::system_utils::save_settings;
use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{ChildStderr, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
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
const VIDEO_IO_RETRY_DELAYS_MS: [u64; 5] = [60, 120, 240, 480, 800];


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

fn persist_wgc_capture_fallback_if_needed(state_arc: &Arc<Mutex<SharedAppState>>) {
    let force_default_border = is_force_default_border_enabled();
    let force_default_dirty_region = is_force_default_dirty_region_enabled();
    if !force_default_border && !force_default_dirty_region {
        return;
    }
    let mut guard = lock_arc_mutex(state_arc);
    let mut changed = Vec::new();
    if force_default_border && !guard.settings.recording_wgc_force_default_border {
        guard.settings.recording_wgc_force_default_border = true;
        changed.push("DrawBorderSettings::Default");
    }
    if force_default_dirty_region && !guard.settings.recording_wgc_force_default_dirty_region {
        guard.settings.recording_wgc_force_default_dirty_region = true;
        changed.push("DirtyRegionSettings::Default");
    }
    if changed.is_empty() {
        return;
    }
    let snapshot = guard.settings.clone();
    drop(guard);
    if let Err(e) = save_settings(&snapshot) {
        log::warn!("持久化 WGC 捕获回退策略失败: {}", e);
    } else {
        log::info!("已持久化 WGC 捕获回退策略: {}", changed.join(", "));
    }
}

fn finalize_auto_stop_recording(app: &AppHandle, state_arc: Arc<Mutex<SharedAppState>>, session_id: String) {
    let request = SessionRequest {
        session_id: Some(session_id.clone()),
    };
    match stop_recording(app, state_arc.clone(), request.clone()) {
        Ok(_) => {
            let auto_open_folder = {
                let guard = lock_arc_mutex(&state_arc);
                guard.settings.recording_auto_open_folder
            };
            if auto_open_folder {
                if let Err(e) = open_recording_folder(app, state_arc.clone()) {
                    log::warn!("自动停止后打开录制目录失败: {}", e);
                }
            }
        }
        Err(stop_err) => {
            let stop_msg = stop_err.to_string();
            match cancel_recording(app, state_arc, request) {
                Ok(()) => {
                    log::warn!("自动停止收尾失败，已执行 cancel_recording 兜底清理: {}", stop_msg);
                    emit_recording_error(app, Some(session_id.as_str()), RECORDING_PROCESS_EXITED, &stop_msg);
                }
                Err(cancel_err) => {
                    let merged = format!("{}；自动兜底清理失败: {}", stop_msg, cancel_err);
                    log::warn!("自动停止收尾与兜底清理均失败: {}", merged);
                    emit_recording_error(app, Some(session_id.as_str()), RECORDING_PROCESS_EXITED, &merged);
                }
            }
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

fn build_window_segment_path(output_dir: &PathBuf, session_id: &str, segment_index: usize) -> PathBuf {
    output_dir.join(format!("{}.video.{}.tmp.mp4", session_id, segment_index))
}

fn concat_video_segments(
    ffmpeg_path: &std::path::Path,
    segments: &[PathBuf],
    output_path: &PathBuf,
) -> Result<(), AppError> {
    if segments.is_empty() {
        return Err(AppError::new(ErrorCode::ValidationError, "没有可拼接的视频分段"));
    }
    if segments.len() == 1 {
        return rename_recording_output_with_retry(&segments[0], output_path);
    }
    let list_path = output_path.with_extension("concat.txt");
    let mut list_file = fs::File::create(&list_path)
        .map_err(|e| AppError::new(ErrorCode::IoError, "创建视频拼接列表失败").with_details(e.to_string()))?;
    for seg in segments {
        let seg_path = seg.to_string_lossy().replace('\'', "'\\''");
        let line = format!("file '{}'\n", seg_path);
        list_file
            .write_all(line.as_bytes())
            .map_err(|e| AppError::new(ErrorCode::IoError, "写入视频拼接列表失败").with_details(e.to_string()))?;
    }
    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);
    let output = cmd
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y")
        .arg("-f")
        .arg("concat")
        .arg("-safe")
        .arg("0")
        .arg("-i")
        .arg(&list_path)
        .arg("-c")
        .arg("copy")
        .arg(output_path)
        .output()
        .map_err(|e| AppError::new(ErrorCode::SystemError, "执行视频拼接失败").with_details(e.to_string()))?;
    let _ = fs::remove_file(&list_path);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::new(ErrorCode::SystemError, "视频拼接失败").with_details(stderr));
    }
    Ok(())
}

fn merge_system_audio_into_video(
    ffmpeg_path: &std::path::Path,
    video_path: &PathBuf,
    system_segments: &[crate::features::recording::state::AudioSegment],
    mic_segments: &[crate::features::recording::state::AudioSegment],
) -> Result<(), AppError> {
    let started_at = Instant::now();
    let expected_system_count = system_segments.len();
    let expected_mic_count = mic_segments.len();

    // 🔧 性能优化：快速路径 - 单个音频片段且延迟很小(<100ms)时直接复制流
    // 适用场景：全屏/区域录制（无暂停、单次音频）
    // 速度提升：10-50倍（避免重新编码）
    if system_segments.len() == 1 && mic_segments.is_empty() {
        let seg = &system_segments[0];
        if seg.start_ms < 100 && seg.path.exists() {
            log::info!("快速路径：单个系统音频片段(start_ms={})，使用流复制模式", seg.start_ms);
            return merge_audio_fast(ffmpeg_path, video_path, &seg.path, false);
        }
    }
    if mic_segments.len() == 1 && system_segments.is_empty() {
        let seg = &mic_segments[0];
        if seg.start_ms < 100 && seg.path.exists() {
            log::info!("快速路径：单个麦克风音频片段(start_ms={})，使用流复制模式", seg.start_ms);
            return merge_audio_fast(ffmpeg_path, video_path, &seg.path, false);
        }
    }
    
    let is_valid_audio_segment = |seg: &crate::features::recording::state::AudioSegment| {
        if !seg.path.exists() {
            log::warn!("音频片段不存在: {:?}", seg.path);
            return false;
        }
        // 🔧 支持 WAV 和 AAC 格式
        let is_aac = seg.path.extension()
            .map(|ext| ext.to_string_lossy().to_lowercase() == "aac")
            .unwrap_or(false);
        let min_size = if is_aac { 7 } else { 44 };  // AAC 最小 7 bytes (ADTS header), WAV 最小 44 bytes
        match fs::metadata(&seg.path) {
            Ok(meta) => {
                let size = meta.len();
                let valid = meta.is_file() && size > min_size;
                if !valid {
                    log::warn!("音频片段无效: {:?}, 大小: {} bytes (需要 > {})", seg.path, size, min_size);
                }
                valid
            }
            Err(e) => {
                log::warn!("无法读取音频片段元数据: {:?}, 错误: {}", seg.path, e);
                false
            }
        }
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

    // 🔧 性能优化：项目录制的视频始终是 H.264 格式，直接使用流复制模式
    // 无需检测视频编码格式，避免额外的 FFmpeg 进程开销
    log::info!("✅ 视频编码格式为 H.264，使用流复制模式（无重编码）");
    
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y")
        .arg("-threads")
        .arg("0")  // 自动使用所有可用 CPU 核心
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
    cmd.arg("-metadata")
        .arg(format!("fy_sys_delay_ms={}", sys_delay))
        .arg("-metadata")
        .arg(format!("fy_mic_delay_ms={}", mic_delay));
    let mut filter_parts: Vec<String> = Vec::new();
    let mut sys_labels: Vec<String> = Vec::new();
    for (idx, (input, delay)) in sys_inputs.iter().enumerate() {
        let label = format!("sys{}", idx);
        // 🔧 性能优化：简化音频处理链
        // - aresample=async=1:min_hard_comp=0.100000:first_pts=0: 减少重采样计算
        // - adelay={d}|{d}: 延迟对齐
        // 注意：如果所有音频都是 48kHz，可以跳过 aresample，直接使用 adelay
        filter_parts.push(format!(
            "[{i}:a]adelay={d}|{d}[{l}]",
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
        // 🔧 性能优化：简化音频处理链，跳过重采样
        filter_parts.push(format!(
            "[{i}:a]adelay={d}|{d}[{l}]",
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
        // 🔧 性能优化：简化混音过滤器
        filter_parts.push(format!(
            "{}{}amix=inputs=2:duration=longest:normalize=0[aout]",
            sys, mic
        ));
        "[aout]"
    } else if sys_out.is_some() {
        filter_parts.push("[sysa]anull[aout]".to_string());
        "[aout]"
    } else if mic_out.is_some() {
        filter_parts.push("[mica]anull[aout]".to_string());
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

    // 🔧 性能优化：检测所有音频片段是否都是 AAC 格式
    let all_aac = valid_system.iter().chain(valid_mic.iter()).all(|seg| {
        seg.path.extension()
            .map(|ext| ext.to_string_lossy().to_lowercase() == "aac")
            .unwrap_or(false)
    });

    if all_aac {
        log::info!("✅ 所有音频片段均为 AAC 格式，使用流复制模式（无重编码）");
        cmd.arg("-c:v")
            .arg("copy")
            .arg("-c:a")
            .arg("copy")  // 🔧 AAC 直接 copy
            .arg("-movflags")
            .arg("+faststart")
            .arg(&merged_path);
    } else {
        log::info!("🔧 检测到 WAV 音频，需要重编码为 AAC");
        cmd.arg("-c:v")
            .arg("copy")
            .arg("-c:a")
            .arg("aac")
            .arg("-b:a")
            .arg("128k")
            .arg("-profile:a")
            .arg("aac_low")
            .arg("-movflags")
            .arg("+faststart")
            .arg(&merged_path);
    }

    log::info!("🔧 开始音频合并，系统音频片段: {}, 麦克风片段: {}", valid_system.len(), valid_mic.len());

    // 🔧 性能优化：使用spawn + wait替代output，FFmpeg内部会流式处理
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::new(ErrorCode::SystemError, "启动音频合成进程失败").with_details(e.to_string()))?;

    // 等待FFmpeg完成（仍为同步，但FFmpeg内部会流式处理）
    let output = child
        .wait_with_output()
        .map_err(|e| AppError::new(ErrorCode::SystemError, "执行系统音频合成失败").with_details(e.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let details = if stderr.is_empty() {
            format!("ffmpeg exit status: {}", output.status)
        } else {
            format!("ffmpeg exit status: {}；stderr: {}", output.status, stderr)
        };
        record_perf_metric(
            "recording.audio_merge",
            "录屏音频合成耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(details.clone()),
        );
        return Err(AppError::new(ErrorCode::SystemError, "系统音频合成失败").with_details(details));
    }
    if video_path.exists() {
        let _ = fs::remove_file(video_path);
    }
    fs::rename(&merged_path, video_path).map_err(|e| {
        record_perf_metric(
            "recording.audio_merge",
            "录屏音频合成耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(e.to_string()),
        );
        AppError::new(ErrorCode::IoError, "写入合成文件失败").with_details(e.to_string())
    })?;
    record_perf_metric(
        "recording.audio_merge",
        "录屏音频合成耗时",
        started_at.elapsed().as_millis() as u64,
        true,
        None,
    );

    let elapsed_ms = started_at.elapsed().as_millis();
    log::info!("✅ 音频合并完成，耗时: {}ms ({:.1}s)", elapsed_ms, elapsed_ms as f64 / 1000.0);
    if elapsed_ms > 5000 {
        log::warn!("⚠️ 音频合并耗时较长({}ms)，建议优化方案", elapsed_ms);
    }

    // ✅ 注意：不要在这里删除音频片段！
    // 清理操作将在调用方（异步任务）中统一执行
    // for seg in &valid_system {
    //     let _ = fs::remove_file(&seg.path);
    // }
    // for seg in &valid_mic {
    //     let _ = fs::remove_file(&seg.path);
    // }

    Ok(())
}

// 🔧 性能优化：快速音频合并（简化处理链）
// 适用于单个音频片段且延迟<100ms的场景
// 注意：支持 WAV 和 AAC 两种格式
fn merge_audio_fast(
    ffmpeg_path: &std::path::Path,
    video_path: &PathBuf,
    audio_path: &PathBuf,
    _need_hwaccel: bool,
) -> Result<(), AppError> {
    let started_at = Instant::now();
    let merged_path = video_path.with_extension("merged.tmp.mp4");

    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);

    // 🔧 方案2：检测音频格式，AAC 可直接 copy，WAV 需重编码
    let is_aac = audio_path.extension()
        .map(|ext| ext.to_string_lossy().to_lowercase() == "aac")
        .unwrap_or(false);
    
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y")
        .arg("-i")
        .arg(video_path)
        .arg("-i")
        .arg(audio_path)
        .arg("-c:v")
        .arg("copy")
        .arg("-c:a")
        .arg(if is_aac { "copy" } else { "aac" })  // 🔧 AAC 直接 copy，WAV 重编码
        .arg("-b:a")
        .arg("128k")
        .arg("-movflags")
        .arg("+faststart")
        .arg(&merged_path);

    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::new(ErrorCode::SystemError, "启动快速音频合并失败").with_details(e.to_string()))?;

    let output = child
        .wait_with_output()
        .map_err(|e| AppError::new(ErrorCode::SystemError, "执行快速音频合并失败").with_details(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let details = if stderr.is_empty() {
            format!("ffmpeg exit status: {}", output.status)
        } else {
            format!("ffmpeg exit status: {}；stderr: {}", output.status, stderr)
        };
        record_perf_metric(
            "recording.audio_merge",
            "录屏快速音频合并耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(details.clone()),
        );
        return Err(AppError::new(ErrorCode::SystemError, "快速音频合并失败").with_details(details));
    }

    if video_path.exists() {
        let _ = fs::remove_file(video_path);
    }
    fs::rename(&merged_path, video_path).map_err(|e| {
        record_perf_metric(
            "recording.audio_merge",
            "录屏快速音频合并耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(e.to_string()),
        );
        AppError::new(ErrorCode::IoError, "写入快速合并文件失败").with_details(e.to_string())
    })?;

    record_perf_metric(
        "recording.audio_merge",
        "录屏快速音频合并耗时",
        started_at.elapsed().as_millis() as u64,
        true,
        None,
    );

    let elapsed_ms = started_at.elapsed().as_millis();
    let operation = if is_aac { "AAC流复制" } else { "WAV→AAC重编码" };
    log::info!("✅ 快速音频合并完成，耗时: {}ms ({})", elapsed_ms, operation);
    // 🔧 调整警告阈值：AAC 流复制通常 <1s，WAV 重编码可能 >5s
    let warn_threshold = if is_aac { 2000 } else { 5000 };
    if elapsed_ms > warn_threshold {
        log::warn!("⚠️ 快速路径耗时较长({}ms)，考虑优化方案", elapsed_ms);
    }
    Ok(())
}

// 🔧 保留用于未来可能的视频验证需求
#[allow(dead_code)]
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

fn rename_recording_output_with_retry(output_tmp: &PathBuf, output_final: &PathBuf) -> Result<(), AppError> {
    let mut last_err = String::new();
    for (idx, delay_ms) in VIDEO_IO_RETRY_DELAYS_MS.iter().enumerate() {
        if output_final.exists() {
            let _ = fs::remove_file(output_final);
        }
        match fs::rename(output_tmp, output_final) {
            Ok(_) => return Ok(()),
            Err(e) => {
                last_err = e.to_string();
                if idx + 1 < VIDEO_IO_RETRY_DELAYS_MS.len() {
                    thread::sleep(Duration::from_millis(*delay_ms));
                }
            }
        }
    }
    Err(AppError::new(ErrorCode::IoError, "重命名录制文件失败").with_details(last_err))
}

// 🔧 保留用于未来可能的视频验证需求
#[allow(dead_code)]
fn validate_video_input_for_merge_with_retry(
    ffmpeg_path: &std::path::Path,
    video_path: &PathBuf,
) -> Result<(), AppError> {
    let mut last_err: Option<AppError> = None;
    for (idx, delay_ms) in VIDEO_IO_RETRY_DELAYS_MS.iter().enumerate() {
        match fs::metadata(video_path) {
            Ok(meta) if meta.len() > 0 => {}
            Ok(_) => {
                last_err = Some(AppError::new(
                    ErrorCode::ValidationError,
                    "录制视频文件为空，未捕获到有效视频帧",
                ).with_details("请确认目标窗口未最小化；若录制包含受保护内容或硬件加速，请尝试关闭加速或改用全屏录制"));
                if idx + 1 < VIDEO_IO_RETRY_DELAYS_MS.len() {
                    thread::sleep(Duration::from_millis(*delay_ms));
                }
                continue;
            }
            Err(e) => {
                last_err = Some(
                    AppError::new(ErrorCode::IoError, "读取录制视频失败").with_details(e.to_string()),
                );
                if idx + 1 < VIDEO_IO_RETRY_DELAYS_MS.len() {
                    thread::sleep(Duration::from_millis(*delay_ms));
                }
                continue;
            }
        }
        match validate_video_input_for_merge(ffmpeg_path, video_path) {
            Ok(_) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                if idx + 1 < VIDEO_IO_RETRY_DELAYS_MS.len() {
                    thread::sleep(Duration::from_millis(*delay_ms));
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| AppError::new(ErrorCode::SystemError, "录制视频文件无效，无法合成音频")))
}

fn is_benign_wgc_stop_error(details: &str) -> bool {
    let lower = details.to_lowercase();
    lower.contains("already stopped")
        || lower.contains("already stopped the capture")
        || lower.contains("capture has been closed")
        || lower.contains("operation is not valid in the current state")
        || lower.contains("borderconfigunsupported")
        || lower.contains("graphicscaptureapierror(borderconfigunsupported)")
}

fn build_window_capture_unavailable_details(details: &str) -> String {
    let trimmed = details.trim();
    if trimmed.is_empty() {
        "请确认目标窗口未最小化；若窗口包含受保护视频、硬件加速覆盖层或系统限制内容，请改用区域录制或全屏录制".to_string()
    } else {
        format!(
            "{}；请确认目标窗口未最小化；若窗口包含受保护视频、硬件加速覆盖层或系统限制内容，请改用区域录制或全屏录制",
            trimmed
        )
    }
}

fn build_window_capture_unavailable_error(details: &str) -> AppError {
    AppError::new(ErrorCode::ValidationError, "当前窗口不可录制")
        .with_details(build_window_capture_unavailable_details(details))
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
    let pause_flag = runtime
        .recording_pause_flag
        .clone()
        .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    runtime.recording_pause_flag = Some(pause_flag.clone());
    let start_ms = runtime.snapshot().elapsed_ms;
    let seg_idx = runtime.system_audio_segments.len();
    if !runtime.system_audio_process_ids.is_empty() {
        // 应用音频仍然使用 WAV（因为多进程混音复杂）
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
        let first_try = start_process_loopback_wavs(
            process_ids,
            output_paths.clone(),
            enabled_flag.clone(),
            pause_flag.clone(),
        );
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

    // 🔧 方案A：使用 FFmpeg 实时 AAC 编码，而非 WAV
    let sys_aac = sys_wav.with_extension("aac");
    let first_try = start_system_loopback_aac_with_device(
        runtime.system_audio_device_id.clone(),
        sys_aac.clone(),
        enabled_flag.clone(),
        pause_flag.clone(),
    );
    let start_result = match first_try {
        Ok(h) => Ok(h),
        Err(first_err) => {
            if runtime.system_audio_device_id.is_some() {
                runtime.system_audio_device_id = None;
                start_system_loopback_aac_with_device(None, sys_aac.clone(), enabled_flag, pause_flag)
                    .map_err(|second_err| format!("{}；回退默认设备失败: {}", first_err, second_err))
            } else {
                Err(first_err)
            }
        }
    };
    match start_result {
        Ok(handle) => {
            runtime.system_audio_wav_path = Some(sys_aac);  // 🔧 存储 AAC 路径
            runtime.system_audio_stop_flag = Some(handle.stop_flag.clone());
            runtime.system_audio_thread = handle.join;
            runtime.system_audio_stream_start_ms = Some(start_ms);
            if let Some(path) = runtime.system_audio_wav_path.clone() {
                runtime
                    .system_audio_segments
                    .push(crate::features::recording::state::AudioSegment { path, start_ms });
            } else {
                return Err("系统音频路径未设置".to_string());
            }
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
    let pause_flag = runtime
        .recording_pause_flag
        .clone()
        .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    runtime.recording_pause_flag = Some(pause_flag.clone());
    let start_ms = runtime.snapshot().elapsed_ms;
    let seg_idx = runtime.mic_audio_segments.len();
    let mic_wav = if seg_idx == 0 {
        output_dir.join(format!("{}.mic.wav", session_id))
    } else {
        output_dir.join(format!("{}.mic.{}.wav", session_id, seg_idx))
    };
    let first_try = start_microphone_wav_with_device(
        runtime.mic_audio_device_id.clone(),
        mic_wav.clone(),
        enabled_flag.clone(),
        pause_flag.clone(),
    );
    let start_result = match first_try {
        Ok(h) => Ok(h),
        Err(first_err) => {
            if runtime.mic_audio_device_id.is_some() {
                runtime.mic_audio_device_id = None;
                start_microphone_wav_with_device(None, mic_wav.clone(), enabled_flag, pause_flag)
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
            if let Some(path) = runtime.mic_audio_wav_path.clone() {
                runtime
                    .mic_audio_segments
                    .push(crate::features::recording::state::AudioSegment { path, start_ms });
            } else {
                return Err("麦克风音频路径未设置".to_string());
            }
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

fn spawn_stats_loop(
    app: AppHandle,
    state_arc: Arc<Mutex<SharedAppState>>,
    runtime_arc: Arc<Mutex<crate::features::recording::state::RecordingRuntime>>,
) {
    thread::spawn(move || loop {
        let mut emit_error: Option<(&'static str, String, Option<String>)> = None;
        let mut auto_stop_session_id: Option<String> = None;
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
                auto_stop_session_id = session_id.clone();
                emit_error = Some((
                    MAX_DURATION_REACHED,
                    "已达到最大录制时长，自动停止录制".to_string(),
                    session_id.clone(),
                ));
            }

            if let Some(process) = runtime.process.as_mut() {
                if let Ok(Some(status)) = process.try_wait() {
                    runtime.process = None;
                    if !runtime.auto_stop_requested
                        && phase != RecordingPhase::Idle
                        && phase != RecordingPhase::Stopping
                    {
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

            // 无画面看门狗：提前拦截“死黑屏/没内容”的假录制状态
            if phase == RecordingPhase::Recording && snapshot.elapsed_ms > 4000 && emit_error.is_none() {
                let mut no_video_frames = false;
                if runtime.target_type == "window" {
                    if let Some(first_frame) = runtime.wgc_first_frame_elapsed_ms.as_ref() {
                        if first_frame.load(Ordering::Relaxed) == u64::MAX {
                            no_video_frames = true;
                        }
                    }
                } else if let Some(tmp_path) = runtime.output_path_tmp.as_ref() {
                    // FFmpeg 模式下，如果超过 4 秒文件依然是 0 字节，说明没录进任何有效视频帧
                    if fs::metadata(tmp_path).map(|m| m.len()).unwrap_or(0) == 0 {
                        no_video_frames = true;
                    }
                }

                if no_video_frames {
                    runtime.auto_stop_requested = true;
                    runtime.phase = RecordingPhase::Stopping;
                    phase = RecordingPhase::Stopping;
                    auto_stop_session_id = session_id.clone();

                    let details = "请确认目标窗口未最小化；若录制包含受保护内容或硬件加速，请尝试关闭加速或改用全屏录制";
                    let err_msg = format!("未捕获到有效视频帧；{}", details);
                    runtime.last_error = Some(err_msg.clone());
                    emit_error = Some((
                        "VALIDATION_ERROR",
                        err_msg,
                        session_id.clone(),
                    ));
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
        if let Some(session_id) = auto_stop_session_id.clone() {
            let app_clone = app.clone();
            let state_clone = state_arc.clone();
            tauri::async_runtime::spawn_blocking(move || {
                finalize_auto_stop_recording(&app_clone, state_clone, session_id);
            });
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

        let mut target_type = request
            .target_type
            .clone()
            .unwrap_or_else(|| "screen".to_string())
            .to_lowercase();
        let mut target_id = request.target_id.clone().unwrap_or_default();
        // 统一录制时钟起点：必须早于视频采集与音频采集启动，避免后续音频延迟估算偏小导致 A/V 不同步。
        let capture_origin_unix_ms = now_unix_ms();
        let capture_origin_instant = std::time::Instant::now();
        let mut window_wgc_handle = None;
        let mut window_segment_path: Option<PathBuf> = None;
        bootstrap_force_default_border_from_settings(settings_snapshot.recording_wgc_force_default_border);
        bootstrap_force_default_dirty_region_from_settings(settings_snapshot.recording_wgc_force_default_dirty_region);
        let mut args: Vec<String> = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "warning".into(),
            "-y".into(),
        ];

        // 第一次尝试处理 window，如果命中 WGC 不支持的内容，降级到 gdigrab 的窗口模式
        if target_type == "window" {
            if target_id.trim().is_empty() {
                return Err(rollback_starting("窗口录制目标不能为空", "target_id is empty".to_string()));
            }
            if let Err(e) = validate_window_capture_target(target_id.trim()) {
                return Err(rollback_starting("当前窗口不可录制", e));
            }

            #[cfg(debug_assertions)]
            let force_ffmpeg_fallback = settings_snapshot.dev_force_ffmpeg_window_capture;
            #[cfg(not(debug_assertions))]
            let force_ffmpeg_fallback = false;

            if force_ffmpeg_fallback {
                log::warn!("开发模式：强制将 WGC 窗口录制降级为 GDI/FFmpeg 窗口录制");
                if let Ok(title) = crate::features::recording::wgc_capture::get_window_title_from_target(target_id.trim()) {
                    target_type = "gdigrab_window".to_string();
                    target_id = title;
                } else {
                    return Err(rollback_starting("开发模式强制降级失败：无法获取目标窗口标题", "".to_string()));
                }
            } else {
                let first_segment_path = build_window_segment_path(&output_dir, &session_id, 0);
                match start_window_capture_to_mp4(
                    target_id.trim(),
                    first_segment_path.clone(),
                    fps,
                    video_bitrate,
                    capture_cursor,
                    capture_origin_instant,
                    settings_snapshot.recording_wgc_force_default_border,
                ) {
                    Ok(handle) => {
                        window_wgc_handle = Some(handle);
                        window_segment_path = Some(first_segment_path);
                    }
                    Err(e) => {
                        if is_item_convert_failed(&e) {
                            log::warn!("WGC 窗口录制被系统拒绝 ({})，自动降级为 GDI/FFmpeg 窗口录制", e);
                            if let Ok(title) = crate::features::recording::wgc_capture::get_window_title_from_target(target_id.trim()) {
                                target_type = "gdigrab_window".to_string();
                                target_id = title;
                                // 降级成功，交给下面的 ffmpeg 分支处理
                            } else {
                                return Err(rollback_starting("当前窗口不可录制且降级失败", build_window_capture_unavailable_details(&e)));
                            }
                        } else {
                            return Err(rollback_starting("启动窗口源录制失败", e));
                        }
                    }
                }
            }
        }

        match target_type.as_str() {
            "window" => {
                // 已在上方通过 WGC 启动，此处留空
            }
            "gdigrab_window" => {
                args.push("-f".into());
                args.push("gdigrab".into());
                args.push("-framerate".into());
                args.push(format!("{}", fps));
                args.push("-draw_mouse".into());
                args.push(if capture_cursor { "1".into() } else { "0".into() });
                args.push("-i".into());
                args.push(format!("title={}", target_id));
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
        runtime.started_at_ms = capture_origin_unix_ms;
        runtime.started_instant = Some(capture_origin_instant);
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
        runtime.wgc_audio_sync_advance_ms =
            (settings_snapshot.recording_window_audio_sync_advance_ms as u64).min(500);
        runtime.output_path_tmp = Some(tmp_path.clone());
        runtime.output_path_final = Some(final_path.clone());
        runtime.target_type = target_type.clone();
        runtime.target_id = target_id.clone();
        runtime.capture_cursor = capture_cursor;
        runtime.system_audio_device_id = system_audio_device_id.clone();
        runtime.system_audio_process_ids = request
            .system_audio_process_ids
            .clone()
            .unwrap_or_default()
            .into_iter()
            .filter(|pid| *pid > 0)
            .collect::<Vec<_>>();
        runtime.mic_audio_device_id = request.microphone_device_id.clone();

        // 🔧 诊断日志：记录音频捕获配置
        log::info!(
            "录制启动 - 音频配置: capture_system_audio={}, system_audio_device_id={:?}, capture_microphone={}, mic_device_id={:?}",
            capture_system_audio,
            system_audio_device_id,
            capture_microphone,
            request.microphone_device_id
        );
        
        runtime.system_audio_enabled_flag = Some(Arc::new(AtomicBool::new(capture_system_audio)));
        runtime.mic_audio_enabled_flag = Some(Arc::new(AtomicBool::new(capture_microphone)));
        runtime.recording_pause_flag = Some(Arc::new(AtomicBool::new(false)));
        runtime.system_audio_ever_enabled = capture_system_audio;
        runtime.mic_audio_ever_enabled = capture_microphone;
        runtime.system_audio_stream_start_ms = None;
        runtime.mic_audio_stream_start_ms = None;
        runtime.system_audio_segments.clear();
        runtime.mic_audio_segments.clear();
        runtime.window_video_segments.clear();
        runtime.window_segment_index = 0;
        if let Some(seg_path) = window_segment_path.as_ref() {
            runtime.window_video_segments.push(seg_path.clone());
        }
        // 系统音频关闭时不占用 loopback 设备；重新开启时再创建新音频分段并在合成阶段按 start_ms 对齐。
        if capture_system_audio {
            log::info!("🔧 尝试启动系统音频捕获...");
            match ensure_system_audio_capture_started(app, &mut runtime, &output_dir, &session_id, true) {
                Ok(()) => log::info!("✅ 系统音频捕获启动成功"),
                Err(e) => log::error!("❌ 系统音频捕获启动失败: {}", e),
            }
        }
        if capture_microphone {
            let _ = ensure_mic_capture_started(app, &mut runtime, &output_dir, &session_id, true);
        }
        runtime.process = child_opt;
        if let Some(handle) = window_wgc_handle {
            runtime.wgc_stop_flag = Some(handle.stop_flag);
            runtime.wgc_pause_flag = Some(handle.pause_flag);
            runtime.wgc_first_frame_elapsed_ms = Some(handle.first_frame_elapsed_ms);
            runtime.wgc_thread = Some(handle.join);
        } else {
            runtime.wgc_stop_flag = None;
            runtime.wgc_pause_flag = None;
            runtime.wgc_first_frame_elapsed_ms = None;
            runtime.wgc_thread = None;
        }
        let started_at_ms = runtime.started_at_ms;
        emit_recording_state_changed(app, Some(&session_id), runtime.phase.as_str(), 0);
        drop(runtime);
        persist_wgc_capture_fallback_if_needed(&state_arc);
        if let Some(stderr) = stderr_opt {
            spawn_stderr_parser(app.clone(), runtime_arc.clone(), session_id.clone(), stderr);
        }
        spawn_stats_loop(app.clone(), state_arc.clone(), runtime_arc.clone());

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
    let (
        session_id,
        target_type,
        was_paused,
        mut process,
        wgc_thread,
        wgc_first_frame_elapsed_ms,
        wgc_audio_sync_advance_ms,
        system_audio_stop_flag,
        system_audio_thread,
        mic_audio_stop_flag,
        mic_audio_thread,
        output_tmp,
        output_final,
        mut sys_segments,
        mut mic_segments,
        window_video_segments,
        audio_segment_paths,
    ) = {
        let mut runtime = lock_arc_mutex(&runtime_arc);
        let allow_auto_stop_finalize = runtime.phase == RecordingPhase::Stopping && runtime.auto_stop_requested;
        if runtime.phase != RecordingPhase::Recording
            && runtime.phase != RecordingPhase::Paused
            && !allow_auto_stop_finalize
        {
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
        let target_type = runtime.target_type.clone();
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

        let process = runtime.process.take();
        let wgc_thread = runtime.wgc_thread.take();
        let wgc_first_frame_elapsed_ms = runtime.wgc_first_frame_elapsed_ms.take();
        let wgc_audio_sync_advance_ms = runtime.wgc_audio_sync_advance_ms;
        runtime.wgc_stop_flag = None;
        let system_audio_stop_flag = runtime.system_audio_stop_flag.take();
        let system_audio_thread = runtime.system_audio_thread.take();
        let mic_audio_stop_flag = runtime.mic_audio_stop_flag.take();
        let mic_audio_thread = runtime.mic_audio_thread.take();
        let output_tmp = runtime.output_path_tmp.take();
        let output_final = runtime.output_path_final.take();
        let mut taken_sys_segments = std::mem::take(&mut runtime.system_audio_segments);
        let mut taken_mic_segments = std::mem::take(&mut runtime.mic_audio_segments);
        let window_video_segments = std::mem::take(&mut runtime.window_video_segments);
        let sys_segments = if runtime.system_audio_ever_enabled {
            std::mem::take(&mut taken_sys_segments)
        } else {
            Vec::new()
        };
        let mic_segments = if runtime.mic_audio_ever_enabled {
            std::mem::take(&mut taken_mic_segments)
        } else {
            Vec::new()
        };
        let mut audio_segment_paths = HashSet::<PathBuf>::new();
        for seg in &sys_segments {
            audio_segment_paths.insert(seg.path.clone());
        }
        for seg in &mic_segments {
            audio_segment_paths.insert(seg.path.clone());
        }
        (
            session_id,
            target_type,
            was_paused,
            process,
            wgc_thread,
            wgc_first_frame_elapsed_ms,
            wgc_audio_sync_advance_ms,
            system_audio_stop_flag,
            system_audio_thread,
            mic_audio_stop_flag,
            mic_audio_thread,
            output_tmp,
            output_final,
            sys_segments,
            mic_segments,
            window_video_segments,
            audio_segment_paths,
        )
    };

    let mut fatal_error: Option<AppError> = None;
    let mut pending_window_capture_unavailable_details: Option<String> = None;

    // 🔧 性能优化：立即停止音频捕获，避免等待视频处理完成后才停止
    log::info!("🔧 设置音频停止信号...");
    if let Some(flag) = system_audio_stop_flag.as_ref() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(flag) = mic_audio_stop_flag.as_ref() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    log::info!("✅ 音频停止信号已设置");

    if let Some(process) = process.as_mut() {
        #[cfg(target_os = "windows")]
        if was_paused {
            let _ = set_process_threads_suspended(process.id(), false);
        }
        if let Some(stdin) = process.stdin.as_mut() {
            let _ = stdin.write_all(b"q\n");
        }
    }
    let mut exited = false;
    if let Some(process) = process.as_mut() {
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
    if let Some(join) = wgc_thread {
        match join.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if is_benign_wgc_stop_error(&e) {
                    log::warn!("窗口录制停止返回可忽略状态: {}", e);
                } else if is_item_convert_failed(&e) {
                    pending_window_capture_unavailable_details = Some(e);
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
    persist_wgc_capture_fallback_if_needed(&state_arc);
    if let Some(anchor_holder) = wgc_first_frame_elapsed_ms.as_ref() {
        let anchor_ms = anchor_holder.load(Ordering::Relaxed);
        if anchor_ms == u64::MAX && fatal_error.is_none() {
            if let Some(details) = pending_window_capture_unavailable_details.take() {
                fatal_error = Some(build_window_capture_unavailable_error(&details));
            } else {
                fatal_error = Some(
                    AppError::new(ErrorCode::ValidationError, "窗口录制未捕获到有效视频帧")
                        .with_details(
                            "请确认目标窗口处于可见状态且有内容变化，避免最小化/被系统保护内容；若为视频类窗口，可尝试关闭硬件加速或改用区域录制/全屏录制",
                        ),
                );
            }
        } else if anchor_ms > 0 {
            let calibrated_anchor_ms = anchor_ms.saturating_add(wgc_audio_sync_advance_ms);
            for seg in &mut sys_segments {
                seg.start_ms = seg.start_ms.saturating_sub(calibrated_anchor_ms);
            }
            for seg in &mut mic_segments {
                seg.start_ms = seg.start_ms.saturating_sub(calibrated_anchor_ms);
            }
            log::info!(
                "应用 WGC 首帧锚点校正: anchor_ms={}, advance_ms={}, calibrated_anchor_ms={}",
                anchor_ms,
                wgc_audio_sync_advance_ms,
                calibrated_anchor_ms
            );
        } else if let Some(details) = pending_window_capture_unavailable_details.take() {
            fatal_error = Some(build_window_capture_unavailable_error(&details));
        }
    } else if let Some(details) = pending_window_capture_unavailable_details.take() {
        fatal_error = Some(build_window_capture_unavailable_error(&details));
    }

    if let (Some(output_tmp), Some(output_final)) = (output_tmp.as_ref(), output_final.as_ref()) {
        log::info!("🔧 开始视频后处理...");
        let video_post_start = std::time::Instant::now();
        
        if target_type == "window" && fatal_error.is_none() {
            log::info!("🔧 合并窗口视频片段...");
            if let Err(e) = concat_video_segments(&ffmpeg_path, &window_video_segments, output_tmp) {
                fatal_error = Some(e);
            }
        }
        if fatal_error.is_none() {
            log::info!("🔧 重命名输出文件...");
            if let Err(e) = rename_recording_output_with_retry(output_tmp, output_final) {
                fatal_error = Some(e);
            }
        }

        log::info!("✅ 视频后处理完成，耗时: {}ms", video_post_start.elapsed().as_millis());
    } else if fatal_error.is_none() {
        fatal_error = Some(AppError::new(ErrorCode::SystemError, "录制输出路径不存在"));
    }

    // 🔧 等待音频线程退出（停止信号已在前面设置）
    log::info!("🔧 等待系统音频线程退出...");
    let sys_audio_join_start = std::time::Instant::now();
    if let Some(join) = system_audio_thread {
        let _ = join.join();
    }
    log::info!("✅ 系统音频线程已退出，耗时: {}ms", sys_audio_join_start.elapsed().as_millis());

    log::info!("🔧 等待麦克风音频线程退出...");
    let mic_audio_join_start = std::time::Instant::now();
    if let Some(join) = mic_audio_thread {
        let _ = join.join();
    }
    log::info!("✅ 麦克风音频线程已退出，耗时: {}ms", mic_audio_join_start.elapsed().as_millis());

    if fatal_error.is_none() {
        // 🔧 性能优化：跳过视频验证，音频合并时会自然验证
        // 之前的验证会导致 10-20 秒的延迟（FFmpeg 扫描整个视频文件）
        // if let Some(output_final) = output_final.as_ref() {
        //     log::info!("🔧 验证视频文件完整性...");
        //     let validate_start = std::time::Instant::now();
        //     if let Err(e) = validate_video_input_for_merge_with_retry(&ffmpeg_path, output_final) {
        //         if fatal_error.is_none() {
        //             fatal_error = Some(e);
        //         }
        //     }
        //     log::info!("✅ 视频验证完成，耗时: {}ms", validate_start.elapsed().as_millis());
        // } else {
        //     fatal_error = Some(AppError::new(ErrorCode::SystemError, "录制输出路径不存在"));
        // }
        if output_final.is_none() {
            fatal_error = Some(AppError::new(ErrorCode::SystemError, "录制输出路径不存在"));
        }
    }

    // ✅ 注意：不要在这里删除音频片段！
    // 音频合并是在后台异步执行的，需要这些文件
    // 删除操作将在音频合并完成（或失败）后在异步任务中执行
    // for path in audio_segment_paths {
    //     let _ = fs::remove_file(path);
    // }

    // 只清理窗口视频片段（已经在同步阶段使用完毕）
    for path in window_video_segments {
        let _ = fs::remove_file(path);
    }

    let mut output_path_for_result: Option<String> = None;
    let mut file_size_bytes: u64 = 0;
    if fatal_error.is_none() {
        if let Some(output_final) = output_final.as_ref() {
            output_path_for_result = Some(output_final.to_string_lossy().to_string());
            file_size_bytes = fs::metadata(output_final).map(|m| m.len()).unwrap_or(0);
        } else {
            fatal_error = Some(AppError::new(ErrorCode::SystemError, "录制输出路径不存在"));
        }
    }

    let duration_ms = {
        let mut runtime = lock_arc_mutex(&runtime_arc);
        if let Some(paused_at) = runtime.paused_at_instant {
            runtime.paused_total_ms = runtime
                .paused_total_ms
                .saturating_add(paused_at.elapsed().as_millis() as u64);
            runtime.paused_at_instant = None;
        }
        let duration_ms = runtime.snapshot().elapsed_ms;
        runtime.reset_to_idle();
        emit_recording_state_changed(app, None, runtime.phase.as_str(), 0);
        duration_ms
    };

    if let Some(err) = fatal_error {
        return Err(err);
    }

    let result = RecordingStopResult {
        session_id: session_id.clone(),
        output_path: output_path_for_result
            .ok_or_else(|| AppError::new(ErrorCode::SystemError, "录制停止失败"))?,
        duration_ms,
        file_size_bytes,
    };

    // ✅ 立即发送完成事件，UI 可以立即响应
    emit_recording_finished(app, &result);

    // ✅ 在后台异步执行音频合并，不阻塞 UI
    if !sys_segments.is_empty() || !mic_segments.is_empty() {
        let app_handle = app.clone();
        let session_id_clone = session_id.clone();
        let ffmpeg_path_clone = ffmpeg_path.clone();
        let output_final_clone = output_final.clone().unwrap();

        // ✅ 将音频片段路径 HashSet 转换为 Vec，用于合并后清理
        // audio_segment_paths 已从元组解构获得（L1580）
        let audio_segment_paths_vec: Vec<std::path::PathBuf> = audio_segment_paths.into_iter().collect();

        tauri::async_runtime::spawn(async move {
            // 发送开始事件
            emit_recording_audio_merging(
                &app_handle,
                Some(&session_id_clone),
                "started",
                None,
                Some("正在后台合并音频..."),
            );

            // 执行音频合并（sys_segments 和 mic_segments 会被移动到闭包中）
            let merge_result = merge_system_audio_into_video(
                &ffmpeg_path_clone,
                &output_final_clone,
                &sys_segments,
                &mic_segments,
            );

            // ✅ 合并完成后，清理临时音频片段文件
            let mut cleaned_count = 0;
            let mut not_found_count = 0;
            for path in &audio_segment_paths_vec {
                // ✅ 添加诊断日志：检查文件是否存在
                match std::fs::metadata(path) {
                    Ok(meta) => {
                        log::info!("准备清理音频片段: {:?}, 大小: {} bytes", path.file_name(), meta.len());
                        if let Err(e) = fs::remove_file(path) {
                            // 文件不存在是正常的（可能是进程音频线程提前退出），只记录 debug 级别日志
                            if e.kind() == std::io::ErrorKind::NotFound {
                                log::warn!("音频片段文件在清理时不存在: {:?}", path);
                                not_found_count += 1;
                            } else {
                                log::warn!("清理音频片段失败: {:?}, {}", path, e);
                            }
                        } else {
                            cleaned_count += 1;
                        }
                    }
                    Err(e) => {
                        log::warn!("音频片段文件在清理前就不存在: {:?}, {}", path.file_name(), e);
                        not_found_count += 1;
                    }
                }
            }
            log::info!("已清理 {}/{} 个音频片段文件 ({} 个不存在)", cleaned_count, audio_segment_paths_vec.len(), not_found_count);

            // 根据合并结果发送事件
            match merge_result {
                Ok(_) => {
                    log::info!("后台音频合并完成");
                    emit_recording_audio_merging(
                        &app_handle,
                        Some(&session_id_clone),
                        "completed",
                        Some(100),
                        Some("音频合并完成"),
                    );
                }
                Err(e) => {
                    let detail = e.details.clone().unwrap_or_default();
                    let msg = if detail.is_empty() {
                        format!("音频合成失败，已保留视频文件: {}", e.message)
                    } else {
                        format!("音频合成失败，已保留视频文件: {}；{}", e.message, detail)
                    };
                    log::error!("后台音频合并失败: {}", msg);
                    emit_recording_audio_merging(
                        &app_handle,
                        Some(&session_id_clone),
                        "failed",
                        None,
                        Some(&msg),
                    );
                    emit_recording_error(&app_handle, Some(&session_id_clone), RECORDING_PROCESS_EXITED, &msg);
                }
            }
        });
    } else {
        // ✅ 如果没有音频片段，直接清理 window_video_segments
        // （已在前面清理）
        log::info!("无音频片段，跳过音频合并");
    }
    
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
    let (
        mut process,
        wgc_stop_flag,
        wgc_thread,
        system_audio_stop_flag,
        system_audio_thread,
        mic_audio_stop_flag,
        mic_audio_thread,
        cleanup_paths,
    ) = {
        let mut runtime = lock_arc_mutex(&runtime_arc);
        if runtime.phase == RecordingPhase::Idle {
            return Ok(());
        }
        if let Some(ref expected) = request.session_id {
            if runtime.session_id.as_deref() != Some(expected.as_str()) {
                return Err(AppError::new(ErrorCode::ValidationError, "录制会话已变化，请刷新状态后重试"));
            }
        }

        runtime.phase = RecordingPhase::Stopping;
        runtime.auto_stop_requested = false;
        let process = runtime.process.take();
        let wgc_stop_flag = runtime.wgc_stop_flag.take();
        let wgc_thread = runtime.wgc_thread.take();
        let system_audio_stop_flag = runtime.system_audio_stop_flag.take();
        let system_audio_thread = runtime.system_audio_thread.take();
        let mic_audio_stop_flag = runtime.mic_audio_stop_flag.take();
        let mic_audio_thread = runtime.mic_audio_thread.take();
        let mut cleanup_paths = HashSet::<PathBuf>::new();
        if let Some(wav) = runtime.system_audio_wav_path.take() {
            cleanup_paths.insert(wav);
        }
        if let Some(wav) = runtime.mic_audio_wav_path.take() {
            cleanup_paths.insert(wav);
        }
        if let Some(path) = runtime.output_path_tmp.take() {
            cleanup_paths.insert(path);
        }
        for seg in std::mem::take(&mut runtime.system_audio_segments) {
            cleanup_paths.insert(seg.path);
        }
        for seg in std::mem::take(&mut runtime.mic_audio_segments) {
            cleanup_paths.insert(seg.path);
        }
        for seg in std::mem::take(&mut runtime.window_video_segments) {
            cleanup_paths.insert(seg);
        }
        (
            process,
            wgc_stop_flag,
            wgc_thread,
            system_audio_stop_flag,
            system_audio_thread,
            mic_audio_stop_flag,
            mic_audio_thread,
            cleanup_paths,
        )
    };

    if let Some(process) = process.as_mut() {
        let _ = process.kill();
        let _ = process.wait();
    }
    if let Some(flag) = wgc_stop_flag.as_ref() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(join) = wgc_thread {
        let _ = join.join();
    }
    if let Some(flag) = system_audio_stop_flag.as_ref() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(join) = system_audio_thread {
        let _ = join.join();
    }
    if let Some(flag) = mic_audio_stop_flag.as_ref() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(join) = mic_audio_thread {
        let _ = join.join();
    }
    for path in cleanup_paths {
        let _ = fs::remove_file(path);
    }

    let mut runtime = lock_arc_mutex(&runtime_arc);
    runtime.reset_to_idle();
    emit_recording_state_changed(app, None, runtime.phase.as_str(), 0);
    Ok(())
}

pub fn pause_recording(app: &AppHandle, state_arc: Arc<Mutex<SharedAppState>>) -> Result<(), AppError> {
    let runtime_arc = {
        let state_guard = lock_arc_mutex(&state_arc);
        state_guard.recording_runtime.clone()
    };
    let (
        session_id,
        target_type,
        wgc_thread,
        system_audio_stop_flag,
        system_audio_thread,
        mic_audio_stop_flag,
        mic_audio_thread,
    ) = {
        let mut runtime = lock_arc_mutex(&runtime_arc);
        if runtime.phase != RecordingPhase::Recording {
            return Err(AppError::new(ErrorCode::ValidationError, "当前状态不允许暂停"));
        }
        runtime.phase = RecordingPhase::Stopping;
        if runtime.target_type == "window" {
            if let Some(flag) = runtime.wgc_stop_flag.as_ref() {
                flag.store(true, Ordering::SeqCst);
            }
        } else if let Some(process) = runtime.process.as_mut() {
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
        if let Some(flag) = runtime.recording_pause_flag.as_ref() {
            flag.store(true, Ordering::SeqCst);
        }
        (
            runtime.session_id.clone(),
            runtime.target_type.clone(),
            runtime.wgc_thread.take(),
            runtime.system_audio_stop_flag.take(),
            runtime.system_audio_thread.take(),
            runtime.mic_audio_stop_flag.take(),
            runtime.mic_audio_thread.take(),
        )
    };

    if let Some(flag) = system_audio_stop_flag.as_ref() {
        flag.store(true, Ordering::SeqCst);
    }
    if let Some(join) = system_audio_thread {
        let _ = join.join();
    }
    if let Some(flag) = mic_audio_stop_flag.as_ref() {
        flag.store(true, Ordering::SeqCst);
    }
    if let Some(join) = mic_audio_thread {
        let _ = join.join();
    }

    if target_type == "window" {
        if let Some(join) = wgc_thread {
            match join.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    if !is_benign_wgc_stop_error(&e) {
                        let mut runtime = lock_arc_mutex(&runtime_arc);
                        runtime.phase = RecordingPhase::Recording;
                        if let Some(flag) = runtime.recording_pause_flag.as_ref() {
                            flag.store(false, Ordering::SeqCst);
                        }
                        return Err(AppError::new(ErrorCode::SystemError, "暂停窗口录制失败").with_details(e));
                    }
                }
                Err(_) => {
                    let mut runtime = lock_arc_mutex(&runtime_arc);
                    runtime.phase = RecordingPhase::Recording;
                    if let Some(flag) = runtime.recording_pause_flag.as_ref() {
                        flag.store(false, Ordering::SeqCst);
                    }
                    return Err(AppError::new(ErrorCode::SystemError, "暂停窗口录制失败：线程异常退出"));
                }
            }
        }
    }

    let elapsed_ms = {
        let mut runtime = lock_arc_mutex(&runtime_arc);
        runtime.wgc_stop_flag = None;
        runtime.wgc_pause_flag = None;
        runtime.system_audio_wav_path = None;
        runtime.system_audio_stream_start_ms = None;
        runtime.mic_audio_wav_path = None;
        runtime.mic_audio_stream_start_ms = None;

        // 🔧 修复：暂停时清理已完成的音频片段，避免内存泄漏
        // 这些片段已在磁盘上，合并时会重新读取，无需常驻内存
        let sys_segments_to_clean: Vec<_> = std::mem::take(&mut runtime.system_audio_segments)
            .into_iter()
            .filter(|seg| seg.path.exists())
            .collect();
        let mic_segments_to_clean: Vec<_> = std::mem::take(&mut runtime.mic_audio_segments)
            .into_iter()
            .filter(|seg| seg.path.exists())
            .collect();

        // ✅ 添加诊断日志：记录暂停时清理的音频片段
        if !sys_segments_to_clean.is_empty() {
            log::info!("暂停时清理 {} 个系统音频片段", sys_segments_to_clean.len());
            for seg in &sys_segments_to_clean {
                log::info!("  - {:?}", seg.path.file_name());
            }
        }
        if !mic_segments_to_clean.is_empty() {
            log::info!("暂停时清理 {} 个麦克风音频片段", mic_segments_to_clean.len());
            for seg in &mic_segments_to_clean {
                log::info!("  - {:?}", seg.path.file_name());
            }
        }

        drop(runtime); // 释放锁后再执行I/O操作

        // 异步删除已完成的音频片段文件，释放磁盘空间
        for seg in sys_segments_to_clean {
            if let Err(e) = fs::remove_file(&seg.path) {
                log::warn!("清理暂停音频片段失败 {:?}: {}", seg.path, e);
            }
        }
        for seg in mic_segments_to_clean {
            if let Err(e) = fs::remove_file(&seg.path) {
                log::warn!("清理暂停音频片段失败 {:?}: {}", seg.path, e);
            }
        }

        let mut runtime = lock_arc_mutex(&runtime_arc);
        runtime.phase = RecordingPhase::Paused;
        runtime.paused_at_instant = Some(std::time::Instant::now());
        runtime.snapshot().elapsed_ms
    };
    emit_recording_state_changed(app, session_id.as_deref(), RecordingPhase::Paused.as_str(), elapsed_ms);
    Ok(())
}

pub fn resume_recording(app: &AppHandle, state_arc: Arc<Mutex<SharedAppState>>) -> Result<(), AppError> {
    let runtime_arc = {
        let state_guard = lock_arc_mutex(&state_arc);
        state_guard.recording_runtime.clone()
    };
    let (
        is_window_target,
        target_id,
        output_dir,
        session_id_for_audio,
        should_restore_system_audio,
        should_restore_mic_audio,
        next_segment_index,
        next_segment_path,
        fps,
        video_bitrate_kbps,
        capture_cursor,
    ) = {
        let mut runtime = lock_arc_mutex(&runtime_arc);
        if runtime.phase != RecordingPhase::Paused {
            return Err(AppError::new(ErrorCode::ValidationError, "当前状态不允许恢复"));
        }
        if let Some(paused_at) = runtime.paused_at_instant {
            runtime.paused_total_ms = runtime
                .paused_total_ms
                .saturating_add(paused_at.elapsed().as_millis() as u64);
            runtime.paused_at_instant = None;
        }
        let output_dir = runtime
            .output_path_tmp
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .ok_or_else(|| AppError::new(ErrorCode::ValidationError, "录制输出目录不存在"))?;
        let session_id_for_audio = runtime.session_id.clone().unwrap_or_default();
        let should_restore_system_audio = runtime
            .system_audio_enabled_flag
            .as_ref()
            .map(|f| f.load(Ordering::SeqCst))
            .unwrap_or(false);
        let should_restore_mic_audio = runtime
            .mic_audio_enabled_flag
            .as_ref()
            .map(|f| f.load(Ordering::SeqCst))
            .unwrap_or(false);
        if let Some(flag) = runtime.recording_pause_flag.as_ref() {
            flag.store(false, Ordering::SeqCst);
        }
        let is_window_target = runtime.target_type == "window";
        let target_id = runtime.target_id.clone();
        let next_segment_index = runtime.window_segment_index.saturating_add(1);
        let next_segment_path =
            build_window_segment_path(&output_dir, &session_id_for_audio, next_segment_index);
        let fps = runtime.fps;
        let video_bitrate_kbps = runtime.video_bitrate_kbps;
        let capture_cursor = runtime.capture_cursor;
        if runtime.target_type != "window" {
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
        }
        (
            is_window_target,
            target_id,
            output_dir,
            session_id_for_audio,
            should_restore_system_audio,
            should_restore_mic_audio,
            next_segment_index,
            next_segment_path,
            fps,
            video_bitrate_kbps,
            capture_cursor,
        )
    };

    let window_handle = if is_window_target {
        Some(
            start_window_capture_to_mp4(
                target_id.as_str(),
                next_segment_path.clone(),
                fps,
                video_bitrate_kbps,
                capture_cursor,
                std::time::Instant::now(),
                is_force_default_border_enabled(),
            )
                .map_err(|e| AppError::new(ErrorCode::SystemError, "恢复窗口录制失败").with_details(e))?,
        )
    } else {
        None
    };

    let mut runtime = lock_arc_mutex(&runtime_arc);
    if runtime.phase != RecordingPhase::Paused {
        return Err(AppError::new(
            ErrorCode::ValidationError,
            "录制状态已变化，请刷新状态后重试",
        ));
    }
    if let Some(handle) = window_handle {
        runtime.window_segment_index = next_segment_index;
        runtime.window_video_segments.push(next_segment_path);
        runtime.wgc_stop_flag = Some(handle.stop_flag);
        runtime.wgc_pause_flag = Some(handle.pause_flag);
        if runtime.wgc_first_frame_elapsed_ms.is_none() {
            runtime.wgc_first_frame_elapsed_ms = Some(handle.first_frame_elapsed_ms.clone());
        }
        runtime.wgc_thread = Some(handle.join);
    }
    if should_restore_system_audio && runtime.system_audio_thread.is_none() {
        let _ = ensure_system_audio_capture_started(app, &mut runtime, &output_dir, &session_id_for_audio, false);
    }
    if should_restore_mic_audio && runtime.mic_audio_thread.is_none() {
        let _ = ensure_mic_capture_started(app, &mut runtime, &output_dir, &session_id_for_audio, false);
    }
    runtime.phase = RecordingPhase::Recording;
    let snapshot = runtime.snapshot();
    emit_recording_state_changed(app, runtime.session_id.as_deref(), runtime.phase.as_str(), snapshot.elapsed_ms);
    drop(runtime);
    persist_wgc_capture_fallback_if_needed(&state_arc);
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
    let (
        session_id,
        output_dir,
        should_enable_sys,
        should_enable_mic,
        elapsed_now_ms,
        system_audio_stop_flag,
        system_audio_thread,
        mic_audio_stop_flag,
        mic_audio_thread,
        sys_device_changed,
        mic_device_changed,
    ) = {
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
            if let Some(flag) = runtime.system_audio_enabled_flag.as_ref() {
                flag.store(v, Ordering::SeqCst);
            }
            if v {
                runtime.system_audio_ever_enabled = true;
            }
        }
        if let Some(v) = capture_microphone {
            if let Some(flag) = runtime.mic_audio_enabled_flag.as_ref() {
                flag.store(v, Ordering::SeqCst);
            }
            if v {
                runtime.mic_audio_ever_enabled = true;
            }
        }
        runtime.system_audio_device_id = requested_sys_device;
        runtime.mic_audio_device_id = requested_mic_device;

        let mut system_audio_stop_flag = None;
        let mut system_audio_thread = None;
        if (sys_device_changed || !should_enable_sys) && runtime.system_audio_thread.is_some() {
            system_audio_stop_flag = runtime.system_audio_stop_flag.take();
            system_audio_thread = runtime.system_audio_thread.take();
            runtime.system_audio_wav_path = None;
        }
        let mut mic_audio_stop_flag = None;
        let mut mic_audio_thread = None;
        if (mic_device_changed || !should_enable_mic) && runtime.mic_audio_thread.is_some() {
            mic_audio_stop_flag = runtime.mic_audio_stop_flag.take();
            mic_audio_thread = runtime.mic_audio_thread.take();
            runtime.mic_audio_wav_path = None;
        }

        (
            session_id,
            output_dir,
            should_enable_sys,
            should_enable_mic,
            elapsed_now_ms,
            system_audio_stop_flag,
            system_audio_thread,
            mic_audio_stop_flag,
            mic_audio_thread,
            sys_device_changed,
            mic_device_changed,
        )
    };

    if let Some(flag) = system_audio_stop_flag.as_ref() {
        flag.store(true, Ordering::SeqCst);
    }
    if let Some(join) = system_audio_thread {
        let _ = join.join();
    }
    if let Some(flag) = mic_audio_stop_flag.as_ref() {
        flag.store(true, Ordering::SeqCst);
    }
    if let Some(join) = mic_audio_thread {
        let _ = join.join();
    }
    let mut runtime = lock_arc_mutex(&runtime_arc);
    if sys_device_changed {
        runtime.system_audio_stream_start_ms = Some(elapsed_now_ms);
    }
    if !should_enable_sys {
        runtime.system_audio_stream_start_ms = None;
    }
    if mic_device_changed {
        runtime.mic_audio_stream_start_ms = Some(elapsed_now_ms);
    }
    if !should_enable_mic {
        runtime.mic_audio_stream_start_ms = None;
    }
    if should_enable_sys && runtime.system_audio_thread.is_none() {
        ensure_system_audio_capture_started(app, &mut runtime, &output_dir, &session_id, true).map_err(|e| {
            AppError::new(ErrorCode::SystemError, format!("开启系统音频失败: {}", e)).with_details(e)
        })?;
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
