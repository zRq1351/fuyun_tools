//! FFmpeg 子进程操作层：探针、拼接、裁剪、音频合并与校验
//!
//! 从 `recorder_service.rs` 拆出。本模块所有函数都是**无状态**的 ffmpeg 命令行包装，
//! 不接触 `RecordingRuntime` / `AppState`；录制编排（起停/分段/时间线）留在 recorder_service。

use crate::core::error::{AppError, ErrorCode};
use crate::core::perf_metrics::record_perf_metric;
use crate::features::recording::error_codes::{
    AUDIO_DEVICE_LOST, AUDIO_DEVICE_NOT_FOUND, RECORDING_PROCESS_EXITED,
};
use std::fs;
use std::io::Write;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Windows 控制台窗口抑制标志：子进程 ffmpeg 不弹黑窗
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const VIDEO_IO_RETRY_DELAYS_MS: [u64; 5] = [60, 120, 240, 480, 800];

/// 片头黑帧探测窗口（秒）：只解码开头一小段，开销可忽略
const BLACK_LEAD_DETECT_WINDOW_S: f64 = 2.0;

pub(crate) fn suppress_console_window(command: &mut Command) -> &mut Command {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
/// 原子替换 dst 为 src：先把 dst 改名备份，成功后再删除备份。
/// Windows 下 rename 不覆盖目标，直接"先删后 rename"在 rename 失败时会丢失原文件（#2/#3）。
pub(crate) fn replace_file_atomically(
    src: &std::path::Path,
    dst: &std::path::Path,
) -> std::io::Result<()> {
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

/// 用 ffmpeg 读取视频容器的总时长（毫秒）。
/// 不带输出参数调用会让 ffmpeg 在打印输入信息后立即退出——只解析头部，毫秒级开销；
/// 解析失败返回 None（调用方回退模型推算）。
pub(crate) fn probe_video_duration_ms(
    ffmpeg_path: &std::path::Path,
    video_path: &PathBuf,
) -> Option<u64> {
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
pub(crate) fn parse_duration_from_ffmpeg_stderr(stderr: &str) -> Option<u64> {
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
    Some(
        (hours * 3600 + minutes * 60 + seconds)
            .saturating_mul(1000)
            .saturating_add(frac_ms),
    )
}

/// 生成 ffmpeg concat 清单条目：单引号包裹 + 反斜杠转正斜杠。
/// concat demuxer 会把未引用路径中的反斜杠当作转义符吞掉
/// （Windows 下 `D:\a\b` 被读成 `D:ab` 导致拼接失败），必须引用并转换；
/// 引用的单引号字符串内的字面单引号按 ffmpeg 规则写成 '\''。
pub(crate) fn build_concat_entry(path: &std::path::Path) -> String {
    let forward = path.to_string_lossy().replace('\\', "/");
    format!("file '{}'\n", forward.replace('\'', "'\\''"))
}

/// 定位与 ffmpeg 同目录的 ffprobe（流级时长探测的快路径）；不存在返回 None。
/// 注意：精简版发行包可能只带 ffmpeg，此时走 ffmpeg 解码兜底。
pub(crate) fn resolve_ffprobe_path(ffmpeg_path: &std::path::Path) -> Option<PathBuf> {
    let probe = ffmpeg_path.parent()?.join("ffprobe.exe");
    probe.is_file().then_some(probe)
}

/// "HH:MM:SS.frac" → 毫秒
pub(crate) fn parse_hms_to_ms(s: &str) -> Option<u64> {
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
    Some(
        (hours * 3600 + minutes * 60 + seconds)
            .saturating_mul(1000)
            .saturating_add(frac_ms),
    )
}

/// 探测指定流的时长（毫秒）。spec 形如 "v:0" / "a:0"。
/// 快路径用 ffprobe；缺失时用 ffmpeg 把该流解码到 null 并取最后一次进度行的 time=
/// （音频解码极快，视频为 regression 短样本可接受）。
pub(crate) fn probe_stream_duration_ms(
    ffmpeg_path: &std::path::Path,
    media: &PathBuf,
    spec: &str,
) -> Option<u64> {
    if let Some(ffprobe_path) = resolve_ffprobe_path(ffmpeg_path) {
        if let Ok(output) = Command::new(&ffprobe_path)
            .args([
                "-v",
                "error",
                "-select_streams",
                spec,
                "-show_entries",
                "stream=duration",
                "-of",
                "csv=p=0",
            ])
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
    let map = if spec.starts_with('v') {
        "0:v:0"
    } else {
        "0:a:0"
    };
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
            let value = line[pos + "time=".len()..]
                .split_whitespace()
                .next()
                .unwrap_or("");
            if let Some(ms) = parse_hms_to_ms(value) {
                last_ms = Some(ms);
            }
        }
    }
    last_ms
}

pub(crate) fn concat_video_segments(
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
            return Err(AppError::new(ErrorCode::IoError, "写入视频拼接列表失败")
                .with_details(e.to_string()));
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
    let output = output_result.map_err(|e| {
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
pub(crate) fn parse_black_lead_ms_from_blackdetect(stderr: &str) -> Option<u64> {
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
pub(crate) fn detect_black_lead_ms(
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
pub(crate) fn trim_video_initial_frames(
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
    log::debug!("✅ 已裁剪视频开头 {:.3}s 灰色帧", trim_seconds);
    Ok(())
}

/// 纯音频多片段合并：将多个音频片段（含 adelay）合并为单个 AAC，不涉及视频
/// 用于替代 filter_complex 慢速路径中的视频参与步骤
pub(crate) fn make_silent_aac(
    ffmpeg_path: &std::path::Path,
    output_path: &std::path::Path,
    duration_ms: u64,
) -> Result<bool, String> {
    let mut cmd = Command::new(ffmpeg_path);
    suppress_console_window(&mut cmd);
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("anullsrc=r=48000:cl=stereo")
        .arg("-t")
        .arg(format!("{:.3}", duration_ms as f64 / 1000.0))
        .arg("-c:a")
        .arg("aac")
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

pub(crate) fn merge_audio_segments_only(
    ffmpeg_path: &std::path::Path,
    segments: &[crate::features::recording::state::AudioSegment],
    output_path: &std::path::Path,
    audio_bitrate_kbps: u32,
) -> Result<(), String> {
    // 过滤掉因剪辑点/校正后有效时长为 0 的分段（如整段落在视频起点之前）
    let segments = segments
        .iter()
        .filter(|s| {
            s.end_ms
                .map(|e| e.saturating_sub(s.start_ms) > 0)
                .unwrap_or(true)
        })
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
        let is_aac = seg
            .path
            .extension()
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
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y");

    let mut labels: Vec<String> = Vec::new();
    let mut filter_parts: Vec<String> = Vec::new();
    for (idx, seg) in segments.iter().enumerate() {
        if seg.trim_start_ms > 0 {
            cmd.arg("-ss").arg(format!(
                "{}.{:03}",
                seg.trim_start_ms / 1000,
                seg.trim_start_ms % 1000
            ));
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
        chain.push_str(&format!(
            ",asetpts=PTS-STARTPTS,adelay={d}|{d}[{l}]",
            d = seg.start_ms,
            l = label
        ));
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

    cmd.arg("-filter_complex")
        .arg(filter_parts.join(";"))
        .arg("-map")
        .arg("[aout]")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg(format!("{}k", audio_bitrate_kbps.max(32)))
        .arg(output_path);

    let output = cmd
        .output()
        .map_err(|e| format!("启动纯音频合并失败: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("纯音频合并失败: {}", stderr));
    }
    Ok(())
}

/// 使用 FFmpeg 验证音频文件是否包含可解码的音频数据
/// 返回 true 表示文件有效，false 表示文件损坏或为空
pub(crate) fn validate_audio_file_with_ffmpeg(
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

pub(crate) fn merge_system_audio_into_video(
    ffmpeg_path: &std::path::Path,
    video_path: &PathBuf,
    system_segments: &[crate::features::recording::state::AudioSegment],
    mic_segments: &[crate::features::recording::state::AudioSegment],
    audio_bitrate_kbps: u32,
) -> Result<(), AppError> {
    let started_at = Instant::now();
    // 同进程内快速重启录制时，固定临时文件名会互踩；加 pid+seq 前缀隔离
    static MERGE_TEMP_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let merge_prefix = format!(
        "merge_{}_{}",
        std::process::id(),
        MERGE_TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let expected_system_count = system_segments.len();
    let expected_mic_count = mic_segments.len();

    if system_segments.len() == 1 && mic_segments.is_empty() {
        let seg = &system_segments[0];
        if seg.start_ms < 100 && seg.trim_start_ms == 0 && seg.end_ms.is_none() && seg.path.exists()
        {
            let is_aac = seg
                .path
                .extension()
                .map(|ext| ext.to_string_lossy().to_lowercase() == "aac")
                .unwrap_or(false);
            // AAC 文件由 FFmpeg pipe 产生，100% 有效，跳过完整解码验证（仅做文件大小检查）
            if is_aac || validate_audio_file_with_ffmpeg(ffmpeg_path, &seg.path) {
                log::debug!(
                    "快速路径：单个系统音频片段(start_ms={}, trim_start_ms={})，使用流复制模式",
                    seg.start_ms,
                    seg.trim_start_ms
                );
                return merge_audio_fast(
                    ffmpeg_path,
                    video_path,
                    &seg.path,
                    false,
                    audio_bitrate_kbps,
                );
            } else {
                log::warn!("快速路径：系统音频文件验证失败，跳过音频合并");
                return Ok(());
            }
        }
    }
    if mic_segments.len() == 1 && system_segments.is_empty() {
        let seg = &mic_segments[0];
        if seg.start_ms < 100 && seg.trim_start_ms == 0 && seg.end_ms.is_none() && seg.path.exists()
        {
            // 麦克风 WAV 文件仍需 FFmpeg 验证（cpal 写入可能因 I/O 中断损坏）
            if validate_audio_file_with_ffmpeg(ffmpeg_path, &seg.path) {
                log::debug!(
                    "快速路径：单个麦克风音频片段(start_ms={}, trim_start_ms={})，使用流复制模式",
                    seg.start_ms,
                    seg.trim_start_ms
                );
                return merge_audio_fast(
                    ffmpeg_path,
                    video_path,
                    &seg.path,
                    false,
                    audio_bitrate_kbps,
                );
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
    let output_dir = video_path
        .parent()
        .ok_or_else(|| AppError::new(ErrorCode::SystemError, "无法获取输出目录"))?;
    if has_system && has_mic {
        log::debug!(
            "🔧 双音源合并: sys={} 段, mic={} 段, 先分别对齐再 amix 混音",
            valid_system.len(),
            valid_mic.len()
        );
        let sys_aligned = output_dir.join(format!("{merge_prefix}.sys_aligned.tmp.aac"));
        let mic_aligned = output_dir.join(format!("{merge_prefix}.mic_aligned.tmp.aac"));
        let mixed = output_dir.join(format!("{merge_prefix}.mixed.tmp.aac"));
        if let Err(e) =
            merge_audio_segments_only(ffmpeg_path, &valid_system, &sys_aligned, audio_bitrate_kbps)
        {
            let _ = fs::remove_file(&sys_aligned);
            let _ = fs::remove_file(&mic_aligned);
            return Err(AppError::new(ErrorCode::SystemError, "系统音频预合并失败").with_details(e));
        }
        if let Err(e) =
            merge_audio_segments_only(ffmpeg_path, &valid_mic, &mic_aligned, audio_bitrate_kbps)
        {
            let _ = fs::remove_file(&sys_aligned);
            let _ = fs::remove_file(&mic_aligned);
            return Err(
                AppError::new(ErrorCode::SystemError, "麦克风音频预合并失败").with_details(e),
            );
        }
        let mix_result = mix_audio_files(
            ffmpeg_path,
            &sys_aligned,
            &mic_aligned,
            &mixed,
            audio_bitrate_kbps,
        );
        let _ = fs::remove_file(&sys_aligned);
        let _ = fs::remove_file(&mic_aligned);
        mix_result?;
        let result = merge_audio_fast(ffmpeg_path, video_path, &mixed, false, audio_bitrate_kbps);
        let _ = fs::remove_file(&mixed);
        return result;
    }
    // 🔧 两步合并：先纯音频合并（快速），再流复制合并视频（快速）
    // 替代原先的 filter_complex 全路径（视频参与滤镜 → 重编码 → 慢）

    let sys_aligned = output_dir.join(format!("{merge_prefix}.sys_aligned.tmp.aac"));
    if has_system {
        let seg_count = valid_system.len();
        log::debug!("🔧 两步合并 Step 1: 预合并 {} 个系统音频片段", seg_count);
        merge_audio_segments_only(ffmpeg_path, &valid_system, &sys_aligned, audio_bitrate_kbps)
            .map_err(|e| {
                AppError::new(ErrorCode::SystemError, "系统音频预合并失败").with_details(e)
            })?;
        log::debug!("🔧 两步合并 Step 2: 系统音频流复制合并到视频");
        merge_audio_fast(
            ffmpeg_path,
            video_path,
            &sys_aligned,
            false,
            audio_bitrate_kbps,
        )?;
        let _ = fs::remove_file(&sys_aligned);
    }

    let mic_aligned = output_dir.join(format!("{merge_prefix}.mic_aligned.tmp.aac"));
    if has_mic {
        let seg_count = valid_mic.len();
        log::debug!("🔧 两步合并 Step 1: 预合并 {} 个麦克风音频片段", seg_count);
        merge_audio_segments_only(ffmpeg_path, &valid_mic, &mic_aligned, audio_bitrate_kbps)
            .map_err(|e| {
                AppError::new(ErrorCode::SystemError, "麦克风音频预合并失败").with_details(e)
            })?;
        log::debug!("🔧 两步合并 Step 2: 麦克风音频流复制合并到视频");
        merge_audio_fast(
            ffmpeg_path,
            video_path,
            &mic_aligned,
            false,
            audio_bitrate_kbps,
        )?;
        let _ = fs::remove_file(&mic_aligned);
    }

    let elapsed_ms = started_at.elapsed().as_millis();
    log::debug!(
        "✅ 音频合并完成（两步法），耗时: {}ms ({:.1}s)",
        elapsed_ms,
        elapsed_ms as f64 / 1000.0
    );
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

pub(crate) fn merge_audio_fast(
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
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("1:a:0")
        .arg("-c:v")
        .arg("copy")
        .arg("-c:a")
        .arg(if is_aac { "copy" } else { "aac" });
    // 流复制模式下 -b:a 无实际作用，仅重编码时传递码率
    if !is_aac {
        cmd.arg("-b:a")
            .arg(format!("{}k", audio_bitrate_kbps.max(32)));
    }
    cmd.arg("-movflags").arg("+faststart").arg(&merged_path);

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
            log::warn!("⚠️ AAC 流复制失败，尝试重编码回退: {}", details);
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
                log::debug!("✅ AAC 重编码回退成功");
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
                log::debug!(
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
                    Some(format!(
                        "流复制失败:{}；重编码回退失败:{}",
                        details, retry_stderr
                    )),
                );
                return Err(AppError::new(ErrorCode::SystemError, "快速音频合并失败")
                    .with_details(format!(
                        "流复制失败:{}；重编码回退失败:{}",
                        details, retry_stderr
                    )));
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
    log::debug!(
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

pub(crate) fn mix_audio_files(
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

#[allow(dead_code)]
pub(crate) fn validate_video_input_for_merge(
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

pub(crate) fn rename_recording_output_with_retry(
    output_tmp: &std::path::Path,
    output_final: &std::path::Path,
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
pub(crate) fn rename_to_final_output(
    output_tmp: &std::path::Path,
    output_final: &std::path::Path,
) -> Result<PathBuf, AppError> {
    let target = if !output_final.exists() {
        output_final.to_path_buf()
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

#[allow(dead_code)]
pub(crate) fn validate_video_input_for_merge_with_retry(
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

pub(crate) fn map_ffmpeg_error(line: &str) -> Option<(&'static str, String)> {
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

pub(crate) fn parse_u64_after(line: &str, marker: &str) -> Option<u64> {
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
pub(crate) fn parse_f64_after(line: &str, marker: &str) -> Option<f64> {
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

pub(crate) fn parse_kbits_after(line: &str, marker: &str) -> Option<u32> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
    #[test]
    fn probe_duration_parser_handles_duration_line() {
        let stderr = "Input #0, mp4, from 'x.mp4':\n  Duration: 00:00:01.96, start: 0.000000, bitrate: 1234 kb/s\n";
        assert_eq!(parse_duration_from_ffmpeg_stderr(stderr), Some(1960));
        let stderr_nofrac = "Input #0:\n  Duration: 00:01:02, start: 0.000000\n";
        assert_eq!(
            parse_duration_from_ffmpeg_stderr(stderr_nofrac),
            Some(62_000)
        );
        let stderr2 = "Input #0:\n  Duration: N/A, start: -0.5\n";
        assert_eq!(parse_duration_from_ffmpeg_stderr(stderr2), None);
        let stderr3 = "At least one output file must be specified";
        assert_eq!(parse_duration_from_ffmpeg_stderr(stderr3), None);
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
        assert_eq!(
            parse_black_lead_ms_from_blackdetect("frame= 12 fps=0.0\n"),
            None
        );
        // 极短黑段：解析器忠实返回原始值，是否够裁剪阈值由调用方（MIN_BLACK_LEAD_TRIM_MS）判定
        let s3 = "[blackdetect @ 0x7f] black_start:0 black_end:0.02 black_duration:0.02";
        assert_eq!(parse_black_lead_ms_from_blackdetect(s3), Some(20));
    }
}
