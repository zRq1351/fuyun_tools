use crate::core::app_state::AppState as SharedAppState;
use crate::core::error::{AppError, ErrorCode};
use crate::core::error_codes::AppErrorKind;
use crate::core::perf_metrics::record_perf_metric;
use crate::features::recording::audio_device::list_microphones;
use crate::features::recording::error_codes::{
    AUDIO_DEVICE_LOST, AUDIO_DEVICE_NOT_FOUND, MAX_DURATION_REACHED, RECORDING_PROCESS_EXITED,
    RECORDING_START_FAILED,
};
use crate::features::recording::events::{
    emit_recording_audio_merging, emit_recording_device_list, emit_recording_effective_audio_device,
    emit_recording_error, emit_recording_finished, emit_recording_state_changed,
    emit_recording_stats_updated,
};
use crate::features::recording::ffmpeg_runner::{build_output_paths, resolve_ffmpeg_path};
use crate::features::recording::native_wasapi::{
    list_audio_processes, start_microphone_wav_with_device, start_process_loopback_wavs,
    start_system_loopback_aac_with_device,
};
use crate::features::recording::state::RecordingPhase;
use crate::features::recording::types::{
    AudioInputDevice, AudioProcessItem, RecordingMonitorItem, RecordingRegressionReport,
    RecordingRuntimeState, RecordingSessionInfo, RecordingStopResult, SessionRequest,
    StartRecordingRequest,
};
use crate::features::recording::wgc_capture::{
    bootstrap_force_default_border_from_settings,
    bootstrap_force_default_dirty_region_from_settings, enumerate_monitor_infos,
    enumerate_monitors_with_rects, is_force_default_border_enabled,
    is_force_default_dirty_region_enabled, is_item_convert_failed, monitor_count,
    pick_monitor_and_local_rect, start_monitor_capture_to_mp4, start_window_capture_to_mp4,
    validate_window_capture_target,
};
use crate::sync::{lock_arc_mutex, Mutex};
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
use std::time::{Duration, Instant, SystemTime};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

// ====================================================================
//  录制常量
// ====================================================================

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
static LAST_OPEN_FOLDER_MS: AtomicU64 = AtomicU64::new(0);
const VIDEO_IO_RETRY_DELAYS_MS: [u64; 5] = [60, 120, 240, 480, 800];

/// 片头黑帧探测窗口（秒）：只解码开头一小段，开销可忽略
const BLACK_LEAD_DETECT_WINDOW_S: f64 = 2.0;
/// 探测到的片头黑段短于该值时不裁剪（避免为几十毫秒的抖动动文件）
const MIN_BLACK_LEAD_TRIM_MS: u64 = 80;
/// 回归自测的音/视频轨时长差容忍值：暂停边界若存在累积失步，两轨时长会发散，
/// 该断言可在端到端层面拦截此类回归（此前缺失，正是音画失步溜过验证的原因之一）
const AV_DURATION_TOLERANCE_MS: i64 = 120;

fn suppress_console_window(command: &mut Command) -> &mut Command {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// 等待线程退出（每 10ms 轮询，最多 max_iters 次）。
fn join_thread_with_timeout<T>(join: std::thread::JoinHandle<T>, name: &str, max_iters: u32) -> bool {
    for _ in 0..max_iters {
        if join.is_finished() {
            let _ = join.join();
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    log::warn!(
        "{} 线程超时 ({:.1}s)，放弃等待（线程转为后台运行）",
        name,
        max_iters as f64 * 0.01
    );
    false
}

/// 原子替换 dst 为 src：先把 dst 改名备份，成功后再删除备份。
/// Windows 下 rename 不覆盖目标，直接"先删后 rename"在 rename 失败时会丢失原文件（#2/#3）。
fn replace_file_atomically(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if !dst.exists() {
        return fs::rename(src, dst);
    }
    let mut bak_os = dst.as_os_str().to_os_string();
    bak_os.push(".bak");
    let bak = PathBuf::from(bak_os);
    // Windows rename 不覆盖目标：先清理可能的上次残留，避免二次替换失败（N4）
    let _ = fs::remove_file(&bak);
    fs::rename(dst, &bak)?;
    match fs::rename(src, dst) {
        Ok(()) => {
            let _ = fs::remove_file(&bak);
            Ok(())
        }
        Err(e) => {
            // 恢复原文件，避免成片丢失
            let _ = fs::rename(&bak, dst);
            Err(e)
        }
    }
}

// ====================================================================
//  运行时状态管理
// ====================================================================

fn normalize_runtime_state(runtime: &mut crate::features::recording::state::RecordingRuntime) {
    if let Some(process) = runtime.process.as_mut() {
        if let Ok(Some(status)) = process.try_wait() {
            log::debug!("FFmpeg 进程已退出: {:?}", status);
            let _ = process.wait(); // 确保 OS 资源回收
            runtime.process = None;
        }
    }
    let wgc_running = runtime
        .wgc_thread
        .as_ref()
        .map(|t| !t.is_finished())
        .unwrap_or(false);
    let audio_running = !runtime.system_audio_threads.is_empty()
        || runtime.mic_audio_thread.is_some();
    if runtime.process.is_none() && !wgc_running {
        if audio_running {
            // 视频链路已结束但音频线程仍在运行（如 ffmpeg 崩溃进入 Error）：
            // 主动停止并回收音频线程，否则会永久泄漏且下一次录制无法重新启动音频
            if let Some(flag) = runtime.system_audio_stop_flag.take() {
                flag.store(true, Ordering::SeqCst);
            }
            let sys_threads = std::mem::take(&mut runtime.system_audio_threads);
            for join in sys_threads {
                let _ = join_thread_with_timeout(join, "normalize 系统音频", 500);
            }
            if let Some(flag) = runtime.mic_audio_stop_flag.take() {
                flag.store(true, Ordering::SeqCst);
            }
            if let Some(join) = runtime.mic_audio_thread.take() {
                let _ = join_thread_with_timeout(join, "normalize 麦克风音频", 500);
            }
            log::warn!("normalize_runtime_state: 已停止残留音频线程");
        }
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

fn finalize_auto_stop_recording(
    app: &AppHandle,
    state_arc: Arc<Mutex<SharedAppState>>,
    session_id: String,
) {
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
            // 录制已被其他路径收尾（手动停止/另一路自动停止）：此时无残留可清理，不打扰用户
            let already_finalizing = {
                let state_guard = lock_arc_mutex(&state_arc);
                let rt = lock_arc_mutex(&state_guard.recording_runtime);
                matches!(rt.phase, RecordingPhase::Idle | RecordingPhase::Stopping)
            };
            if already_finalizing {
                log::warn!("自动停止收尾时录制已被其他流程处理，跳过兜底清理: {}", stop_msg);
                return;
            }
            match cancel_recording(app, state_arc, request) {
                Ok(()) => {
                    log::warn!(
                        "自动停止收尾失败，已执行 cancel_recording 兜底清理: {}",
                        stop_msg
                    );
                    emit_recording_error(
                        app,
                        Some(session_id.as_str()),
                        RECORDING_PROCESS_EXITED,
                        &stop_msg,
                    );
                }
                Err(cancel_err) => {
                    let merged = format!("{}；自动兜底清理失败: {}", stop_msg, cancel_err);
                    log::warn!("自动停止收尾与兜底清理均失败: {}", merged);
                    emit_recording_error(
                        app,
                        Some(session_id.as_str()),
                        RECORDING_PROCESS_EXITED,
                        &merged,
                    );
                }
            }
        }
    }
}

// check_system_audio_capability removed in native WASAPI mode

fn now_unix_ms() -> i64 {
    crate::utils::utils_helpers::now_unix_ms_i64()
}

/// 根据采集线程回传的流启动时刻（unix ms）换算录制时间轴上的分段起点。
/// 流实际出声发生在 ACK 之前，若在 ACK 之后才读 elapsed 会把分段整体放晚，
/// 导致该段音频相对视频滞后（音画不同步）。流启动时间越精确，分段边界越准。
fn derive_audio_segment_start_ms(
    runtime: &crate::features::recording::state::RecordingRuntime,
    stream_start_unix_ms: Option<u64>,
) -> u64 {
    let elapsed_now_ms = runtime.snapshot().elapsed_ms;
    match stream_start_unix_ms {
        Some(start_unix) => {
            let now_ms_i64 = now_unix_ms();
            let age_ms = if now_ms_i64 > 0 && (now_ms_i64 as u64) >= start_unix {
                (now_ms_i64 as u64) - start_unix
            } else {
                0
            };
            elapsed_now_ms.saturating_sub(age_ms)
        }
        None => elapsed_now_ms,
    }
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

/// WGC 托管目标（窗口/单屏/区域）统一判定：这些模式的视频由 WGC 线程产出，
/// 暂停/恢复/停止走 WGC 语义而非 ffmpeg 进程语义。
fn is_wgc_target(target_type: &str) -> bool {
    matches!(target_type, "window" | "wgc_screen" | "wgc_region")
}

/// 解析显示器目标编码：
/// - "wgc_screen" + "mon=0" → (0, None)
/// - "wgc_region" + "mon=1,crop=100,200,800,600" → (1, Some((100,200,800,600)))，局部非负坐标
fn parse_wgc_monitor_target(
    target_type: &str,
    target_id: &str,
) -> Option<(usize, Option<(u32, u32, u32, u32)>)> {
    if target_type != "wgc_screen" && target_type != "wgc_region" {
        return None;
    }
    let (mon_part, rest) = match target_id.trim().split_once(',') {
        Some((m, r)) => (m, Some(r)),
        None => (target_id.trim(), None),
    };
    let index = mon_part.strip_prefix("mon=")?.trim().parse::<usize>().ok()?;
    match (target_type, rest) {
        ("wgc_screen", None) => Some((index, None)),
        ("wgc_region", Some(rest)) => {
            let crop = rest.trim().strip_prefix("crop=")?;
            let (x, y, w, h) = parse_region_target(crop)?;
            if x < 0 || y < 0 {
                return None;
            }
            Some((index, Some((x as u32, y as u32, w, h))))
        }
        _ => None,
    }
}

/// 解析 screen 模式显式指定的显示器目标："mon=N" / "monitor=N"（大小写不敏感）；未指定返回 None
fn parse_screen_explicit_monitor(target_id: &str) -> Option<usize> {
    let raw = target_id.trim().to_lowercase();
    let value = raw
        .strip_prefix("mon=")
        .or_else(|| raw.strip_prefix("monitor="))?;
    value.trim().parse::<usize>().ok()
}

#[cfg(target_os = "windows")]
fn resolve_wgc_monitor_start_params(
    target_type: &str,
    target_id: &str,
) -> Option<(usize, Option<(u32, u32, u32, u32)>)> {
    match target_type {
        "region" => {
            let rect = parse_region_target(target_id)?;
            let normalized = normalize_region_to_virtual_screen(rect.0, rect.1, rect.2, rect.3)?;
            pick_monitor_and_local_rect(normalized, &enumerate_monitors_with_rects())
                .map(|(idx, lx, ly, w, h)| (idx, Some((lx, ly, w, h))))
        }
        _ => parse_wgc_monitor_target(target_type, target_id),
    }
}

#[cfg(not(target_os = "windows"))]
fn resolve_wgc_monitor_start_params(
    _target_type: &str,
    _target_id: &str,
) -> Option<(usize, Option<(u32, u32, u32, u32)>)> {
    None
}

#[cfg(target_os = "windows")]
fn normalize_region_to_virtual_screen(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Option<(i32, i32, u32, u32)> {
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
    Some((
        clamped_x as i32,
        clamped_y as i32,
        clamped_w as u32,
        clamped_h as u32,
    ))
}

#[cfg(not(target_os = "windows"))]
fn normalize_region_to_virtual_screen(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Option<(i32, i32, u32, u32)> {
    Some((x, y, width.max(1), height.max(1)))
}

/// 停止采集时为活动音频分段标记终点（仅标记未设终点的最后一个分段）。
/// 每个分段只标记一次（第一个 end_ms 为 None 的段），合并阶段据此裁剪尾部，
/// 避免 2s 静音填充 + 采集队列残留与后续分段重叠（声音叠加）或超出视频时长。
fn mark_active_segment_end(
    segments: &mut [crate::features::recording::state::AudioSegment],
    end_ms: u64,
) {
    if let Some(seg) = segments.iter_mut().rev().find(|s| s.end_ms.is_none()) {
        seg.end_ms = Some(end_ms);
    }
}

/// 将音频分段整体前移 offset_ms（单锚点全局校正，与历史行为一致）。
fn shift_audio_segments_global(
    segments: &mut [crate::features::recording::state::AudioSegment],
    offset_ms: u64,
) {
    for seg in segments.iter_mut() {
        if seg.start_ms < offset_ms {
            seg.trim_start_ms = seg.trim_start_ms.saturating_add(offset_ms - seg.start_ms);
            seg.start_ms = 0;
        } else {
            seg.start_ms -= offset_ms;
            seg.trim_start_ms = 0;
        }
        if let Some(end_ms) = seg.end_ms.as_mut() {
            *end_ms = end_ms.saturating_sub(offset_ms);
        }
    }
}

/// 计算窗口录制各视频分段的 U→拼接时间轴 偏移表 δ_k：
/// 位于第 k 个分段 U 区间 [S_k, E_k) 的音频事件，其拼接后位置 = U − δ_k。
/// 分段时长优先使用容器实测值（measured_durations_ms）——模型推算值
/// (E_k−S_k)−A_k 不含停止信号轮询/编码器收尾的尾部延迟，每个暂停边界
/// 会累积 0~150ms 偏差，多次暂停后表现为渐进音画失步。
/// 推导：第 k 段内容的拼接起点 B_k = Σ_{j<k} D_k，故 V(u) = B_k + (u − S_k − A_k)。
/// last_calibrated_anchor 为停止时实时读取的末段锚点（含安全裕量），
/// total_u_ms 为录制结束时 U 时钟值（末段右边界 E_N）。
fn compute_window_segment_shifts(
    segments_meta: &[crate::features::recording::state::WindowVideoSegment],
    measured_durations_ms: &[Option<u64>],
    last_calibrated_anchor: u64,
    total_u_ms: u64,
) -> Vec<u64> {
    let mut shifts = Vec::with_capacity(segments_meta.len());
    let mut base_ms = 0u64;
    for (k, seg) in segments_meta.iter().enumerate() {
        let s_k = seg.u_start_ms;
        let e_k = segments_meta
            .get(k + 1)
            .map(|n| n.u_start_ms)
            .unwrap_or(total_u_ms)
            .max(s_k);
        let span = e_k.saturating_sub(s_k);
        let raw_anchor = seg
            .first_frame_anchor
            .as_ref()
            .map(|a| a.load(Ordering::Relaxed))
            .unwrap_or(0);
        // 末段以停止时实时读到的值为准；u64::MAX 表示该段从未出帧，按 0 处理避免负时长
        let a_k = if k + 1 == segments_meta.len() && raw_anchor != u64::MAX {
            last_calibrated_anchor
        } else if raw_anchor == u64::MAX {
            0
        } else {
            raw_anchor
        };
        // 时长来源：容器实测（含尾部收尾的真实长度）优先；探测失败回退模型推算。
        // 实测值做合理性钳制：不可能比 U 跨度长出太多（防止损坏头导致后续偏移爆炸）。
        let d_k = match measured_durations_ms.get(k).copied().flatten() {
            Some(d) => d.min(span.saturating_add(500)),
            None => span.saturating_sub(a_k),
        };
        shifts.push(s_k.saturating_add(a_k).saturating_sub(base_ms));
        base_ms = base_ms.saturating_add(d_k);
    }
    shifts
}

/// 用 ffmpeg 读取视频容器的总时长（毫秒）。
/// 不带输出参数调用会让 ffmpeg 在打印输入信息后立即退出——只解析头部，毫秒级开销；
/// 解析失败返回 None（调用方回退模型推算）。
fn probe_video_duration_ms(ffmpeg_path: &std::path::Path, video_path: &PathBuf) -> Option<u64> {
    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);
    cmd.arg("-hide_banner")
        .arg("-nostats")
        .arg("-i")
        .arg(video_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let output = cmd.output().ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_duration_from_ffmpeg_stderr(&stderr)
}

/// 从 ffmpeg 的输入信息输出中解析 "Duration: HH:MM:SS.frac" 为毫秒
fn parse_duration_from_ffmpeg_stderr(stderr: &str) -> Option<u64> {
    let idx = stderr.find("Duration:")?;
    let rest = &stderr[idx + "Duration:".len()..];
    // 格式形如 " 00:00:01.96,"
    let time_part = rest.split(',').next()?.trim();
    let mut parts = time_part.split(':');
    let hours: u64 = parts.next()?.trim().parse().ok()?;
    let minutes: u64 = parts.next()?.trim().parse().ok()?;
    let seconds_raw = parts.next()?.trim();
    let mut sec_parts = seconds_raw.split('.');
    let seconds: u64 = sec_parts.next()?.parse().ok()?;
    let frac = sec_parts.next().unwrap_or("");
    let frac_digits: String = frac.chars().take_while(|c| c.is_ascii_digit()).collect();
    // 右补齐到毫秒："9"→900ms、"96"→960ms
    let padded = format!("{:0<3}", frac_digits);
    let frac_ms: u64 = padded.get(..3).unwrap_or("0").parse().unwrap_or(0);
    Some((hours * 3600 + minutes * 60 + seconds).saturating_mul(1000).saturating_add(frac_ms))
}

/// 找到 U 位置所属的视频分段序号（最后一个 u_start <= pos 的段；早于首段归 0）
fn find_window_cycle_index(
    segments_meta: &[crate::features::recording::state::WindowVideoSegment],
    u_pos: u64,
) -> usize {
    let mut idx = 0usize;
    for (k, seg) in segments_meta.iter().enumerate() {
        if seg.u_start_ms <= u_pos {
            idx = k;
        } else {
            break;
        }
    }
    idx
}

/// 按音频分段所属视频分段的偏移量校准其时间轴（多段窗口录制）
fn apply_window_cycle_shifts(
    segments: &mut [crate::features::recording::state::AudioSegment],
    segments_meta: &[crate::features::recording::state::WindowVideoSegment],
    shifts: &[u64],
) {
    if segments_meta.is_empty() || shifts.len() != segments_meta.len() {
        return;
    }
    for seg in segments.iter_mut() {
        let idx = find_window_cycle_index(segments_meta, seg.start_ms);
        let delta = shifts[idx];
        if seg.start_ms < delta {
            seg.trim_start_ms = seg.trim_start_ms.saturating_add(delta - seg.start_ms);
            seg.start_ms = 0;
        } else {
            seg.start_ms -= delta;
            seg.trim_start_ms = 0;
        }
        if let Some(end_ms) = seg.end_ms.as_mut() {
            *end_ms = end_ms.saturating_sub(delta);
        }
    }
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

fn build_exit_error_with_stderr(
    status_text: String,
    runtime: &crate::features::recording::state::RecordingRuntime,
) -> String {
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

// fn set_process_threads_suspended removed since it is no longer used and caused video corruption.

fn resolve_output_dir(
    state: &SharedAppState,
    request_output_dir: Option<String>,
) -> Result<PathBuf, AppError> {
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
        .map_err(|e| AppError::new(ErrorCode::IoError, AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e))))?;
    let _ = base.pop();
    Ok(base.join("recordings"))
}

fn build_window_segment_path(
    output_dir: &PathBuf,
    session_id: &str,
    segment_index: usize,
) -> PathBuf {
    output_dir.join(format!("{}.video.{}.tmp.mp4", session_id, segment_index))
}

// ====================================================================
//  视频片段处理（拼接/裁剪/重命名）
// ====================================================================

/// 生成 ffmpeg concat 清单条目：单引号包裹 + 反斜杠转正斜杠。
/// concat demuxer 会把未引用路径中的反斜杠当作转义符吞掉
/// （Windows 下 `D:\a\b` 被读成 `D:ab` 导致拼接失败），必须引用并转换；
/// 引用的单引号字符串内的字面单引号按 ffmpeg 规则写成 '\''。
fn build_concat_entry(path: &std::path::Path) -> String {
    let forward = path.to_string_lossy().replace('\\', "/");
    format!("file '{}'\n", forward.replace('\'', "'\\''"))
}

/// 定位与 ffmpeg 同目录的 ffprobe（流级时长探测的快路径）；不存在返回 None。
/// 注意：精简版发行包可能只带 ffmpeg，此时走 ffmpeg 解码兜底。
fn resolve_ffprobe_path(ffmpeg_path: &std::path::Path) -> Option<PathBuf> {
    let probe = ffmpeg_path.parent()?.join("ffprobe.exe");
    probe.is_file().then_some(probe)
}

/// "HH:MM:SS.frac" → 毫秒
fn parse_hms_to_ms(s: &str) -> Option<u64> {
    let mut parts = s.trim().split(':');
    let hours: u64 = parts.next()?.trim().parse().ok()?;
    let minutes: u64 = parts.next()?.trim().parse().ok()?;
    let seconds_raw = parts.next()?.trim();
    let mut sec_parts = seconds_raw.split('.');
    let seconds: u64 = sec_parts.next()?.parse().ok()?;
    let frac_digits: String = sec_parts
        .next()
        .unwrap_or("")
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let padded = format!("{:0<3}", frac_digits);
    let frac_ms: u64 = padded.get(..3).unwrap_or("0").parse().unwrap_or(0);
    Some((hours * 3600 + minutes * 60 + seconds).saturating_mul(1000).saturating_add(frac_ms))
}

/// 探测指定流的时长（毫秒）。spec 形如 "v:0" / "a:0"。
/// 快路径用 ffprobe；缺失时用 ffmpeg 把该流解码到 null 并取最后一次进度行的 time=
/// （音频解码极快，视频为 regression 短样本可接受）。
fn probe_stream_duration_ms(
    ffmpeg_path: &std::path::Path,
    media: &PathBuf,
    spec: &str,
) -> Option<u64> {
    if let Some(ffprobe_path) = resolve_ffprobe_path(ffmpeg_path) {
        if let Ok(output) = Command::new(&ffprobe_path)
            .args(["-v", "error", "-select_streams", spec, "-show_entries", "stream=duration", "-of", "csv=p=0"])
            .arg(media)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let first = stdout.lines().next()?.trim();
            if !first.is_empty() && first != "N/A" {
                if let Ok(seconds) = first.parse::<f64>() {
                    return Some((seconds * 1000.0).round() as u64);
                }
            }
        }
    }
    // ffmpeg 解码兜底
    let map = if spec.starts_with('v') { "0:v:0" } else { "0:a:0" };
    let output = Command::new(ffmpeg_path)
        .arg("-hide_banner")
        .arg("-nostats")
        .arg("-i")
        .arg(media)
        .args(["-map", map, "-f", "null", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut last_ms: Option<u64> = None;
    for line in stderr.lines() {
        if let Some(pos) = line.find("time=") {
            let value = line[pos + "time=".len()..].split_whitespace().next().unwrap_or("");
            if let Some(ms) = parse_hms_to_ms(value) {
                last_ms = Some(ms);
            }
        }
    }
    last_ms
}

fn concat_video_segments(
    ffmpeg_path: &std::path::Path,
    segments: &[PathBuf],
    output_path: &PathBuf,
) -> Result<(), AppError> {
    if segments.is_empty() {
        return Err(AppError::new(
            ErrorCode::ValidationError,
            "没有可拼接的视频分段",
        ));
    }
    if segments.len() == 1 {
        return rename_recording_output_with_retry(&segments[0], output_path);
    }
    let list_path = output_path.with_extension("concat.txt");
    let mut list_file = fs::File::create(&list_path).map_err(|e| {
        AppError::new(ErrorCode::IoError, "创建视频拼接列表失败").with_details(e.to_string())
    })?;
    for seg in segments {
        let line = build_concat_entry(seg);
        if let Err(e) = list_file.write_all(line.as_bytes()) {
            drop(list_file);
            let _ = fs::remove_file(&list_path);
            return Err(AppError::new(ErrorCode::IoError, "写入视频拼接列表失败").with_details(e.to_string()));
        }
    }
    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);
    let output_result = cmd
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
        .output();
    let _ = fs::remove_file(&list_path);
    let output = output_result
        .map_err(|e| {
            AppError::new(ErrorCode::SystemError, "执行视频拼接失败").with_details(e.to_string())
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::new(ErrorCode::SystemError, "视频拼接失败").with_details(stderr));
    }
    Ok(())
}

/// 从 blackdetect 输出解析片头黑段时长：
/// 仅当第一条黑段从 0 附近开始（≤50ms 容差）时返回其毫秒长度；否则 None。
fn parse_black_lead_ms_from_blackdetect(stderr: &str) -> Option<u64> {
    for line in stderr.lines() {
        let Some(pos) = line.find("black_start:") else {
            continue;
        };
        let rest = &line[pos..];
        let start = parse_f64_after(rest, "black_start:")?;
        if start > 0.05 {
            // 第一条黑段不从片头开始 → 无片头黑帧
            return None;
        }
        let duration = parse_f64_after(rest, "black_duration:").unwrap_or(0.0);
        return if duration > 0.0 {
            Some((duration * 1000.0).round() as u64)
        } else {
            None
        };
    }
    None
}

/// 探测视频片头是否存在黑帧段及其时长（仅解码开头窗口，开销可忽略）。
/// 返回 Some(ms) 表示应裁剪的片头黑帧毫秒数；None 表示无需裁剪或探测失败（保守不裁）。
fn detect_black_lead_ms(
    ffmpeg_path: &std::path::Path,
    video_path: &PathBuf,
) -> Option<u64> {
    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);
    cmd.arg("-hide_banner")
        .arg("-nostats")
        .arg("-t")
        .arg(format!("{:.3}", BLACK_LEAD_DETECT_WINDOW_S))
        .arg("-i")
        .arg(video_path)
        .args([
            "-vf",
            "blackdetect=d=0.04:pic_th=0.98:pix_th=0.12",
            "-an",
            "-f",
            "null",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let output = cmd.output().ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_black_lead_ms_from_blackdetect(&stderr)
}

/// 裁剪视频开头的灰色帧（gdigrab 初始化时可能产生）
/// 使用流复制模式，无重编码，性能开销极小
fn trim_video_initial_frames(
    ffmpeg_path: &std::path::Path,
    video_path: &PathBuf,
    trim_seconds: f64,
) -> Result<(), AppError> {
    let trimmed_path = video_path.with_extension("trimmed.tmp.mp4");
    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);
    let output = cmd
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y")
        .arg("-ss")
        .arg(format!("{:.3}", trim_seconds))
        .arg("-i")
        .arg(video_path)
        .arg("-c")
        .arg("copy")
        .arg("-movflags")
        .arg("+faststart")
        .arg(&trimmed_path)
        .output()
        .map_err(|e| {
            AppError::new(ErrorCode::SystemError, "执行视频裁剪失败").with_details(e.to_string())
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let _ = fs::remove_file(&trimmed_path);
        return Err(AppError::new(ErrorCode::SystemError, "视频裁剪失败").with_details(stderr));
    }
    // 短视频（时长 < trim_seconds）时 ffmpeg 可能产出 0 字节文件且 exit 0：保留原视频，跳过裁剪
    let trimmed_size = fs::metadata(&trimmed_path).map(|m| m.len()).unwrap_or(0);
    if trimmed_size == 0 {
        let _ = fs::remove_file(&trimmed_path);
        log::warn!("视频过短（裁剪结果为 0 字节），跳过灰帧裁剪");
        return Ok(());
    }
    replace_file_atomically(&trimmed_path, video_path).map_err(|e| {
        AppError::new(ErrorCode::IoError, "重命名裁剪文件失败").with_details(e.to_string())
    })?;
    log::info!("✅ 已裁剪视频开头 {:.3}s 灰色帧", trim_seconds);
    Ok(())
}

// ====================================================================
//  音频合并与混音
// ====================================================================

/// 纯音频多片段合并：将多个音频片段（含 adelay）合并为单个 AAC，不涉及视频
/// 用于替代 filter_complex 慢速路径中的视频参与步骤
fn make_silent_aac(
    ffmpeg_path: &std::path::Path,
    output_path: &std::path::Path,
    duration_ms: u64,
) -> Result<bool, String> {
    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);
    cmd.arg("-hide_banner").arg("-loglevel").arg("warning").arg("-y")
        .arg("-f").arg("lavfi")
        .arg("-i").arg("anullsrc=r=48000:cl=stereo")
        .arg("-t").arg(format!("{:.3}", duration_ms as f64 / 1000.0))
        .arg("-c:a").arg("aac")
        .arg(output_path);
    let output = cmd
        .output()
        .map_err(|e| format!("启动静音占位音频生成失败: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        log::warn!("静音占位音频生成失败: {}", stderr);
        return Ok(false);
    }
    Ok(true)
}

fn merge_audio_segments_only(
    ffmpeg_path: &std::path::Path,
    segments: &[crate::features::recording::state::AudioSegment],
    output_path: &std::path::Path,
    audio_bitrate_kbps: u32,
) -> Result<(), String> {
    // 过滤掉因剪辑点/校正后有效时长为 0 的分段（如整段落在视频起点之前）
    let segments = segments
        .iter()
        .filter(|s| s.end_ms.map(|e| e.saturating_sub(s.start_ms) > 0).unwrap_or(true))
        .collect::<Vec<_>>();
    if segments.is_empty() {
        // 输入非空但全部被裁为 0 时长（极短开关音频/整段落在起点前）：
        // 生成静音占位 AAC，避免下游流复制合并步骤因缺少对齐文件而失败
        let dummy = make_silent_aac(ffmpeg_path, output_path, 200)?;
        if !dummy {
            return Err("没有可合并的音频片段".to_string());
        }
        return Ok(());
    }
    // 单片段无延迟且未打终点：直接复制（AAC）或重编码（WAV）
    if segments.len() == 1
        && segments[0].start_ms == 0
        && segments[0].trim_start_ms == 0
        && segments[0].end_ms.is_none()
    {
        let seg = &segments[0];
        let is_aac = seg.path.extension()
            .map(|ext| ext.to_string_lossy().to_lowercase() == "aac")
            .unwrap_or(false);
        if is_aac {
            return std::fs::copy(&seg.path, output_path)
                .map(|_| ())
                .map_err(|e| format!("复制音频片段失败: {}", e));
        }
    }

    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);
    cmd.arg("-hide_banner").arg("-loglevel").arg("warning").arg("-y");

    let mut labels: Vec<String> = Vec::new();
    let mut filter_parts: Vec<String> = Vec::new();
    for (idx, seg) in segments.iter().enumerate() {
        if seg.trim_start_ms > 0 {
            cmd.arg("-ss").arg(format!("{}.{:03}", seg.trim_start_ms / 1000, seg.trim_start_ms % 1000));
        }
        cmd.arg("-i").arg(&seg.path);

        let label = format!("a{}", idx);
        // 已打终点的分段：先归零时间戳再用 atrim 裁剪到终点，
        // 去除 2s 静音填充与采集队列残留，避免与后续分段重叠（声音叠加）
        let mut chain = format!("[{i}:a]asetpts=PTS-STARTPTS", i = idx);
        if let Some(end_ms) = seg.end_ms {
            let keep_s = end_ms.saturating_sub(seg.start_ms) as f64 / 1000.0;
            chain.push_str(&format!(",atrim=end={:.6}", keep_s));
        }
        chain.push_str(&format!(",asetpts=PTS-STARTPTS,adelay={d}|{d}[{l}]", d = seg.start_ms, l = label));
        filter_parts.push(chain);
        labels.push(format!("[{}]", label));
    }

    if labels.len() == 1 {
        filter_parts.push(format!("{}anull[aout]", labels[0]));
    } else {
        filter_parts.push(format!(
            "{}amix=inputs={}:duration=longest:normalize=0[aout]",
            labels.join(""),
            labels.len()
        ));
    }

    cmd.arg("-filter_complex").arg(filter_parts.join(";"))
        .arg("-map").arg("[aout]")
        .arg("-c:a").arg("aac")
        .arg("-b:a").arg(format!("{}k", audio_bitrate_kbps.max(32)))
        .arg(output_path);

    let output = cmd.output()
        .map_err(|e| format!("启动纯音频合并失败: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("纯音频合并失败: {}", stderr));
    }
    Ok(())
}

/// 使用 FFmpeg 验证音频文件是否包含可解码的音频数据
/// 返回 true 表示文件有效，false 表示文件损坏或为空
fn validate_audio_file_with_ffmpeg(
    ffmpeg_path: &std::path::Path,
    audio_path: &std::path::Path,
) -> bool {
    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);
    match cmd
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(audio_path)
        .arg("-f")
        .arg("null")
        .arg("-")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => match child.wait_with_output() {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    log::warn!(
                        "音频文件验证失败: {:?}, exit={}, stderr={}",
                        audio_path.file_name().unwrap_or_default(),
                        output.status,
                        stderr
                    );
                    return false;
                }
                true
            }
            Err(e) => {
                log::warn!("音频文件验证执行失败: {:?}, {}", audio_path, e);
                false
            }
        },
        Err(e) => {
            log::warn!("启动音频文件验证失败: {}", e);
            false
        }
    }
}

fn merge_system_audio_into_video(
    ffmpeg_path: &std::path::Path,
    video_path: &PathBuf,
    system_segments: &[crate::features::recording::state::AudioSegment],
    mic_segments: &[crate::features::recording::state::AudioSegment],
    audio_bitrate_kbps: u32,
) -> Result<(), AppError> {
    let started_at = Instant::now();
    let expected_system_count = system_segments.len();
    let expected_mic_count = mic_segments.len();

        if system_segments.len() == 1 && mic_segments.is_empty() {
            let seg = &system_segments[0];
            if seg.start_ms < 100 && seg.trim_start_ms == 0 && seg.end_ms.is_none() && seg.path.exists() {
                let is_aac = seg.path.extension()
                    .map(|ext| ext.to_string_lossy().to_lowercase() == "aac")
                    .unwrap_or(false);
                // AAC 文件由 FFmpeg pipe 产生，100% 有效，跳过完整解码验证（仅做文件大小检查）
                if is_aac || validate_audio_file_with_ffmpeg(ffmpeg_path, &seg.path) {
                    log::info!(
                        "快速路径：单个系统音频片段(start_ms={}, trim_start_ms={})，使用流复制模式",
                        seg.start_ms,
                        seg.trim_start_ms
                    );
                    return merge_audio_fast(ffmpeg_path, video_path, &seg.path, false, audio_bitrate_kbps);
                } else {
                    log::warn!("快速路径：系统音频文件验证失败，跳过音频合并");
                    return Ok(());
                }
            }
        }
        if mic_segments.len() == 1 && system_segments.is_empty() {
            let seg = &mic_segments[0];
            if seg.start_ms < 100 && seg.trim_start_ms == 0 && seg.end_ms.is_none() && seg.path.exists() {
                // 麦克风 WAV 文件仍需 FFmpeg 验证（cpal 写入可能因 I/O 中断损坏）
                if validate_audio_file_with_ffmpeg(ffmpeg_path, &seg.path) {
                    log::info!(
                        "快速路径：单个麦克风音频片段(start_ms={}, trim_start_ms={})，使用流复制模式",
                        seg.start_ms,
                        seg.trim_start_ms
                    );
                    return merge_audio_fast(ffmpeg_path, video_path, &seg.path, false, audio_bitrate_kbps);
                } else {
                    log::warn!("快速路径：麦克风音频文件验证失败，跳过音频合并");
                    return Ok(());
                }
            }
        }

    let is_valid_audio_segment = |seg: &crate::features::recording::state::AudioSegment| {
        if !seg.path.exists() {
            log::warn!("音频片段不存在: {:?}", seg.path);
            return false;
        }

        let is_aac = seg
            .path
            .extension()
            .map(|ext| ext.to_string_lossy().to_lowercase() == "aac")
            .unwrap_or(false);
        // AAC 文件最低检查阈值：1024 字节（约 60-80ms 的 128kbps AAC 数据）
        // 7 字节阈值过低，会将损坏/空的 AAC 文件误判为有效
        let min_size = if is_aac { 1024 } else { 44 };
        match fs::metadata(&seg.path) {
            Ok(meta) => {
                let size = meta.len();
                let valid = meta.is_file() && size > min_size;
                if !valid {
                    log::warn!(
                        "音频片段无效: {:?}, 大小: {} bytes (需要 > {})",
                        seg.path,
                        size,
                        min_size
                    );
                    return false;
                }
            }
            Err(e) => {
                log::warn!("无法读取音频片段元数据: {:?}, 错误: {}", seg.path, e);
                return false;
            }
        }
        // AAC 文件由 FFmpeg pipe 产生，文件大小检查已足够，不再做完整解码验证
        true
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
    // P4 优化：多片段快速路径 — 当所有系统音频片段延迟极低且无裁剪时，
    // 先用 concat 拼接为单个文件，再走快速路径，避免 amix 重编码
    // ⚠️ 已移除：多进程并行音频段 start_ms 相同，concat 首尾拼接会错误地把并行流串接，
    //    正确语义是 amix 混音（走下方两步法）；pause/resume 段 start_ms 递增、永远不满足
    //    all_low_latency，故该快路径仅会命中错误的并行场景。

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
    // 🔧 双音源（sys+mic）：分别对齐合并后 amix 混音，再单次写入视频
    // 两步法对双音源不适用：第二次 merge_audio_fast 的 -map 会替换第一次写入的音频轨，导致系统音频丢失
    let output_dir = video_path.parent()
        .ok_or_else(|| AppError::new(ErrorCode::SystemError, "无法获取输出目录"))?;
    if has_system && has_mic {
        log::info!(
            "🔧 双音源合并: sys={} 段, mic={} 段, 先分别对齐再 amix 混音",
            valid_system.len(),
            valid_mic.len()
        );
        let sys_aligned = output_dir.join("sys_aligned.tmp.aac");
        let mic_aligned = output_dir.join("mic_aligned.tmp.aac");
        let mixed = output_dir.join("mixed.tmp.aac");
        if let Err(e) =
            merge_audio_segments_only(ffmpeg_path, &valid_system, &sys_aligned, audio_bitrate_kbps)
        {
            let _ = fs::remove_file(&sys_aligned);
            let _ = fs::remove_file(&mic_aligned);
            return Err(
                AppError::new(ErrorCode::SystemError, "系统音频预合并失败").with_details(e)
            );
        }
        if let Err(e) =
            merge_audio_segments_only(ffmpeg_path, &valid_mic, &mic_aligned, audio_bitrate_kbps)
        {
            let _ = fs::remove_file(&sys_aligned);
            let _ = fs::remove_file(&mic_aligned);
            return Err(
                AppError::new(ErrorCode::SystemError, "麦克风音频预合并失败").with_details(e)
            );
        }
        let mix_result =
            mix_audio_files(ffmpeg_path, &sys_aligned, &mic_aligned, &mixed, audio_bitrate_kbps);
        let _ = fs::remove_file(&sys_aligned);
        let _ = fs::remove_file(&mic_aligned);
        mix_result?;
        let result = merge_audio_fast(ffmpeg_path, video_path, &mixed, false, audio_bitrate_kbps);
        let _ = fs::remove_file(&mixed);
        return result;
    }
    // 🔧 两步合并：先纯音频合并（快速），再流复制合并视频（快速）
    // 替代原先的 filter_complex 全路径（视频参与滤镜 → 重编码 → 慢）

    let sys_aligned = output_dir.join("sys_aligned.tmp.aac");
    if has_system {
        let seg_count = valid_system.len();
        log::info!("🔧 两步合并 Step 1: 预合并 {} 个系统音频片段", seg_count);
        merge_audio_segments_only(ffmpeg_path, &valid_system, &sys_aligned, audio_bitrate_kbps)
            .map_err(|e| AppError::new(ErrorCode::SystemError, "系统音频预合并失败").with_details(e))?;
        log::info!("🔧 两步合并 Step 2: 系统音频流复制合并到视频");
        merge_audio_fast(ffmpeg_path, video_path, &sys_aligned, false, audio_bitrate_kbps)?;
        let _ = fs::remove_file(&sys_aligned);
    }

    let mic_aligned = output_dir.join("mic_aligned.tmp.aac");
    if has_mic {
        let seg_count = valid_mic.len();
        log::info!("🔧 两步合并 Step 1: 预合并 {} 个麦克风音频片段", seg_count);
        merge_audio_segments_only(ffmpeg_path, &valid_mic, &mic_aligned, audio_bitrate_kbps)
            .map_err(|e| AppError::new(ErrorCode::SystemError, "麦克风音频预合并失败").with_details(e))?;
        log::info!("🔧 两步合并 Step 2: 麦克风音频流复制合并到视频");
        merge_audio_fast(ffmpeg_path, video_path, &mic_aligned, false, audio_bitrate_kbps)?;
        let _ = fs::remove_file(&mic_aligned);
    }

    let elapsed_ms = started_at.elapsed().as_millis();
    log::info!("✅ 音频合并完成（两步法），耗时: {}ms ({:.1}s)", elapsed_ms, elapsed_ms as f64 / 1000.0);
    if elapsed_ms > 5000 {
        log::warn!("⚠️ 音频合并耗时较长({}ms)", elapsed_ms);
    }
    record_perf_metric(
        "recording.audio_merge",
        "录屏音频合成耗时（两步法）",
        elapsed_ms as u64,
        true,
        None,
    );

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
    audio_bitrate_kbps: u32,
) -> Result<(), AppError> {
    let started_at = Instant::now();
    let merged_path = video_path.with_extension("merged.tmp.mp4");

    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);

    let is_aac = audio_path
        .extension()
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
        .arg("-map").arg("0:v:0")
        .arg("-map").arg("1:a:0")
        .arg("-c:v")
        .arg("copy")
        .arg("-c:a")
        .arg(if is_aac { "copy" } else { "aac" });
    // 流复制模式下 -b:a 无实际作用，仅重编码时传递码率
    if !is_aac {
        cmd.arg("-b:a")
            .arg(format!("{}k", audio_bitrate_kbps.max(32)));
    }
    cmd.arg("-movflags")
        .arg("+faststart")
        .arg(&merged_path);

    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            AppError::new(ErrorCode::SystemError, "启动快速音频合并失败")
                .with_details(e.to_string())
        })?;

    crate::features::recording::job_object::assign_to_global_job_object(&child);

    let output = child.wait_with_output().map_err(|e| {
        AppError::new(ErrorCode::SystemError, "执行快速音频合并失败").with_details(e.to_string())
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let details = if stderr.is_empty() {
            format!("ffmpeg exit status: {}", output.status)
        } else {
            format!("ffmpeg exit status: {}；stderr: {}", output.status, stderr)
        };

        // 🔧 AAC 流复制失败时，回退到重编码模式重试一次
        // 常见场景：AAC 文件格式不完整/损坏，流复制无法处理但重编码可以恢复
        if is_aac {
            log::warn!(
                "⚠️ AAC 流复制失败，尝试重编码回退: {}",
                details
            );
            let _ = fs::remove_file(&merged_path);

            let mut retry_cmd = Command::new(ffmpeg_path);
            suppress_console_window(&mut retry_cmd);
            retry_cmd
                .arg("-hide_banner")
                .arg("-loglevel")
                .arg("warning")
                .arg("-y")
                .arg("-i")
                .arg(video_path)
                .arg("-i")
                .arg(audio_path)
                .arg("-map")
                .arg("0:v:0")
                .arg("-map")
                .arg("1:a:0")
                .arg("-c:v")
                .arg("copy")
                .arg("-c:a")
                .arg("aac")
                .arg("-b:a")
                .arg(format!("{}k", audio_bitrate_kbps.max(32)))
                .arg("-movflags")
                .arg("+faststart")
                .arg(&merged_path);

            let retry_child = retry_cmd
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| {
                    AppError::new(ErrorCode::SystemError, "启动AAC重编码回退失败")
                        .with_details(e.to_string())
                })?;

            crate::features::recording::job_object::assign_to_global_job_object(&retry_child);

            let retry_output = retry_child.wait_with_output().map_err(|e| {
                AppError::new(ErrorCode::SystemError, "执行AAC重编码回退失败")
                    .with_details(e.to_string())
            })?;

            if retry_output.status.success() {
                log::info!("✅ AAC 重编码回退成功");
                replace_file_atomically(&merged_path, video_path).map_err(|e| {
                    // 不再删除 merged_path：rename 失败时保留合并产物以便恢复
                    record_perf_metric(
                        "recording.audio_merge",
                        "录屏快速音频合并耗时(重编码回退)",
                        started_at.elapsed().as_millis() as u64,
                        false,
                        Some(e.to_string()),
                    );
                    AppError::new(ErrorCode::IoError, "写入重编码回退文件失败")
                        .with_details(e.to_string())
                })?;
                record_perf_metric(
                    "recording.audio_merge",
                    "录屏快速音频合并耗时(重编码回退)",
                    started_at.elapsed().as_millis() as u64,
                    true,
                    None,
                );
                let elapsed_ms = started_at.elapsed().as_millis();
                log::info!(
                    "✅ 快速音频合并完成（AAC重编码回退），耗时: {}ms",
                    elapsed_ms
                );
                return Ok(());
            } else {
                let retry_stderr = String::from_utf8_lossy(&retry_output.stderr)
                    .trim()
                    .to_string();
                log::error!("❌ AAC 重编码回退也失败了: {}", retry_stderr);
                let _ = fs::remove_file(&merged_path);
                record_perf_metric(
                    "recording.audio_merge",
                    "录屏快速音频合并耗时",
                    started_at.elapsed().as_millis() as u64,
                    false,
                    Some(format!("流复制失败:{}；重编码回退失败:{}", details, retry_stderr)),
                );
                return Err(AppError::new(ErrorCode::SystemError, "快速音频合并失败")
                    .with_details(format!("流复制失败:{}；重编码回退失败:{}", details, retry_stderr)));
            }
        }

        record_perf_metric(
            "recording.audio_merge",
            "录屏快速音频合并耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(details.clone()),
        );
        return Err(AppError::new(ErrorCode::SystemError, "快速音频合并失败").with_details(details));
    }

    replace_file_atomically(&merged_path, video_path).map_err(|e| {
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
    let operation = if is_aac {
        "AAC流复制"
    } else {
        "WAV→AAC重编码"
    };
    log::info!(
        "✅ 快速音频合并完成，耗时: {}ms ({})",
        elapsed_ms,
        operation
    );

    let warn_threshold = if is_aac { 2000 } else { 5000 };
    if elapsed_ms > warn_threshold {
        log::warn!("⚠️ 快速路径耗时较长({}ms)，考虑优化方案", elapsed_ms);
    }
    Ok(())
}

// 🔧 将两个已对齐的音频文件混音为单轨 AAC（统一重采样到 48kHz 以满足 amix 输入一致性）
fn mix_audio_files(
    ffmpeg_path: &std::path::Path,
    audio_a: &PathBuf,
    audio_b: &PathBuf,
    output_path: &PathBuf,
    audio_bitrate_kbps: u32,
) -> Result<(), AppError> {
    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);
    cmd.arg("-hide_banner").arg("-loglevel").arg("warning").arg("-y")
        .arg("-i").arg(audio_a)
        .arg("-i").arg(audio_b)
        .arg("-filter_complex")
        .arg("[0:a]aresample=48000,aformat=channel_layouts=stereo[a0];[1:a]aresample=48000,aformat=channel_layouts=stereo[a1];[a0][a1]amix=inputs=2:duration=longest:normalize=0[aout]")
        .arg("-map").arg("[aout]")
        .arg("-c:a").arg("aac")
        .arg("-b:a").arg(format!("{}k", audio_bitrate_kbps.max(32)))
        .arg(output_path);

    let output = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            AppError::new(ErrorCode::SystemError, "启动音频混音失败").with_details(e.to_string())
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let _ = fs::remove_file(output_path);
        let details = if stderr.is_empty() {
            format!("ffmpeg exit status: {}", output.status)
        } else {
            stderr
        };
        return Err(AppError::new(ErrorCode::SystemError, "音频混音失败").with_details(details));
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
    let output = cmd.output().map_err(|e| {
        AppError::new(ErrorCode::SystemError, "校验录制视频失败").with_details(e.to_string())
    })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let details = if stderr.is_empty() {
        format!("ffmpeg exit status: {}", output.status)
    } else {
        format!("ffmpeg exit status: {}；stderr: {}", output.status, stderr)
    };
    Err(
        AppError::new(ErrorCode::SystemError, "录制视频文件无效，无法合成音频")
            .with_details(details),
    )
}

fn rename_recording_output_with_retry(
    output_tmp: &PathBuf,
    output_final: &PathBuf,
) -> Result<(), AppError> {
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

/// 将临时文件重命名为最终输出；目标已存在时追加序号 (1)/(2)…，避免静默覆盖历史录制
fn rename_to_final_output(output_tmp: &PathBuf, output_final: &PathBuf) -> Result<PathBuf, AppError> {
    let target = if !output_final.exists() {
        output_final.clone()
    } else {
        let stem = output_final
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("recording");
        let ext = output_final
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("mp4");
        let dir = output_final.parent().unwrap_or(std::path::Path::new("."));
        let mut idx = 1;
        loop {
            let candidate = dir.join(format!("{} ({}).{}", stem, idx, ext));
            if !candidate.exists() {
                break candidate;
            }
            idx += 1;
        }
    };
    rename_recording_output_with_retry(output_tmp, &target)?;
    Ok(target)
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
                    AppError::new(ErrorCode::IoError, "读取录制视频失败")
                        .with_details(e.to_string()),
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
    Err(last_err
        .unwrap_or_else(|| AppError::new(ErrorCode::SystemError, "录制视频文件无效，无法合成音频")))
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

// ====================================================================
//  音频设备捕获管理
// ====================================================================

fn ensure_system_audio_capture_started(
    app: &AppHandle,
    runtime: &mut crate::features::recording::state::RecordingRuntime,
    output_dir: &PathBuf,
    session_id: &str,
    emit_error_on_fail: bool,
) -> Result<(), String> {
    // 确保 enabled_flag 和 pause_flag 始终存在，即使提前返回
    if runtime.system_audio_enabled_flag.is_none() {
        runtime.system_audio_enabled_flag = Some(Arc::new(AtomicBool::new(false)));
    }
    if runtime.recording_pause_flag.is_none() {
        runtime.recording_pause_flag = Some(Arc::new(AtomicBool::new(false)));
    }
    if !runtime.system_audio_threads.is_empty() {
        return Ok(());
    }
    let enabled_flag = runtime.system_audio_enabled_flag.clone().unwrap();
    let pause_flag = runtime.recording_pause_flag.clone().unwrap();
    // 设备错误槽位：采集线程 err_fn 写入、stats_loop 周期读取上报（原生路径无 stderr 可解析）
    let error_slot: crate::features::recording::state::AudioDeviceErrorSlot =
        Arc::new(std::sync::Mutex::new(None));
    runtime.system_audio_error_slot = Some(error_slot.clone());
    let seg_idx = runtime.system_audio_segments.len();
    if !runtime.system_audio_process_ids.is_empty() {
        let process_ids = runtime.system_audio_process_ids.clone();
        let output_paths = process_ids
            .iter()
            .enumerate()
            .map(|(idx, pid)| {
                output_dir.join(format!(
                    "{}.sys.proc{}.seg{}.wav",
                    session_id,
                    pid,
                    seg_idx + idx
                ))
            })
            .collect::<Vec<_>>();
        let first_try = start_process_loopback_wavs(
            process_ids,
            output_paths.clone(),
            enabled_flag.clone(),
            pause_flag.clone(),
            error_slot.clone(),
        );
        return match first_try {
            Ok(handle) => {
                let stream_start_ms = handle.stream_start_unix_ms;
                runtime.system_audio_stop_flag = Some(handle.stop_flag.clone());
                runtime.system_audio_threads = handle.joins;
                // 用采集线程回传的精确流启动时刻对齐时间轴，避免音频段整体偏早/偏晚
                let actual_start_ms = derive_audio_segment_start_ms(&runtime, stream_start_ms);
                runtime.system_audio_stream_start_ms = Some(actual_start_ms);
                for p in output_paths {
                    runtime.system_audio_segments.push(
                        crate::features::recording::state::AudioSegment { path: p, start_ms: actual_start_ms, trim_start_ms: 0, end_ms: None },
                    );
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

    let sys_aac = sys_wav.with_extension("aac");
    let mut system_fell_back_to_default = false;
    let first_try = start_system_loopback_aac_with_device(
        runtime.system_audio_device_id.clone(),
        sys_aac.clone(),
        enabled_flag.clone(),
        pause_flag.clone(),
        Some(runtime.audio_bitrate_kbps.max(32)),
        error_slot.clone(),
    );
    let start_result = match first_try {
        Ok(h) => Ok(h),
        Err(first_err) => {
            if runtime.system_audio_device_id.is_some() {
                runtime.system_audio_device_id = None;
                system_fell_back_to_default = true;
                start_system_loopback_aac_with_device(
                    None,
                    sys_aac.clone(),
                    enabled_flag,
                    pause_flag,
                    Some(runtime.audio_bitrate_kbps.max(32)),
                    error_slot,
                )
                .map_err(|second_err| format!("{}；回退默认设备失败: {}", first_err, second_err))
            } else {
                Err(first_err)
            }
        }
    };
    match start_result {
        Ok(handle) => {
            if system_fell_back_to_default {
                // 通知前端实际生效设备已变为默认：消除双侧状态脱节
                emit_recording_effective_audio_device(app, Some(session_id), "system", None);
            }
            let stream_start_ms = handle.stream_start_unix_ms;
            runtime.system_audio_wav_path = Some(sys_aac);
            runtime.system_audio_stop_flag = Some(handle.stop_flag.clone());
            runtime.system_audio_threads = handle.join.into_iter().collect();
            // 用采集线程回传的精确流启动时刻对齐时间轴（含设备初始化/回退的实际延迟）
            let actual_start_ms = derive_audio_segment_start_ms(&runtime, stream_start_ms);
            runtime.system_audio_stream_start_ms = Some(actual_start_ms);
            if let Some(path) = runtime.system_audio_wav_path.clone() {
                runtime
                    .system_audio_segments
                    .push(crate::features::recording::state::AudioSegment { path, start_ms: actual_start_ms, trim_start_ms: 0, end_ms: None });
            } else {
                return Err(AppErrorKind::InternalError.to_frontend_json());
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
    if runtime.mic_audio_enabled_flag.is_none() {
        runtime.mic_audio_enabled_flag = Some(Arc::new(AtomicBool::new(false)));
    }
    if runtime.recording_pause_flag.is_none() {
        runtime.recording_pause_flag = Some(Arc::new(AtomicBool::new(false)));
    }
    if runtime.mic_audio_thread.is_some() {
        return Ok(());
    }
    let enabled_flag = runtime.mic_audio_enabled_flag.clone().unwrap();
    let pause_flag = runtime.recording_pause_flag.clone().unwrap();
    // 设备错误槽位：采集线程 err_fn 写入、stats_loop 周期读取上报
    let error_slot: crate::features::recording::state::AudioDeviceErrorSlot =
        Arc::new(std::sync::Mutex::new(None));
    runtime.mic_audio_error_slot = Some(error_slot.clone());
    let seg_idx = runtime.mic_audio_segments.len();
    let mic_wav = if seg_idx == 0 {
        output_dir.join(format!("{}.mic.wav", session_id))
    } else {
        output_dir.join(format!("{}.mic.{}.wav", session_id, seg_idx))
    };
    let mut mic_fell_back_to_default = false;
    let first_try = start_microphone_wav_with_device(
        runtime.mic_audio_device_id.clone(),
        mic_wav.clone(),
        enabled_flag.clone(),
        pause_flag.clone(),
        error_slot.clone(),
    );
    let start_result = match first_try {
        Ok(h) => Ok(h),
        Err(first_err) => {
            if runtime.mic_audio_device_id.is_some() {
                runtime.mic_audio_device_id = None;
                mic_fell_back_to_default = true;
                start_microphone_wav_with_device(None, mic_wav.clone(), enabled_flag, pause_flag, error_slot)
                    .map_err(|second_err| {
                        format!("{}；回退默认设备失败: {}", first_err, second_err)
                    })
            } else {
                Err(first_err)
            }
        }
    };
    match start_result {
        Ok(handle) => {
            if mic_fell_back_to_default {
                emit_recording_effective_audio_device(app, Some(session_id), "mic", None);
            }
            let stream_start_ms = handle.stream_start_unix_ms;
            runtime.mic_audio_wav_path = Some(mic_wav);
            runtime.mic_audio_stop_flag = Some(handle.stop_flag.clone());
            runtime.mic_audio_thread = handle.joins.into_iter().next();
            // 用采集线程回传的精确流启动时刻对齐时间轴，避免麦克风相对系统音频提前/滞后
            let actual_start_ms = derive_audio_segment_start_ms(&runtime, stream_start_ms);
            runtime.mic_audio_stream_start_ms = Some(actual_start_ms);
            if let Some(path) = runtime.mic_audio_wav_path.clone() {
                runtime
                    .mic_audio_segments
                    .push(crate::features::recording::state::AudioSegment { path, start_ms: actual_start_ms, trim_start_ms: 0, end_ms: None });
            } else {
                return Err(AppErrorKind::InternalError.to_frontend_json());
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
    let ffmpeg_path = std::path::Path::new("");
    let outs = crate::features::recording::audio_device::list_system_audio_sources(ffmpeg_path)
        .map_err(|e| {
            AppError::new(ErrorCode::SystemError, "读取系统输出设备失败").with_details(e)
        })?;
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

/// 枚举可录制的显示器（多屏时前端展示选择列表）
pub fn list_recording_monitors() -> Result<Vec<RecordingMonitorItem>, AppError> {
    #[cfg(target_os = "windows")]
    {
        Ok(enumerate_monitor_infos()
            .into_iter()
            .map(|m| RecordingMonitorItem {
                index: m.index as u32,
                name: m.name,
                x: m.x,
                y: m.y,
                width: m.width,
                height: m.height,
                is_primary: m.x == 0 && m.y == 0,
            })
            .collect::<Vec<_>>())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }
}

fn cleanup_stale_tmp_files(output_dir: &PathBuf) {
    let threshold = SystemTime::now() - Duration::from_secs(24 * 3600);
    if let Ok(entries) = fs::read_dir(output_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let is_stale_pattern = file_name.ends_with(".tmp.mp4")
                || file_name.contains("_aligned.tmp.")
                || file_name.contains(".sys.wav")
                || file_name.contains(".mic.wav")
                || file_name.contains(".sys.aac")
                || file_name.contains(".mic.aac");
            if !is_stale_pattern {
                continue;
            }

            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext != "mp4" && ext != "wav" && ext != "aac" {
                continue;
            }

            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if modified > threshold {
                        continue;
                    }
                }
            }

            if let Err(e) = fs::remove_file(&path) {
                log::warn!("清理过期临时文件失败: {:?} - {}", path, e);
            } else {
                log::info!("已清理过期临时文件: {:?}", path);
            }
        }
    }
}

fn map_ffmpeg_error(line: &str) -> Option<(&'static str, String)> {
    let lower = line.to_lowercase();
    if lower.contains("error opening input file default")
        || lower.contains("error opening input files") && lower.contains("invalid argument")
    {
        return Some((
            AUDIO_DEVICE_NOT_FOUND,
            "系统音频设备不可用，请在录屏控制台关闭“系统音频”后重试".to_string(),
        ));
    }
    if lower.contains("device not found")
        || lower.contains("could not find audio device")
        || lower.contains("audio device") && (lower.contains("failed") || lower.contains("invalid"))
    {
        return Some((AUDIO_DEVICE_LOST, "音频设备不可用或已断开".to_string()));
    }
    if lower.contains("immediate exit requested")
        || lower.contains("conversion failed")
        || lower.contains("i/o error")
    {
        return Some((RECORDING_PROCESS_EXITED, line.trim().to_string()));
    }
    // M1 修复：移除过于宽泛的 "error" catch-all，避免非致命 FFmpeg 消息触发虚假错误
    // 仅匹配明确的致命错误模式
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

/// 提取 marker 之后的第一个十进制小数（用于解析 blackdetect 的秒值）
fn parse_f64_after(line: &str, marker: &str) -> Option<f64> {
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
    buf.parse::<f64>().ok().filter(|v| v.is_finite())
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

// ====================================================================
//  FFmpeg 进程监控与诊断
// ====================================================================

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
                        // 与"无画面看门狗"一致：触发自动收尾，回收音频线程，避免持续泄漏（#6）
                        runtime.auto_stop_requested = true;
                        auto_stop_session_id = session_id.clone();
                    }
                }
            }

            // WGC 会话意外死亡检测：目标窗口关闭/受保护内容/驱动重置会让捕获会话自行结束
            // （on_closed 或监控线程发现会话结束且非我方请求）。若不处理：视频停在最后一帧、
            // 音频继续录、状态机停在 Recording，用户无感知。与 ffmpeg 进程退出的处理对称；
            // 仅认 session_closed 标志，避免与暂停硬回退路径的 stop_flag 时序竞态。
            if phase == RecordingPhase::Recording && emit_error.is_none() {
                let session_dead = runtime
                    .wgc_session_closed
                    .as_ref()
                    .map(|f| f.load(Ordering::SeqCst))
                    .unwrap_or(false);
                if session_dead {
                    runtime.auto_stop_requested = true;
                    runtime.phase = RecordingPhase::Stopping;
                    phase = RecordingPhase::Stopping;
                    auto_stop_session_id = session_id.clone();
                    let err_msg =
                        "画面捕获会话已结束（目标窗口关闭或系统限制），正在保存已录制内容".to_string();
                    runtime.last_error = Some(err_msg.clone());
                    emit_error = Some((RECORDING_PROCESS_EXITED, err_msg, session_id.clone()));
                }
            }

            // 无画面看门狗：提前拦截“死黑屏/没内容”的假录制状态
            let segment_age_ms = runtime.video_segment_started_at
                .map(|t| t.elapsed().as_millis() as u64)
                .unwrap_or(u64::MAX);
            if phase == RecordingPhase::Recording
                && segment_age_ms > 4000
                && emit_error.is_none()
            {
                let mut no_video_frames = false;
                if is_wgc_target(&runtime.target_type) {
                    if let Some(first_frame) = runtime.wgc_first_frame_elapsed_ms.as_ref() {
                        if first_frame.load(Ordering::Relaxed) == u64::MAX {
                            no_video_frames = true;
                        }
                    }
                } else if let Some(current_seg) = runtime.window_video_segments.last() {
                    // FFmpeg 模式下，如果超过 4 秒文件依然是 0 字节，说明没录进任何有效视频帧
                    if fs::metadata(&current_seg.path).map(|m| m.len()).unwrap_or(0) == 0 {
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
                    emit_error = Some(("VALIDATION_ERROR", err_msg, session_id.clone()));
                }
            }

            // 原生音频采集的设备错误上报：cpal err_fn 写入槽位，此处消费并转发给前端
            let mut device_error_msg: Option<String> = None;
            if let Some(slot) = runtime.mic_audio_error_slot.as_ref() {
                if let Ok(mut guard) = slot.lock() {
                    if guard.is_some() {
                        device_error_msg = guard.take();
                    }
                }
            }
            if device_error_msg.is_none() {
                if let Some(slot) = runtime.system_audio_error_slot.as_ref() {
                    if let Ok(mut guard) = slot.lock() {
                        if guard.is_some() {
                            device_error_msg = guard.take();
                        }
                    }
                }
            }
            if let Some(msg) = device_error_msg.as_ref() {
                runtime.last_error = Some(msg.clone());
                if emit_error.is_none()
                    && matches!(
                        runtime.phase,
                        RecordingPhase::Recording | RecordingPhase::Paused
                    )
                {
                    emit_error = Some((
                        AUDIO_DEVICE_LOST,
                        format!("音频设备错误: {}", msg),
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
                snapshot.elapsed_ms,
            )
        };
        if let Some((code, message, sid)) = emit_error {
            emit_recording_error(&app, sid.as_deref(), code, message.as_str());
            // 进程异常退出进入 Error 态时补发状态事件，否则前端停留在"录制中"无法恢复
            if phase == RecordingPhase::Error {
                emit_recording_state_changed(
                    &app,
                    sid.as_deref(),
                    RecordingPhase::Error.as_str(),
                    elapsed_ms,
                );
            }
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

fn spawn_ffmpeg_video_segment(
    ffmpeg_path: &std::path::Path,
    target_type: &str,
    target_id: &str,
    fps: u32,
    capture_cursor: bool,
    video_bitrate: u32,
    output_path: &PathBuf,
) -> Result<(std::process::Child, std::process::ChildStderr), AppError> {
    let mut args = Vec::new();

    // 🔧 全局 flags：生成正确 PTS，丢弃损坏帧，增大输入缓冲防止初始帧丢失
    args.push("-fflags".into());
    args.push("+genpts+discardcorrupt".into());
    args.push("-thread_queue_size".into());
    args.push("1024".into());

    match target_type {
        "window" => {
            return Err(AppError::new(
                ErrorCode::ValidationError,
                "WGC 模式不支持 FFmpeg 分段录制",
            ));
        }
        "gdigrab_window" => {
            args.push("-f".into());
            args.push("gdigrab".into());
            args.push("-framerate".into());
            args.push(format!("{}", fps));
            args.push("-draw_mouse".into());
            args.push(if capture_cursor {
                "1".into()
            } else {
                "0".into()
            });
            args.push("-i".into());
            args.push(format!("title={}", target_id));
        }
        "region" => {
            let (x, y, width, height) = parse_region_target(target_id).ok_or_else(|| {
                AppError::new(ErrorCode::ValidationError, "区域录制参数无效")
                    .with_details(format!("target_id={}", target_id))
            })?;
            let (x, y, width, height) = normalize_region_to_virtual_screen(x, y, width, height)
                .ok_or_else(|| {
                    AppError::new(ErrorCode::ValidationError, "区域录制参数无效")
                        .with_details("virtual screen unavailable")
                })?;
            args.push("-f".into());
            args.push("gdigrab".into());
            args.push("-framerate".into());
            args.push(format!("{}", fps));
            args.push("-draw_mouse".into());
            args.push(if capture_cursor {
                "1".into()
            } else {
                "0".into()
            });
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
            args.push(if capture_cursor {
                "1".into()
            } else {
                "0".into()
            });
            args.push("-i".into());
            args.push("desktop".into());
        }
    }

    args.push("-map".to_string());
    args.push("0:v:0".to_string());
    args.push("-vf".to_string());
    args.push("scale=trunc(iw/2)*2:trunc(ih/2)*2".to_string());
    args.extend_from_slice(&[
        "-c:v".to_string(),
        "libx264".to_string(),
        "-preset".to_string(),
        "veryfast".to_string(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
        "-r".to_string(),
        format!("{}", fps),           // 输出帧率
        "-g".to_string(),
        format!("{}", fps),           // GOP 大小 = 帧率，每秒一个关键帧
        "-b:v".to_string(),
        format!("{}k", video_bitrate),
        "-movflags".to_string(),      // moov atom 前置，改善预览体验
        "+faststart".to_string(),
    ]);
    args.push("-an".to_string());
    args.push(output_path.to_string_lossy().to_string());

    let mut command = Command::new(ffmpeg_path);
    suppress_console_window(&mut command);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|e| {
        AppError::new(ErrorCode::SystemError, "启动录制片段失败").with_details(e.to_string())
    })?;

    // 🔧 将录制进程绑定到 Job Object 以确保随主进程退出
    crate::features::recording::job_object::assign_to_global_job_object(&child);

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            AppError::new(ErrorCode::SystemError, "获取录制进程 stderr 失败")
        })?;

    Ok((child, stderr))
}

// ====================================================================
//  录制控制 API — start / stop / cancel / pause / resume
// ====================================================================

pub fn start_recording(
    app: &AppHandle,
    state_arc: Arc<Mutex<SharedAppState>>,
    request: StartRecordingRequest,
) -> Result<RecordingSessionInfo, AppError> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, state_arc, request);
        return Err(AppError::new(
            ErrorCode::SystemError,
            "当前平台暂不支持录屏",
        ));
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
                request
                    .capture_cursor
                    .unwrap_or(state_guard.settings.recording_capture_cursor),
                request
                    .capture_microphone
                    .unwrap_or(state_guard.settings.recording_capture_microphone),
                request
                    .capture_system_audio
                    .unwrap_or(state_guard.settings.recording_capture_system_audio),
                request
                    .fps
                    .unwrap_or(state_guard.settings.recording_default_fps)
                    .clamp(1, 120),
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

        fs::create_dir_all(&output_dir).map_err(|e| {
            AppError::new(ErrorCode::IoError, "创建录制目录失败").with_details(e.to_string())
        })?;
        cleanup_stale_tmp_files(&output_dir);
        let (tmp_path, final_path, session_id) = build_output_paths(&output_dir, &settings_snapshot.recording_file_name_template);
        let mut runtime = lock_arc_mutex(&runtime_arc);
        normalize_runtime_state(&mut runtime);
        if matches!(
            runtime.phase,
            RecordingPhase::Recording
                | RecordingPhase::Starting
                | RecordingPhase::Paused
                | RecordingPhase::Stopping
        ) {
            return Err(AppError::new(
                ErrorCode::ValidationError,
                "已有录制任务在运行",
            ));
        }
        runtime.phase = RecordingPhase::Starting;
        runtime.last_error = None;
        // starting 态即携带 sessionId，供前端过滤旧会话的迟到事件（如上一段后台合并的 finished）
        emit_recording_state_changed(app, Some(&session_id), runtime.phase.as_str(), 0);
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
        // 回归自测使用的别名，统一为全屏语义
        if target_type == "display" {
            target_type = "screen".to_string();
        }
        let mut target_id = request.target_id.clone().unwrap_or_default();
        // 统一录制时钟起点：必须早于视频采集与音频采集启动，避免后续音频延迟估算偏小导致 A/V 不同步。
        let capture_origin_unix_ms = now_unix_ms();
        let capture_origin_instant = std::time::Instant::now();
        let mut window_wgc_handle = None;
        let mut window_segment_path: Option<PathBuf> = None;
        bootstrap_force_default_border_from_settings(
            settings_snapshot.recording_wgc_force_default_border,
        );
        bootstrap_force_default_dirty_region_from_settings(
            settings_snapshot.recording_wgc_force_default_dirty_region,
        );
        let _args: Vec<String> = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "warning".into(),
            "-y".into(),
        ];

        #[cfg(debug_assertions)]
        let force_ffmpeg_fallback = settings_snapshot.dev_force_ffmpeg_window_capture;
        #[cfg(not(debug_assertions))]
        let force_ffmpeg_fallback = false;

        // 第一次尝试处理 window，如果命中 WGC 不支持的内容，降级到 gdigrab 的窗口模式
        if target_type == "window" {
            if target_id.trim().is_empty() {
                return Err(rollback_starting(
                    "窗口录制目标不能为空",
                    "target_id is empty".to_string(),
                ));
            }
            if let Err(e) = validate_window_capture_target(target_id.trim()) {
                return Err(rollback_starting("当前窗口不可录制", e));
            }

            if force_ffmpeg_fallback {
                log::warn!("开发模式：强制将 WGC 窗口录制降级为 GDI/FFmpeg 窗口录制");
                if let Ok(title) =
                    crate::features::recording::wgc_capture::get_window_title_from_target(
                        target_id.trim(),
                    )
                {
                    target_type = "gdigrab_window".to_string();
                    target_id = title;
                } else {
                    return Err(rollback_starting(
                        "开发模式强制降级失败：无法获取目标窗口标题",
                        "".to_string(),
                    ));
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
                            log::warn!(
                                "WGC 窗口录制被系统拒绝 ({})，自动降级为 GDI/FFmpeg 窗口录制",
                                e
                            );
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

        // 全屏（WGC 显示器捕获：硬件编码、分段携带首帧锚点）与区域（显示器+裁剪）路由。
        // 多屏时全屏必须显式指定目标屏（target_id="mon=N"），避免静默改变录制范围。
        // 任何 WGC 启动失败均回退 gdigrab 原路径。
        if matches!(target_type.as_str(), "screen" | "region") && !force_ffmpeg_fallback {
            #[cfg(target_os = "windows")]
            let monitor_start: Option<(usize, Option<(u32, u32, u32, u32)>)> =
                if target_type == "screen" {
                    match parse_screen_explicit_monitor(&target_id) {
                        Some(idx) => {
                            if idx < monitor_count() {
                                Some((idx, None))
                            } else {
                                return Err(rollback_starting(
                                    "指定的录制屏幕不存在",
                                    format!("mon={}，检测到 {} 块屏幕", idx, monitor_count()),
                                ));
                            }
                        }
                        None => {
                            if monitor_count() > 1 {
                                return Err(rollback_starting(
                                    "检测到多个显示器，请先在录屏工具条中选择要录制的屏幕",
                                    format!("monitors={}", monitor_count()),
                                ));
                            } else {
                                // 单屏（或枚举失败交由 gdigrab 兜底）
                                (monitor_count() == 1).then_some((0usize, None))
                            }
                        }
                    }
                } else {
                    resolve_wgc_monitor_start_params(&target_type, &target_id)
                };
            #[cfg(not(target_os = "windows"))]
            let monitor_start: Option<(usize, Option<(u32, u32, u32, u32)>)> = None;

            if let Some((mon_index, crop_local)) = monitor_start {
                let first_segment_path = build_window_segment_path(&output_dir, &session_id, 0);
                match start_monitor_capture_to_mp4(
                    mon_index,
                    crop_local,
                    first_segment_path.clone(),
                    fps,
                    video_bitrate,
                    capture_cursor,
                    capture_origin_instant,
                ) {
                    Ok(handle) => {
                        window_wgc_handle = Some(handle);
                        window_segment_path = Some(first_segment_path);
                        let mut new_id = format!("mon={}", mon_index);
                        if let Some(c) = crop_local {
                            new_id.push_str(&format!(",crop={},{},{},{}", c.0, c.1, c.2, c.3));
                        }
                        target_type = if crop_local.is_some() {
                            "wgc_region".to_string()
                        } else {
                            "wgc_screen".to_string()
                        };
                        target_id = new_id;
                    }
                    Err(e) => {
                        log::warn!("WGC 显示器捕获启动失败({})，回退 gdigrab", e);
                    }
                }
            }
        }

        let (child_opt, stderr_opt) = if !is_wgc_target(&target_type) {
            let first_segment_path = build_window_segment_path(&output_dir, &session_id, 0);
            match spawn_ffmpeg_video_segment(
                &ffmpeg_path,
                &target_type,
                &target_id,
                fps,
                capture_cursor,
                video_bitrate,
                &first_segment_path,
            ) {
                Ok((child, stderr)) => {
                    window_segment_path = Some(first_segment_path);
                    (Some(child), Some(stderr))
                }
                Err(e) => {
                    emit_recording_error(app, None, RECORDING_START_FAILED, "录制进程启动失败");
                    return Err(rollback_starting("启动录制失败", e.to_string()));
                }
            }
        } else {
            (None, None)
        };
        let ffmpeg_spawned_at = std::time::Instant::now();
        runtime.phase = RecordingPhase::Recording;
        runtime.session_id = Some(session_id.clone());
        runtime.started_at_ms = capture_origin_unix_ms;
        runtime.started_instant = Some(capture_origin_instant);
        runtime.video_segment_started_at = Some(capture_origin_instant);
        runtime.paused_at_instant = None;
        runtime.paused_total_ms = 0;
        // 记录 FFmpeg 启动延迟（仅对非窗口录制有意义）
        runtime.ffmpeg_start_delay_ms = if !is_wgc_target(&target_type) {
            ffmpeg_spawned_at.duration_since(capture_origin_instant).as_millis() as u64
        } else {
            0
        };
        runtime.max_duration_ms =
            (settings_snapshot.recording_max_duration_minutes as u64).saturating_mul(60_000);
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
            let seg0_u_start_ms = runtime.snapshot().elapsed_ms;
            runtime
                .window_video_segments
                .push(crate::features::recording::state::WindowVideoSegment {
                    path: seg_path.clone(),
                    // 首个分段从 U 时钟 0 开始（started_instant 刚设置，elapsed≈0）
                    u_start_ms: seg0_u_start_ms,
                    first_frame_anchor: window_wgc_handle
                        .as_ref()
                        .map(|h| h.first_frame_elapsed_ms.clone()),
                });
        }
        // 系统音频关闭时不占用 loopback 设备；重新开启时再创建新音频分段并在合成阶段按 start_ms 对齐。
        if capture_system_audio {
            log::info!("🔧 尝试启动系统音频捕获...");
            match ensure_system_audio_capture_started(
                app,
                &mut runtime,
                &output_dir,
                &session_id,
                true,
            ) {
                Ok(()) => log::info!("✅ 系统音频捕获启动成功"),
                Err(e) => {
                    log::error!("❌ 系统音频捕获启动失败: {}", e);
                    runtime.last_error = Some(format!("系统音频未录制: {}", e));
                }
            }
        }
        if capture_microphone {
            if let Err(e) = ensure_mic_capture_started(app, &mut runtime, &output_dir, &session_id, true) {
                log::error!("麦克风捕获启动失败: {}", e);
                runtime.last_error = Some(format!("麦克风未录制: {}", e));
            }
        }
        runtime.process = child_opt;
        if let Some(handle) = window_wgc_handle {
            runtime.wgc_stop_flag = Some(handle.stop_flag);
            runtime.wgc_pause_flag = Some(handle.pause_flag);
            runtime.wgc_session_closed = Some(handle.session_closed.clone());
            runtime.wgc_first_frame_elapsed_ms = Some(handle.first_frame_elapsed_ms);
            runtime.wgc_thread = Some(handle.join);
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
        _was_paused,
        mut process,
        wgc_thread,
        wgc_first_frame_elapsed_ms,
        wgc_audio_sync_advance_ms,
        ffmpeg_start_delay_ms,
        system_audio_stop_flag,
        system_audio_threads,
        mic_audio_stop_flag,
        mic_audio_thread,
        output_tmp,
        mut output_final,
        mut sys_segments,
        mut mic_segments,
        window_video_segments,
        audio_segment_paths,
    ) = {
        let mut runtime = lock_arc_mutex(&runtime_arc);
        let allow_auto_stop_finalize =
            runtime.phase == RecordingPhase::Stopping && runtime.auto_stop_requested;
        if runtime.phase == RecordingPhase::Stopping && !allow_auto_stop_finalize {
            // 已有停止流程在途（手动重复点击，或自动停止收尾已接管并复位 auto_stop 标志）。
            // 直接幂等拒绝：此时句柄已被在途流程 take，若放行会拿到空句柄集继续后处理，
            // 并触发命令层兜底 cancel 把在途流程的 runtime 强制重置（duration=0/重复事件）。
            return Err(AppError::new(
                ErrorCode::ValidationError,
                "录制正在停止中，请勿重复操作",
            ));
        }
        if runtime.phase != RecordingPhase::Recording
            && runtime.phase != RecordingPhase::Paused
            && !allow_auto_stop_finalize
        {
            return Err(AppError::new(
                ErrorCode::ValidationError,
                "当前没有正在进行的录制任务",
            ));
        }
        if let Some(ref expected) = request.session_id {
            if runtime.session_id.as_deref() != Some(expected.as_str()) {
                return Err(AppError::new(
                    ErrorCode::ValidationError,
                    "录制会话已变化，请刷新状态后重试",
                ));
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
        let ffmpeg_start_delay_ms = runtime.ffmpeg_start_delay_ms;
        runtime.wgc_stop_flag = None;
        let system_audio_stop_flag = runtime.system_audio_stop_flag.take();
        let system_audio_threads = std::mem::take(&mut runtime.system_audio_threads);
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
            ffmpeg_start_delay_ms,
            system_audio_stop_flag,
            system_audio_threads,
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

    // 🔧 记录停止流程开始时间
    let stop_started_at = std::time::Instant::now();
    let mut video_exit_elapsed: Option<std::time::Duration> = None;

    // 关键修复：根据录制类型采用不同的停止顺序，确保音频录制到视频完全停止的时刻
    // 问题：音频提前停止导致视频最后一段没有声音

    if is_wgc_target(&target_type) {
        // WGC 托管录制（窗口/单屏/区域）：先停止WGC线程，再停止音频
        log::info!("🔧 窗口录制：首先停止WGC线程...");
        if let Some(join) = wgc_thread {
            let mut wgc_exited = false;
            for _ in 0..500 {
                // wait up to 5 seconds
                if join.is_finished() {
                    wgc_exited = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            if wgc_exited {
                match join.join() {
                    Ok(Ok(())) => {
                        log::info!("✅ WGC线程已退出");
                    }
                    Ok(Err(e)) => {
                        if is_benign_wgc_stop_error(&e) {
                            log::warn!("窗口录制停止返回可忽略状态: {}", e);
                        } else if is_item_convert_failed(&e) {
                            pending_window_capture_unavailable_details = Some(e);
                        } else if fatal_error.is_none() {
                            fatal_error = Some(
                                AppError::new(ErrorCode::SystemError, "窗口录制停止失败")
                                    .with_details(e),
                            );
                        }
                    }
                    Err(_) => {
                        if fatal_error.is_none() {
                            fatal_error = Some(AppError::new(
                                ErrorCode::SystemError,
                                "窗口录制线程异常退出",
                            ));
                        }
                    }
                }
            } else {
                log::warn!("WGC 线程停止超时 (5.0s)，强制等待退出...");
                let _ = join.join();
            }
        }
        persist_wgc_capture_fallback_if_needed(&state_arc);

        // WGC线程已停止，现在停止音频
        log::info!("🔧 WGC已停止，现在设置音频停止信号...");
        if let Some(flag) = system_audio_stop_flag.as_ref() {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        if let Some(flag) = mic_audio_stop_flag.as_ref() {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        log::info!("✅ 音频停止信号已设置");
    } else {
        // 非窗口录制（FFmpeg）：同时停止视频和音频，避免音频冗余
        log::info!("🔧 非窗口录制：同时发送音视频停止信号...");

        // 1. 首先发送音频停止信号（音频线程会继续录制2秒以确保覆盖）
        if let Some(flag) = system_audio_stop_flag.as_ref() {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        if let Some(flag) = mic_audio_stop_flag.as_ref() {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        log::info!("✅ 音频停止信号已设置");

        // 2. 然后停止FFmpeg进程
        if let Some(process) = process.as_mut() {
            if let Some(stdin) = process.stdin.as_mut() {
                let _ = stdin.write_all(b"q\n");
                let _ = stdin.flush();
            }
        }

        // 3. 等待FFmpeg进程退出（需要时间处理最后一帧并写入文件）
        log::info!("🔧 等待视频录制进程退出...");
        let video_exit_start = std::time::Instant::now();
        let mut video_exited = false;
        if let Some(process) = process.as_mut() {
            for _ in 0..800 {
                if let Ok(Some(_)) = process.try_wait() {
                    video_exited = true;
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            if !video_exited {
                let _ = process.kill();
                let _ = process.wait();
            }
        }
        video_exit_elapsed = Some(video_exit_start.elapsed());
        log::info!(
            "✅ 视频录制进程已退出，耗时: {}ms",
            video_exit_elapsed.unwrap_or_default().as_millis()
        );

        // 对于非窗口录制，wgc_thread应该为None，不需要处理
        if wgc_thread.is_some() {
            log::warn!("非窗口录制模式下发现意外的WGC线程");
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
            // BUG-10 改进：使用实际测量的首帧延迟进行校正
            // anchor_ms 已经是从录制开始到首帧的实际经过时间，直接作为校正值
            // wgc_audio_sync_advance_ms 仅作为额外的安全裕量（可选）
            // 当 anchor_ms 较大（>200ms）时，说明系统延迟本身已足够，不需要额外裕量
            let safety_margin = if anchor_ms > 200 { 0 } else { wgc_audio_sync_advance_ms.min(20) };
            let calibrated_anchor_ms = anchor_ms.saturating_add(safety_margin);
            // 多段窗口录制（暂停/恢复产生多个 WGC 分段）时，每段有自己的首帧锚点，
            // 全局单锚点会把末段锚点误用于所有音频分段 → 音画失步；
            // 此时按分段周期分别计算 U→拼接时间轴的偏移。
            let per_cycle = window_video_segments.len() > 1
                && window_video_segments
                .iter()
                .any(|s| s.first_frame_anchor.is_some());
            if per_cycle {
                let total_u_ms = {
                    let rt = lock_arc_mutex(&runtime_arc);
                    rt.snapshot().elapsed_ms
                };
                // 容器实测各分段时长：消除停止轮询/编码器收尾尾差在暂停边界的累积误差
                let measured: Vec<Option<u64>> = window_video_segments
                    .iter()
                    .map(|s| probe_video_duration_ms(&ffmpeg_path, &s.path))
                    .collect();
                let shifts = compute_window_segment_shifts(
                    &window_video_segments,
                    &measured,
                    calibrated_anchor_ms,
                    total_u_ms,
                );
                apply_window_cycle_shifts(&mut sys_segments, &window_video_segments, &shifts);
                apply_window_cycle_shifts(&mut mic_segments, &window_video_segments, &shifts);
                log::info!(
                    "应用 WGC 分段周期校正: segments={:?}, measured={:?}, shifts={:?}, calibrated_last_anchor={}ms",
                    window_video_segments.iter().map(|s| s.u_start_ms).collect::<Vec<_>>(),
                    measured,
                    shifts,
                    calibrated_anchor_ms
                );
            } else {
                shift_audio_segments_global(&mut sys_segments, calibrated_anchor_ms);
                shift_audio_segments_global(&mut mic_segments, calibrated_anchor_ms);
                log::info!(
                    "应用 WGC 首帧锚点校正: anchor_ms={}, safety_margin={}, calibrated_anchor_ms={}",
                    anchor_ms,
                    safety_margin,
                    calibrated_anchor_ms
                );
            }
        } else if let Some(details) = pending_window_capture_unavailable_details.take() {
            fatal_error = Some(build_window_capture_unavailable_error(&details));
        }
    } else if let Some(details) = pending_window_capture_unavailable_details.take() {
        fatal_error = Some(build_window_capture_unavailable_error(&details));
    }

    // FFmpeg/gdigrab 录制：先探测片头是否存在黑帧段（gdigrab 初始化遗留）。
    // 现代 ffmpeg 通常无黑帧 → 探测结果为 0，既不再盲删开头 300ms 真实内容，
    // 音频补偿也同步使用实测值，避免固定常量引入的音画偏移。
    let measured_black_lead_ms: u64 = if !is_wgc_target(&target_type) && fatal_error.is_none() {
        match output_tmp.as_ref().and_then(|p| detect_black_lead_ms(&ffmpeg_path, p)) {
            Some(ms) if ms >= MIN_BLACK_LEAD_TRIM_MS => {
                log::info!("探测到片头黑帧 {}ms，将裁剪并同步音频补偿", ms);
                ms
            }
            _ => {
                log::info!("片头未检测到黑帧，跳过灰头裁剪");
                0
            }
        }
    } else {
        0
    };

    // FFmpeg/gdigrab 录制 A/V 同步校正：测量 FFmpeg 进程启动相对于录制原点的延迟
    // 类似 WGC 的 first_frame_elapsed_ms 机制，消除音频提前 10~50ms 的固有偏差
    // 注意：若探测到片头黑帧并将在后处理中裁掉，视频时间线整体前移该时长，
    // 音频侧需同步多扣实测黑帧时长，否则音频相对视频滞后
    if !is_wgc_target(&target_type)
        && (ffmpeg_start_delay_ms > 0 || measured_black_lead_ms > 0)
    {
        let effective_delay = ffmpeg_start_delay_ms.saturating_add(measured_black_lead_ms);
        log::info!(
            "应用 FFmpeg 启动延迟校正: ffmpeg_delay={}ms, trim_compensation={}ms, effective={}ms",
            ffmpeg_start_delay_ms,
            measured_black_lead_ms,
            effective_delay
        );
        for seg in &mut sys_segments {
            let orig_start_ms = seg.start_ms;
            if let Some(end_ms) = seg.end_ms.as_mut() {
                *end_ms = if *end_ms > effective_delay {
                    *end_ms - effective_delay
                } else {
                    0
                };
            }
            if orig_start_ms < effective_delay {
                seg.trim_start_ms = effective_delay - orig_start_ms;
                seg.start_ms = 0;
            } else {
                seg.start_ms = orig_start_ms - effective_delay;
                seg.trim_start_ms = 0;
            }
        }
        for seg in &mut mic_segments {
            if seg.start_ms < effective_delay {
                seg.trim_start_ms = effective_delay - seg.start_ms;
                seg.start_ms = 0;
            } else {
                seg.start_ms = seg.start_ms - effective_delay;
                seg.trim_start_ms = 0;
            }
        }
    }

    if let (Some(output_tmp), Some(output_final)) = (output_tmp.as_ref(), output_final.as_mut()) {
        log::info!("🔧 开始视频后处理...");
        let video_post_start = std::time::Instant::now();

        if !window_video_segments.is_empty() && fatal_error.is_none() {
            log::info!("🔧 合并视频片段...");
            let seg_paths: Vec<PathBuf> =
                window_video_segments.iter().map(|s| s.path.clone()).collect();
            if let Err(e) = concat_video_segments(&ffmpeg_path, &seg_paths, output_tmp) {
                fatal_error = Some(e);
            }
        } else if fatal_error.is_none() && window_video_segments.is_empty() {
            // 立即开始并停止的极短录制没有任何视频分段，给出明确错误而非 rename 失败
            fatal_error = Some(AppError::new(
                ErrorCode::ValidationError,
                "录制时间过短，未生成视频文件",
            ));
        }

        // 🔧 非窗口录制（gdigrab）且探测到片头黑帧时才裁剪（实测值），
        // 现代 ffmpeg 无黑帧 → 不再盲删开头 300ms 真实内容
        // （音频校准已按实测黑帧时长同步补偿）
        if fatal_error.is_none()
            && !is_wgc_target(&target_type)
            && measured_black_lead_ms >= MIN_BLACK_LEAD_TRIM_MS
        {
            log::info!("🔧 裁剪 gdigrab 片头黑帧 {}ms...", measured_black_lead_ms);
            if let Err(e) = trim_video_initial_frames(
                &ffmpeg_path,
                output_tmp,
                measured_black_lead_ms as f64 / 1000.0,
            ) {
                // 裁剪失败不影响主流程，只记录警告
                log::warn!("裁剪片头黑帧失败（不影响视频保存）: {}", e);
            }
        }

        if fatal_error.is_none() {
            log::info!("🔧 重命名输出文件...");
            match rename_to_final_output(output_tmp, output_final) {
                Ok(actual_path) => *output_final = actual_path,
                Err(e) => fatal_error = Some(e),
            }
        }

        log::info!(
            "✅ 视频后处理完成，耗时: {}ms",
            video_post_start.elapsed().as_millis()
        );
    } else if fatal_error.is_none() {
        fatal_error = Some(AppError::new(ErrorCode::SystemError, "录制输出路径不存在"));
    }

    // 🔧 等待音频线程退出（停止信号已在前面设置）
    log::info!("🔧 等待系统音频线程退出...");
    let sys_audio_join_start = std::time::Instant::now();
    let mut sys_audio_timed_out = false;
    for join in system_audio_threads {
        if !join_thread_with_timeout(join, "stop 系统音频", 500) {
            sys_audio_timed_out = true;
        }
    }
    if sys_audio_timed_out {
        log::warn!("系统音频线程超时，跳过系统音频合并以避免使用不完整文件");
        sys_segments.clear();
    }
    let sys_audio_elapsed = sys_audio_join_start.elapsed().as_millis();
    if sys_audio_elapsed > 100 {
        log::info!("✅ 系统音频线程已退出，join耗时: {}ms", sys_audio_elapsed);
    } else {
        log::debug!("✅ 系统音频线程已退出，join耗时: {}ms", sys_audio_elapsed);
    }

    log::info!("🔧 等待麦克风音频线程退出...");
    let mic_audio_join_start = std::time::Instant::now();
    if let Some(join) = mic_audio_thread {
        if !join_thread_with_timeout(join, "stop 麦克风音频", 500) {
            log::warn!("麦克风音频线程超时，跳过麦克风音频合并以避免使用不完整文件");
            mic_segments.clear();
        }
    }
    let mic_audio_elapsed = mic_audio_join_start.elapsed().as_millis();
    if mic_audio_elapsed > 100 {
        log::info!("✅ 麦克风音频线程已退出，join耗时: {}ms", mic_audio_elapsed);
    } else {
        log::debug!("✅ 麦克风音频线程已退出，join耗时: {}ms", mic_audio_elapsed);
    }

    // 🔧 记录总停止耗时
    let total_stop_ms = stop_started_at.elapsed().as_millis();
    log::info!(
        "📊 录制停止总耗时: {}ms (视频={ }ms, 系统音频={}ms, 麦克风={}ms)",
        total_stop_ms,
        video_exit_elapsed.map(|e| e.as_millis()).unwrap_or(0),
        sys_audio_elapsed,
        mic_audio_elapsed
    );

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

    // 只清理窗口视频片段（已经在同步阶段使用完毕）；
    // concat 失败时保留分段，避免用户失去手动恢复可能（#4）
    if fatal_error.is_none() {
        for seg in window_video_segments {
            let _ = fs::remove_file(&seg.path);
        }
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

    let (duration_ms, saved_audio_bitrate_kbps) = {
        let mut runtime = lock_arc_mutex(&runtime_arc);
        if let Some(paused_at) = runtime.paused_at_instant {
            runtime.paused_total_ms = runtime
                .paused_total_ms
                .saturating_add(paused_at.elapsed().as_millis() as u64);
            runtime.paused_at_instant = None;
        }
        let duration_ms = runtime.snapshot().elapsed_ms;
        let bitrate = runtime.audio_bitrate_kbps;
        runtime.reset_to_idle();
        emit_recording_state_changed(app, None, runtime.phase.as_str(), 0);
        (duration_ms, bitrate)
    };

    if let Some(err) = fatal_error {
        // 清理错误路径上的临时音频文件
        for path in &audio_segment_paths {
            let _ = fs::remove_file(path);
        }
        return Err(err);
    }

    let result = RecordingStopResult {
        session_id: session_id.clone(),
        output_path: output_path_for_result
            .ok_or_else(|| AppError::new(ErrorCode::SystemError, "录制停止失败"))?,
        duration_ms,
        file_size_bytes,
    };

    // ✅ 在后台异步执行音频合并，不阻塞 UI
    let runtime_audio_bitrate_kbps = saved_audio_bitrate_kbps;
    if !sys_segments.is_empty() || !mic_segments.is_empty() {
        let app_handle = app.clone();
        let session_id_clone = session_id.clone();
        let ffmpeg_path_clone = ffmpeg_path.clone();
        let output_final_clone = match output_final.clone() {
            Some(path) => path,
            None => {
                log::warn!("音频合并跳过：输出路径为空");
                // 无输出路径时直接发送完成事件
                emit_recording_finished(app, &result);
                return Ok(result);
            }
        };

        // ✅ 将音频片段路径 HashSet 转换为 Vec，用于合并后清理
        // audio_segment_paths 已从元组解构获得（L1580）
        let audio_segment_paths_vec: Vec<std::path::PathBuf> =
            audio_segment_paths.into_iter().collect();

        // ✅ 移动 result 到 async 块中，音频合并完成后再发送 finished 事件
        let result_for_emit = result.clone();
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
            let merge_result = tauri::async_runtime::spawn_blocking(move || {
                merge_system_audio_into_video(
                    &ffmpeg_path_clone,
                    &output_final_clone,
                    &sys_segments,
                    &mic_segments,
                    runtime_audio_bitrate_kbps,
                )
            })
            .await
            .unwrap_or_else(|e| {
                Err(AppError::new(ErrorCode::SystemError, "音频合并任务崩溃")
                    .with_details(e.to_string()))
            });

            // ✅ 合并完成后，清理临时音频片段文件
            let mut cleaned_count = 0;
            let mut not_found_count = 0;
            for path in &audio_segment_paths_vec {
                // ✅ 添加诊断日志：检查文件是否存在
                match std::fs::metadata(path) {
                    Ok(meta) => {
                        log::info!(
                            "准备清理音频片段: {:?}, 大小: {} bytes",
                            path.file_name(),
                            meta.len()
                        );
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
                        log::warn!(
                            "音频片段文件在清理前就不存在: {:?}, {}",
                            path.file_name(),
                            e
                        );
                        not_found_count += 1;
                    }
                }
            }
            log::info!(
                "已清理 {}/{} 个音频片段文件 ({} 个不存在)",
                cleaned_count,
                audio_segment_paths_vec.len(),
                not_found_count
            );

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
                    // ✅ 音频合并完成后才发送 finished 事件，确保 UI 看到的视频已包含音频
                    emit_recording_finished(&app_handle, &result_for_emit);
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
                    // ✅ 即使音频合并失败，也发送 finished 事件（视频仍可用，只是无音频）
                    emit_recording_finished(&app_handle, &result_for_emit);
                    emit_recording_error(
                        &app_handle,
                        Some(&session_id_clone),
                        RECORDING_PROCESS_EXITED,
                        &msg,
                    );
                }
            }
        });
    } else {
        // ✅ 如果没有音频片段，直接清理 window_video_segments 并发送完成事件
        log::info!("无音频片段，跳过音频合并");
        emit_recording_finished(app, &result);
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
        system_audio_threads,
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
                return Err(AppError::new(
                    ErrorCode::ValidationError,
                    "录制会话已变化，请刷新状态后重试",
                ));
            }
        }

        runtime.phase = RecordingPhase::Stopping;
        runtime.auto_stop_requested = false;
        let process = runtime.process.take();
        let wgc_stop_flag = runtime.wgc_stop_flag.take();
        let wgc_thread = runtime.wgc_thread.take();
        let system_audio_stop_flag = runtime.system_audio_stop_flag.take();
        let system_audio_threads = std::mem::take(&mut runtime.system_audio_threads);
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
        if let Some(path) = runtime.output_path_final.take() {
            cleanup_paths.insert(path);
        }
        for seg in std::mem::take(&mut runtime.system_audio_segments) {
            cleanup_paths.insert(seg.path);
        }
        for seg in std::mem::take(&mut runtime.mic_audio_segments) {
            cleanup_paths.insert(seg.path);
        }
        for seg in std::mem::take(&mut runtime.window_video_segments) {
            cleanup_paths.insert(seg.path);
        }
        (
            process,
            wgc_stop_flag,
            wgc_thread,
            system_audio_stop_flag,
            system_audio_threads,
            mic_audio_stop_flag,
            mic_audio_thread,
            cleanup_paths,
        )
    };

    if let Some(process) = process.as_mut() {
        if let Err(e) = process.kill() {
            log::warn!("取消录制时终止 FFmpeg 进程失败: {}", e);
        }
        // 等待进程退出，最多 10 秒
        let mut exited = false;
        for _ in 0..1000 {
            if let Ok(Some(_)) = process.try_wait() {
                exited = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !exited {
            log::warn!("取消录制时 FFmpeg 进程未能在 10 秒内退出，强制结束");
            let _ = process.kill();
            let _ = process.wait();
        }
    }
    if let Some(flag) = wgc_stop_flag.as_ref() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(join) = wgc_thread {
        let _ = join_thread_with_timeout(join, "cancel WGC", 500);
    }
    if let Some(flag) = system_audio_stop_flag.as_ref() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    for join in system_audio_threads {
        let _ = join_thread_with_timeout(join, "cancel 系统音频", 500);
    }
    if let Some(flag) = mic_audio_stop_flag.as_ref() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(join) = mic_audio_thread {
        let _ = join_thread_with_timeout(join, "cancel 麦克风音频", 500);
    }
    for path in cleanup_paths {
        let _ = fs::remove_file(path);
    }

    let mut runtime = lock_arc_mutex(&runtime_arc);
    runtime.reset_to_idle();
    emit_recording_state_changed(app, None, runtime.phase.as_str(), 0);
    Ok(())
}

pub fn pause_recording(
    app: &AppHandle,
    state_arc: Arc<Mutex<SharedAppState>>,
) -> Result<(), AppError> {
    let runtime_arc = {
        let state_guard = lock_arc_mutex(&state_arc);
        state_guard.recording_runtime.clone()
    };
    let (
        session_id,
        target_type,
        wgc_thread,
        system_audio_stop_flag,
        system_audio_threads,
        mic_audio_stop_flag,
        mic_audio_thread,
        wgc_soft_pause,
    ) = {
        let mut runtime = lock_arc_mutex(&runtime_arc);
        if runtime.phase != RecordingPhase::Recording {
            return Err(AppError::new(
                ErrorCode::ValidationError,
                "当前状态不允许暂停",
            ));
        }
        // WGC 目标优先走软暂停：编码会话保持存活，仅停送帧；
        // PTS 间隙由捕获回调内的补偿器扣除，成片时间轴不含暂停区间。
        // 结构上消除了分段边界 → 不存在跨段音画累积误差。
        // 会话句柄缺失（异常态）时退回硬停止，恢复时走分段重建+按周期校准兜底。
        let soft_pause = is_wgc_target(&runtime.target_type) && runtime.wgc_pause_flag.is_some();
        if soft_pause {
            if let Some(flag) = runtime.wgc_pause_flag.as_ref() {
                flag.store(true, Ordering::SeqCst);
            }
        } else if is_wgc_target(&runtime.target_type) {
            if let Some(flag) = runtime.wgc_stop_flag.as_ref() {
                flag.store(true, Ordering::SeqCst);
            }
        } else if let Some(mut process) = runtime.process.take() {
            // 🔧 修复暴力的线程挂起，改为优雅地向 FFmpeg 发送退出信号（q）
            if let Some(stdin) = process.stdin.as_mut() {
                let _ = stdin.write_all(b"q\n");
                let _ = stdin.flush();
            }
            // 等待进程退出以完成当前视频片段，加入超时机制防死锁
            let mut exited = false;
            for _ in 0..1000 {
                if let Ok(Some(_)) = process.try_wait() {
                    exited = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if !exited {
                log::warn!("FFmpeg 暂停时未能在 10 秒内退出，强制结束并丢弃当前片段");
                let _ = process.kill();
                let _ = process.wait();
                // 被 kill 的片段未写 moov 不可播放：从列表移除并删除，避免后续 concat 失败（#5）
                if let Some(last) = runtime.window_video_segments.pop() {
                    let _ = fs::remove_file(&last.path);
                }
                runtime.video_segment_started_at = None;
            }
        }
        if let Some(flag) = runtime.recording_pause_flag.as_ref() {
            flag.store(true, Ordering::SeqCst);
        }
        // 暂停会停止音频采集：给活动分段打终点，避免 2s 尾部静音填充与恢复后的分段重叠
        let pause_elapsed_ms = runtime.snapshot().elapsed_ms;
        mark_active_segment_end(&mut runtime.system_audio_segments, pause_elapsed_ms);
        mark_active_segment_end(&mut runtime.mic_audio_segments, pause_elapsed_ms);
        (
            runtime.session_id.clone(),
            runtime.target_type.clone(),
            // 软暂停：线程留在 runtime 中继续存活（不 join）
            if soft_pause { None } else { runtime.wgc_thread.take() },
            runtime.system_audio_stop_flag.take(),
            runtime.system_audio_threads.drain(..).collect::<Vec<_>>(),
            runtime.mic_audio_stop_flag.take(),
            runtime.mic_audio_thread.take(),
            soft_pause,
        )
    };

    if let Some(flag) = system_audio_stop_flag.as_ref() {
        flag.store(true, Ordering::SeqCst);
    }
    for join in system_audio_threads {
        let _ = join_thread_with_timeout(join, "pause 系统音频", 500);
    }
    if let Some(flag) = mic_audio_stop_flag.as_ref() {
        flag.store(true, Ordering::SeqCst);
    }
    if let Some(join) = mic_audio_thread {
        let _ = join_thread_with_timeout(join, "pause 麦克风音频", 500);
    }

    if is_wgc_target(&target_type) {
        if let Some(join) = wgc_thread {
            let mut wgc_exited = false;
            for _ in 0..500 {
                if join.is_finished() {
                    wgc_exited = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            if wgc_exited {
                match join.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        if !is_benign_wgc_stop_error(&e) {
                            let runtime = lock_arc_mutex(&runtime_arc);
                            if let Some(flag) = runtime.recording_pause_flag.as_ref() {
                                flag.store(false, Ordering::SeqCst);
                            }
                            return Err(AppError::new(ErrorCode::SystemError, "暂停窗口录制失败")
                                .with_details(e));
                        }
                    }
                    Err(_) => {
                        let runtime = lock_arc_mutex(&runtime_arc);
                        if let Some(flag) = runtime.recording_pause_flag.as_ref() {
                            flag.store(false, Ordering::SeqCst);
                        }
                        return Err(AppError::new(
                            ErrorCode::SystemError,
                            "暂停窗口录制失败：线程异常退出",
                        ));
                    }
                }
            } else {
                log::warn!("WGC 线程暂停超时 (5.0s)，强制等待退出...");
                let _ = join.join();
            }
        }
    }

    let elapsed_ms = {
        let mut runtime = lock_arc_mutex(&runtime_arc);
        if runtime.phase != RecordingPhase::Recording {
            // 等待线程退出期间 stop/cancel 已插入并完成，放弃置为 Paused，避免状态复活
            if let Some(flag) = runtime.recording_pause_flag.as_ref() {
                flag.store(false, Ordering::SeqCst);
            }
            return Err(AppError::new(
                ErrorCode::ValidationError,
                "录制状态已变化，无法暂停",
            ));
        }
        if wgc_soft_pause {
            // 软暂停：保留 stop_flag（终停/取消仍需）与 pause_flag=true（恢复时清除），
            // 编码会话与线程继续存活——这是"无分段边界"的关键
        } else {
            runtime.wgc_stop_flag = None;
            runtime.wgc_pause_flag = None;
        }
        runtime.system_audio_wav_path = None;
        runtime.system_audio_stream_start_ms = None;
        runtime.mic_audio_wav_path = None;
        runtime.mic_audio_stream_start_ms = None;

        // 🔧 修复：暂停时不再删除音频片段文件，保留元数据用于合并
        // 原代码删除文件导致暂停/恢复后丢失暂停前的音频数据
        // AudioSegment 结构体仅含 PathBuf + 2个u64，内存开销极小

        runtime.phase = RecordingPhase::Paused;
        runtime.paused_at_instant = Some(std::time::Instant::now());
        runtime.snapshot().elapsed_ms
    };
    emit_recording_state_changed(
        app,
        session_id.as_deref(),
        RecordingPhase::Paused.as_str(),
        elapsed_ms,
    );
    Ok(())
}

pub fn resume_recording(
    app: &AppHandle,
    state_arc: Arc<Mutex<SharedAppState>>,
) -> Result<(), AppError> {
    let runtime_arc = {
        let state_guard = lock_arc_mutex(&state_arc);
        state_guard.recording_runtime.clone()
    };
    let (
        wgc_resume_kind,
        wgc_soft_resume,
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
        let runtime = lock_arc_mutex(&runtime_arc);
        if runtime.phase != RecordingPhase::Paused {
            return Err(AppError::new(
                ErrorCode::ValidationError,
                "当前状态不允许恢复",
            ));
        }
        // 只读快照：暂停时长的消费与 pause 标志的清除统一推迟到资源创建成功后的提交块。
        // 原实现提前消费，若随后视频源创建失败（如暂停期间目标窗口被关闭），会留下
        // phase=Paused 但 paused_at_instant=None 的损坏状态——计时器在“已暂停”下继续走，
        // 且重试成功后两次尝试之间的死区间被计入有效时长（成片音画失步、时长虚增）。
        // 若此期间 stop/cancel 插入：stop 收尾时会补记剩余暂停时长（见 stop_recording），
        // 暂停时长同样不会丢失。
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
        // WGC 托管类型（window/wgc_screen/wgc_region）：恢复时优先软恢复——
        // 若编码会话仍存活（软暂停），直接唤醒，不产生新分段/新边界；
        // 会话已死亡（如暂停期间目标窗口被关闭）才走硬重建（新分段+按周期校准兜底）
        let wgc_soft_resume = is_wgc_target(&runtime.target_type)
            && runtime
            .wgc_thread
            .as_ref()
            .map(|t| !t.is_finished())
            .unwrap_or(false)
            && runtime.wgc_pause_flag.is_some();
        let wgc_resume_kind = if is_wgc_target(&runtime.target_type) {
            Some(runtime.target_type.clone())
        } else {
            None
        };
        let target_id = runtime.target_id.clone();
        let next_segment_index = runtime.window_segment_index.saturating_add(1);
        let next_segment_path =
            build_window_segment_path(&output_dir, &session_id_for_audio, next_segment_index);
        let fps = runtime.fps;
        let video_bitrate_kbps = runtime.video_bitrate_kbps;
        let capture_cursor = runtime.capture_cursor;
        log::info!(
            "恢复录制: paused_total_ms={}, elapsed_ms={}",
            runtime.paused_total_ms,
            runtime.snapshot().elapsed_ms
        );
        (
            wgc_resume_kind,
            wgc_soft_resume,
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
    // 校验状态：必须在创建任何资源之前完成，避免线程泄漏
    {
        let runtime = lock_arc_mutex(&runtime_arc);
        if runtime.phase != RecordingPhase::Paused {
            return Err(AppError::new(
                ErrorCode::ValidationError,
                "录制状态已变化，请刷新状态后重试",
            ));
        }
    }

    // 软恢复：会话存活时跳过资源创建（不新建分段/编码器），仅唤醒暂停中的回调
    let window_handle = if wgc_soft_resume {
        None
    } else if let Some(kind) = wgc_resume_kind.as_deref() {
        Some(
            match kind {
                "wgc_screen" | "wgc_region" => {
                    let (mon_index, crop_local) =
                        parse_wgc_monitor_target(kind, &target_id).ok_or_else(|| {
                            AppError::new(
                                ErrorCode::SystemError,
                                "恢复显示器录制失败：目标参数无效",
                            )
                                .with_details(format!("target_type={} target_id={}", kind, target_id))
                        })?;
                    start_monitor_capture_to_mp4(
                        mon_index,
                        crop_local,
                        next_segment_path.clone(),
                        fps,
                        video_bitrate_kbps,
                        capture_cursor,
                        std::time::Instant::now(),
                    )
                        .map_err(|e| {
                            AppError::new(ErrorCode::SystemError, "恢复显示器录制失败").with_details(e)
                        })?
                }
                _ => start_window_capture_to_mp4(
                    target_id.as_str(),
                    next_segment_path.clone(),
                    fps,
                    video_bitrate_kbps,
                    capture_cursor,
                    std::time::Instant::now(),
                    is_force_default_border_enabled(),
                )
                    .map_err(|e| {
                        AppError::new(ErrorCode::SystemError, "恢复窗口录制失败").with_details(e)
                    })?,
            },
        )
    } else {
        None
    };

    let ffmpeg_process = if wgc_soft_resume {
        // 软恢复不创建任何新视频源
        None
    } else if wgc_resume_kind.is_none() {
        let ffmpeg_path = resolve_ffmpeg_path().map_err(|e| {
            AppError::new(ErrorCode::SystemError, "恢复录制失败: 找不到 ffmpeg").with_details(e)
        })?;
        let runtime = lock_arc_mutex(&runtime_arc);
        let target_type = runtime.target_type.clone();
        drop(runtime);

        let (child, stderr) = spawn_ffmpeg_video_segment(
            &ffmpeg_path,
            &target_type,
            &target_id,
            fps,
            capture_cursor,
            video_bitrate_kbps,
            &next_segment_path,
        )?;

        Some((child, stderr))
    } else {
        None
    };

    let mut runtime = lock_arc_mutex(&runtime_arc);
    if runtime.phase != RecordingPhase::Paused {
        // 资源创建期间状态已变化（stop/cancel 已插入并完成）：回滚已创建的资源，避免孤儿进程/线程
        if let Some((mut child, _stderr)) = ffmpeg_process {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(handle) = window_handle {
            handle
                .stop_flag
                .store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = join_thread_with_timeout(handle.join, "resume 回滚 WGC", 500);
        }
        let _ = fs::remove_file(&next_segment_path);
        return Err(AppError::new(
            ErrorCode::ValidationError,
            "录制状态已变化，请刷新状态后重试",
        ));
    }
    // 阶段验证通过后，正式提交：消费暂停时长、清除暂停门控、写入分段元数据。
    // 此前任何失败路径都不会破坏 Paused 状态的一致性。
    let resume_u_start_ms;
    {
        // 消费剩余暂停区间（若 stop/cancel 已插入则走上方回滚分支，不会到达此处）
        if let Some(paused_at) = runtime.paused_at_instant.take() {
            runtime.paused_total_ms = runtime
                .paused_total_ms
                .saturating_add(paused_at.elapsed().as_millis() as u64);
        }
        if let Some(flag) = runtime.recording_pause_flag.as_ref() {
            flag.store(false, Ordering::SeqCst);
        }
        if wgc_soft_resume {
            // 软恢复：唤醒同一 WGC 会话——回调内结束 PTS 补偿窗口，
            // 后续帧时间戳无缝衔接，成片仍只有一条连续时间轴（无新分段/无边界）
            if let Some(flag) = runtime.wgc_pause_flag.as_ref() {
                flag.store(false, Ordering::SeqCst);
            }
        } else {
            // 恢复录制时 FFmpeg 延迟清零：新的视频分段与音频缓存同时启动，无需额外校正
            runtime.ffmpeg_start_delay_ms = 0;
        }
        resume_u_start_ms = runtime.snapshot().elapsed_ms;
    }

    // 如果是非 WGC 录制（gdigrab），记录新分段
    if wgc_resume_kind.is_none() {
        runtime.window_segment_index = next_segment_index;
        runtime.window_video_segments.push(
            crate::features::recording::state::WindowVideoSegment {
                path: next_segment_path.clone(),
                u_start_ms: resume_u_start_ms,
                first_frame_anchor: None,
            },
        );
        runtime.video_segment_started_at = Some(std::time::Instant::now());
        if let Some((child, stderr)) = ffmpeg_process {
            runtime.process = Some(child);
            spawn_stderr_parser(
                app.clone(),
                runtime_arc.clone(),
                session_id_for_audio.clone(),
                stderr,
            );
        }
    }

    if let Some(handle) = window_handle {
        runtime.window_segment_index = next_segment_index;
        runtime.window_video_segments.push(
            crate::features::recording::state::WindowVideoSegment {
                path: next_segment_path,
                u_start_ms: resume_u_start_ms,
                first_frame_anchor: Some(handle.first_frame_elapsed_ms.clone()),
            },
        );
        runtime.video_segment_started_at = Some(std::time::Instant::now());
        runtime.wgc_stop_flag = Some(handle.stop_flag);
        runtime.wgc_pause_flag = Some(handle.pause_flag);
        runtime.wgc_session_closed = Some(handle.session_closed.clone());
        // 恢复录制时替换首帧计数：旧计数属于已停止的上一分段，保留会导致看门狗无法检测新分段无画面
        // （各分段的锚点已随 WindowVideoSegment 元数据保留，停止合并阶段按周期分别校正）
        runtime.wgc_first_frame_elapsed_ms = Some(handle.first_frame_elapsed_ms.clone());
        runtime.wgc_thread = Some(handle.join);
    }
    if should_restore_system_audio && runtime.system_audio_threads.is_empty() {
        if let Err(e) = ensure_system_audio_capture_started(
            app,
            &mut runtime,
            &output_dir,
            &session_id_for_audio,
            false,
        ) {
            log::error!("恢复系统音频捕获失败: {}", e);
        }
    }
    if should_restore_mic_audio && runtime.mic_audio_thread.is_none() {
        if let Err(e) = ensure_mic_capture_started(
            app,
            &mut runtime,
            &output_dir,
            &session_id_for_audio,
            false,
        ) {
            log::error!("恢复麦克风捕获失败: {}", e);
        }
    }
    runtime.phase = RecordingPhase::Recording;
    let snapshot = runtime.snapshot();
    emit_recording_state_changed(
        app,
        runtime.session_id.as_deref(),
        runtime.phase.as_str(),
        snapshot.elapsed_ms,
    );
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
        system_audio_threads,
        mic_audio_stop_flag,
        mic_audio_thread,
        sys_device_changed,
        mic_device_changed,
    ) = {
        let mut runtime = lock_arc_mutex(&runtime_arc);
        if runtime.phase != RecordingPhase::Recording && runtime.phase != RecordingPhase::Paused {
            return Err(AppError::new(
                ErrorCode::ValidationError,
                "当前没有正在进行的录制任务",
            ));
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
        let mut system_audio_threads: Vec<std::thread::JoinHandle<()>> = Vec::new();
        if (sys_device_changed || !should_enable_sys) && !runtime.system_audio_threads.is_empty() {
            // 先给活动分段打终点，避免 2s 尾部静音填充与后续分段的真实内容重叠（声音叠加）
            mark_active_segment_end(&mut runtime.system_audio_segments, elapsed_now_ms);
            system_audio_stop_flag = runtime.system_audio_stop_flag.take();
            system_audio_threads = std::mem::take(&mut runtime.system_audio_threads);
            runtime.system_audio_wav_path = None;
        }
        let mut mic_audio_stop_flag = None;
        let mut mic_audio_thread = None;
        if (mic_device_changed || !should_enable_mic) && runtime.mic_audio_thread.is_some() {
            mark_active_segment_end(&mut runtime.mic_audio_segments, elapsed_now_ms);
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
            system_audio_threads,
            mic_audio_stop_flag,
            mic_audio_thread,
            sys_device_changed,
            mic_device_changed,
        )
    };

    if let Some(flag) = system_audio_stop_flag.as_ref() {
        flag.store(true, Ordering::SeqCst);
    }
    for join in system_audio_threads {
        let _ = join_thread_with_timeout(join, "update_audio 系统音频", 500);
    }
    if let Some(flag) = mic_audio_stop_flag.as_ref() {
        flag.store(true, Ordering::SeqCst);
    }
    if let Some(join) = mic_audio_thread {
        let _ = join_thread_with_timeout(join, "update_audio 麦克风音频", 500);
    }
    let mut runtime = lock_arc_mutex(&runtime_arc);
    if runtime.phase != RecordingPhase::Recording && runtime.phase != RecordingPhase::Paused {
        // 等待旧音频线程退出期间 stop/cancel 已插入并完成，不再重启采集线程
        return Err(AppError::new(
            ErrorCode::ValidationError,
            "录制状态已变化，音频配置未应用",
        ));
    }
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
    // 暂停状态下不启动采集线程：此时 U 时钟已冻结而音频文件按墙钟持续写入（暂停期为静音），
    // 若此刻建分段，其内容会横跨恢复点，合并定位将整体滞后剩余暂停时长。
    // 仅更新 enabled_flag，恢复录制时由 should_restore_* 分支以正确的时间轴拉起。
    let is_paused = runtime.phase == RecordingPhase::Paused;
    if should_enable_sys && runtime.system_audio_threads.is_empty() {
        if is_paused {
            log::info!("暂停期间开启系统音频：将在恢复录制时启动采集");
        } else {
            ensure_system_audio_capture_started(
                app,
                &mut runtime,
                &output_dir,
                &session_id,
                true,
            )
            .map_err(|e| {
                AppError::new(ErrorCode::SystemError, AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))
                    .with_details(e)
            })?;
        }
    }
    if should_enable_mic && runtime.mic_audio_thread.is_none() {
        if is_paused {
            log::info!("暂停期间开启麦克风：将在恢复录制时启动采集");
        } else {
            ensure_mic_capture_started(app, &mut runtime, &output_dir, &session_id, true)
                .map_err(|e| {
                    AppError::new(ErrorCode::SystemError, AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))
                        .with_details(e)
                })?;
        }
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
            format!(
                "已有录制任务在运行（当前状态: {}），请先停止后再执行回归自测",
                current.state
            ),
        ));
    }

    let execute = || -> Result<RecordingRegressionReport, AppError> {
        let mut steps = Vec::new();
        let session = start_recording(
            app,
            state_arc.clone(),
            StartRecordingRequest {
                target_type: Some("display".to_string()),
                // 显式指定主屏：多屏环境下 screen 模式必须给出目标屏
                target_id: Some("mon=0".to_string()),
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

        // 先开启系统声再暂停：确保音频分段覆盖暂停边界——
        // 此前音频在暂停之后才启用，"边界+音频"组合未被回归覆盖，
        // 正是音画失步曾溜过验证的盲区
        update_audio_capture(app, state_arc.clone(), Some(true), None, Some(false), None)?;
        steps.push("enable_system_audio:ok".to_string());

        pause_recording(app, state_arc.clone())?;
        steps.push("pause_recording:ok".to_string());
        thread::sleep(Duration::from_millis(700));

        resume_recording(app, state_arc.clone())?;
        steps.push("resume_recording:ok".to_string());
        thread::sleep(Duration::from_millis(1200));

        // 回归新场景：录制中途多次开关系统音频/麦克风，验证分段打点与尾部裁剪（无声音叠加重叠）
        update_audio_capture(app, state_arc.clone(), Some(false), None, Some(false), None)?;
        steps.push("disable_system_audio:ok".to_string());
        thread::sleep(Duration::from_millis(800));
        update_audio_capture(app, state_arc.clone(), Some(false), None, Some(true), None)?;
        steps.push("enable_mic:ok".to_string());
        thread::sleep(Duration::from_millis(800));
        update_audio_capture(app, state_arc.clone(), Some(true), None, Some(false), None)?;
        steps.push("enable_system_audio_again:ok".to_string());
        thread::sleep(Duration::from_millis(800));
        update_audio_capture(app, state_arc.clone(), Some(true), None, Some(true), None)?;
        steps.push("enable_mic_again:ok".to_string());
        thread::sleep(Duration::from_millis(1000));
        update_audio_capture(app, state_arc.clone(), Some(false), None, Some(false), None)?;
        steps.push("disable_all_audio:ok".to_string());
        thread::sleep(Duration::from_millis(600));

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
            AppError::new(ErrorCode::IoError, "回归验证失败：录制文件不存在")
                .with_details(e.to_string())
        })?;
        if metadata.len() == 0 {
            return Err(AppError::new(
                ErrorCode::ValidationError,
                "回归验证失败：录制文件大小为0",
            ));
        }
        steps.push("verify_output_file:ok".to_string());

        // ✅ A/V 时长一致性守卫：暂停边界若存在累积失步，音/视频轨时长会发散。
        // 端到端层面的断言——此前缺失这条线，正是音画失步曾溜过验证的原因之一。
        match resolve_ffmpeg_path() {
            Ok(ffmpeg_path) => {
                // probe_stream_duration_ms 内部优先用同目录 ffprobe，缺失时 ffmpeg 解码兜底
                {
                    let video_ms =
                        probe_stream_duration_ms(&ffmpeg_path, &output, "v:0");
                    let audio_ms =
                        probe_stream_duration_ms(&ffmpeg_path, &output, "a:0");
                    match (video_ms, audio_ms) {
                        (Some(v), Some(a)) => {
                            let diff = (v as i64 - a as i64).abs();
                            steps.push(format!("av_duration_delta:{diff}ms"));
                            if diff > AV_DURATION_TOLERANCE_MS {
                                return Err(AppError::new(
                                    ErrorCode::ValidationError,
                                    format!(
                                        "回归验证失败：音/视频轨时长偏差 {diff}ms 超过容差 {AV_DURATION_TOLERANCE_MS}ms（video={v}ms audio={a}ms）"
                                    ),
                                ));
                            }
                        }
                        _ => steps.push("av_duration_guard:skipped(no-audio-track)".to_string()),
                    }
                }
            }
            Err(_) => steps.push("av_duration_guard:no-ffmpeg".to_string()),
        }

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
            let _ = cancel_recording(app, state_arc, SessionRequest { session_id: None });
            Err(e)
        }
    }
}

pub fn open_recording_folder(
    app: &AppHandle,
    state_arc: Arc<Mutex<SharedAppState>>,
) -> Result<(), AppError> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = state_arc;
        return Err(AppError::new(
            ErrorCode::SystemError,
            "当前平台暂不支持打开录制目录",
        ));
    }
    #[cfg(target_os = "windows")]
    {
        let now_ms = now_unix_ms() as u64;
        let mut last_ms = LAST_OPEN_FOLDER_MS.load(Ordering::Relaxed);
        loop {
            if last_ms > 0 && now_ms.saturating_sub(last_ms) < 1200 {
                return Ok(());
            }
            match LAST_OPEN_FOLDER_MS.compare_exchange(
                last_ms,
                now_ms,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current) => last_ms = current,
            }
        }
        let output_dir = {
            let state_guard = lock_arc_mutex(&state_arc);
            resolve_output_dir(&state_guard, None)?
        };
        fs::create_dir_all(&output_dir).map_err(|e| {
            AppError::new(ErrorCode::IoError, "创建录制目录失败").with_details(e.to_string())
        })?;
        let output_dir_string = output_dir.to_string_lossy().to_string();
        app.opener()
            .open_path(output_dir_string, None::<&str>)
            .map_err(|e| {
                AppError::new(ErrorCode::SystemError, "打开录制目录失败")
                    .with_details(e.to_string())
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod calibration_tests {
    use super::*;
    use crate::features::recording::state::{AudioSegment, WindowVideoSegment};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicU64;

    fn meta(path: &str, u_start: u64, anchor_val: Option<u64>) -> WindowVideoSegment {
        WindowVideoSegment {
            path: PathBuf::from(path),
            u_start_ms: u_start,
            first_frame_anchor: anchor_val.map(|v| Arc::new(AtomicU64::new(v))),
        }
    }

    fn audio(path: &str, start: u64, trim: u64, end: Option<u64>) -> AudioSegment {
        AudioSegment {
            path: PathBuf::from(path),
            start_ms: start,
            trim_start_ms: trim,
            end_ms: end,
        }
    }

    #[test]
    fn single_segment_matches_legacy_global_shift() {
        // 单分段窗口录制：行为与旧全局锚点校正完全一致
        let metas = vec![meta("s0", 0, Some(120))];
        let shifts = compute_window_segment_shifts(&metas, &[None], 130, 10_000);
        assert_eq!(shifts, vec![130]);
    }

    #[test]
    fn two_segments_per_cycle_alignment() {
        // 暂停/恢复产生两段：S0=0/A1=100；S1=5000/A2 原始 150（停止时实测校准为 160，含安全裕量）；总长 9000
        let metas = vec![meta("s0", 0, Some(100)), meta("s1", 5000, Some(150))];
        let shifts = compute_window_segment_shifts(&metas, &[None, None], 160, 9_000);
        // D0=(5000-0)-100=4900 → δ0=0+100-0
        // B1=4900 → δ1=5000+160-4900（末段以实时校准值为准）
        assert_eq!(shifts, vec![100, 260]);

        let mut segments = vec![
            audio("a0", 1000, 0, None),
            audio("a1", 5200, 0, Some(8000)),
        ];
        apply_window_cycle_shifts(&mut segments, &metas, &shifts);
        // 第 0 周期：与全局校正一致
        assert_eq!(segments[0].start_ms, 900);
        // 第 1 周期按自身锚点校正：5200-260=4940；视频第 1 段内容起始于拼接位置 B1=4900，
        // 对应真实 U 起点 5000+150=5150 → 旧实现错用 δ0=130 会得到 5070，偏差 170ms；
        // 新实现 4940 与视频内容位置一致（差值即安全裕量 10ms，与单段行为相同）
        assert_eq!(segments[1].start_ms, 4940);
        assert_eq!(segments[1].end_ms, Some(7740));
    }

    #[test]
    fn position_before_delta_becomes_trim() {
        let metas = vec![meta("s0", 0, Some(300))];
        let shifts = compute_window_segment_shifts(&metas, &[None], 300, 5_000);
        let mut segments = vec![audio("a", 100, 40, Some(2000))];
        apply_window_cycle_shifts(&mut segments, &metas, &shifts);
        assert_eq!(segments[0].start_ms, 0);
        assert_eq!(segments[0].trim_start_ms, 240); // 40 + (300-100)
        assert_eq!(segments[0].end_ms, Some(1700));
    }

    #[test]
    fn never_frame_anchor_treated_as_zero() {
        // 首段从未出帧（u64::MAX，如窗口最小化）按 0 处理，避免负时长导致偏移错乱
        let metas = vec![
            meta("s0", 0, Some(u64::MAX)),
            meta("s1", 3000, Some(80)),
        ];
        let shifts = compute_window_segment_shifts(&metas, &[None, None], 90, 6_000);
        // k0: A=0 → D0=3000, δ0=0；k1(末段): A=90 → D1=2910, δ1=3000+90-3000
        assert_eq!(shifts, vec![0, 90]);
    }

    #[test]
    fn measured_durations_eliminate_pause_tail_drift() {
        // 三段录制（两次暂停）：停止轮询/编码器收尾让每段实际比模型多出 ε。
        // 容器实测时长包含 ε；若仍用模型推算（None 回退），ε 会在拼接位置逐段累积。
        let metas = vec![
            meta("s0", 0, Some(100)),
            meta("s1", 5000, Some(150)),
            meta("s2", 9000, Some(120)),
        ];
        // 模型值：D0=4900、D1=3850；实测：D0 含 +80ms 尾差、D1 含 +70ms、D2 探测失败回退模型
        let measured = vec![Some(4980u64), Some(3920u64), None];
        let shifts =
            compute_window_segment_shifts(&metas, &measured, 130, 12_000);
        // δ0=0+100-0=100
        // B1=实测D0=4980 → δ1=5000+150-4980=170
        // B2=4980+实测D1=8900 → δ2=9000+130(末段实时校准)-8900=230
        assert_eq!(shifts, vec![100, 170, 230]);

        // 对齐验证：第 1 周期内容真实 U 起点 = S1+A1 = 5150，
        // 校正后 5150-170 = 4980 == 实测 B1 ✓（模型推算会得到 5070，滞后 90ms）
        let mut segments = vec![audio("a1", 5150, 0, None)];
        apply_window_cycle_shifts(&mut segments, &metas, &shifts);
        assert_eq!(segments[0].start_ms, 4980);

        // 全部探测失败时回退模型推算（旧行为）
        let fallback = compute_window_segment_shifts(
            &metas,
            &[None, None, None],
            130,
            12_000,
        );
        assert_eq!(fallback[1], 5000 + 150 - 4900);
    }

    #[test]
    fn probe_duration_parser_handles_duration_line() {
        let stderr = "Input #0, mp4, from 'x.mp4':\n  Duration: 00:00:01.96, start: 0.000000, bitrate: 1234 kb/s\n";
        assert_eq!(parse_duration_from_ffmpeg_stderr(stderr), Some(1960));
        let stderr_nofrac =
            "Input #0:\n  Duration: 00:01:02, start: 0.000000\n";
        assert_eq!(parse_duration_from_ffmpeg_stderr(stderr_nofrac), Some(62_000));
        let stderr2 = "Input #0:\n  Duration: N/A, start: -0.5\n";
        assert_eq!(parse_duration_from_ffmpeg_stderr(stderr2), None);
        let stderr3 = "At least one output file must be specified";
        assert_eq!(parse_duration_from_ffmpeg_stderr(stderr3), None);
    }

    #[test]
    fn parse_wgc_monitor_target_formats() {
        assert_eq!(
            parse_wgc_monitor_target("wgc_screen", "mon=0"),
            Some((0, None))
        );
        assert_eq!(
            parse_wgc_monitor_target("wgc_region", "mon=2,crop=100,200,800,600"),
            Some((2, Some((100u32, 200u32, 800u32, 600u32))))
        );
        // 非法形态
        assert_eq!(parse_wgc_monitor_target("window", "mon=0"), None);
        assert_eq!(parse_wgc_monitor_target("wgc_screen", ""), None);
        assert_eq!(parse_wgc_monitor_target("wgc_screen", "mon=x"), None);
        assert_eq!(
            parse_wgc_monitor_target("wgc_region", "mon=0,crop=-1,0,10,10"),
            None // 局部坐标必须非负
        );
        assert_eq!(parse_wgc_monitor_target("wgc_region", "mon=0"), None); // 缺 crop
    }

    #[test]
    fn pick_monitor_selects_max_overlap_and_clamps() {
        let monitors = vec![
            (0usize, 0i32, 0i32, 1920u32, 1080u32),
            (1usize, -1920i32, 0i32, 1920u32, 1080u32),
        ];
        // 区域完全在左侧屏（负坐标）
        let picked = pick_monitor_and_local_rect((-1800, 100, 400, 300), &monitors);
        assert_eq!(picked, Some((1, 120, 100, 400, 300)));
        // 区域跨两屏：取重叠面积更大的右侧屏，并裁剪到其范围内
        let picked = pick_monitor_and_local_rect((1700, 0, 800, 600), &monitors);
        assert!(picked.is_some());
        let (idx, lx, ly, w, h) = picked.unwrap();
        assert_eq!(idx, 0);
        assert_eq!((lx, ly), (1700, 0));
        assert!(lx + w <= 1920 && ly + h <= 1080);
        assert_eq!((w, h), (220, 600)); // 1920-1700=220
        // 区域不与任何显示器相交
        assert_eq!(pick_monitor_and_local_rect((5000, 5000, 10, 10), &monitors), None);
    }

    #[test]
    fn parse_screen_explicit_monitor_variants() {
        assert_eq!(parse_screen_explicit_monitor("mon=1"), Some(1));
        assert_eq!(parse_screen_explicit_monitor("MONITOR=2"), Some(2));
        assert_eq!(parse_screen_explicit_monitor(" mon=0 "), Some(0));
        assert_eq!(parse_screen_explicit_monitor(""), None);
        assert_eq!(parse_screen_explicit_monitor("mon="), None);
        assert_eq!(parse_screen_explicit_monitor("desktop"), None);
    }

    #[test]
    fn parse_blackdetect_lead_variants() {
        // 片头黑段：返回毫秒
        let s = "[blackdetect @ 0x7f] black_start:0 black_end:0.288 black_duration:0.288";
        assert_eq!(parse_black_lead_ms_from_blackdetect(s), Some(288));
        // 黑段不从片头开始 → 无片头黑帧
        let s2 = "[blackdetect @ 0x7f] black_start:1.2 black_end:1.5 black_duration:0.3";
        assert_eq!(parse_black_lead_ms_from_blackdetect(s2), None);
        // 完全无黑帧输出
        assert_eq!(parse_black_lead_ms_from_blackdetect("frame= 12 fps=0.0\n"), None);
        // 极短黑段：解析器忠实返回原始值，是否够裁剪阈值由调用方（MIN_BLACK_LEAD_TRIM_MS）判定
        let s3 = "[blackdetect @ 0x7f] black_start:0 black_end:0.02 black_duration:0.02";
        assert_eq!(parse_black_lead_ms_from_blackdetect(s3), Some(20));
    }

    #[test]
    fn concat_entry_handles_windows_paths_and_quotes() {
        // Windows 反斜杠路径：必须转正斜杠并加单引号，否则被 concat demuxer 吞掉
        let p = PathBuf::from(r"D:\workspace\fuyun_tools\rec.video.0.tmp.mp4");
        assert_eq!(
            build_concat_entry(&p),
            "file 'D:/workspace/fuyun_tools/rec.video.0.tmp.mp4'\n"
        );
        // 字面单引号按 ffmpeg 规则转义
        let q = PathBuf::from("/tmp/it's.mp4");
        assert_eq!(build_concat_entry(&q), "file '/tmp/it'\\''s.mp4'\n");
    }
}
