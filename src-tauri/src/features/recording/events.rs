use crate::features::recording::types::{AudioInputDevice, RecordingStopResult};
use serde_json::json;
use tauri::{AppHandle, Emitter};

pub fn emit_recording_state_changed(
    app: &AppHandle,
    session_id: Option<&str>,
    state: &str,
    elapsed_ms: u64,
) {
    let payload = json!({
        "sessionId": session_id,
        "state": state,
        "elapsedMs": elapsed_ms
    });
    if let Err(e) = app.emit("recording-state-changed", payload) {
        log::warn!("emit_recording_state_changed 失败: {}", e);
    }
}

pub fn emit_recording_error(app: &AppHandle, session_id: Option<&str>, code: &str, message: &str) {
    let payload = json!({
        "sessionId": session_id,
        "code": code,
        "message": message,
        "recoverable": false
    });
    if let Err(e) = app.emit("recording-error", payload) {
        log::warn!("emit_recording_error 失败: {}", e);
    }
}

pub fn emit_recording_finished(app: &AppHandle, result: &RecordingStopResult) {
    let payload = json!({
        "sessionId": result.session_id,
        "outputPath": result.output_path,
        "durationMs": result.duration_ms,
        "fileSizeBytes": result.file_size_bytes
    });
    if let Err(e) = app.emit("recording-finished", payload) {
        log::warn!("emit_recording_finished 失败: {}", e);
    }
}

pub fn emit_recording_device_list(app: &AppHandle, microphones: Vec<AudioInputDevice>) {
    let payload = json!({
        "microphones": microphones
    });
    if let Err(e) = app.emit("recording-device-list-updated", payload) {
        log::warn!("emit_recording_device_list 失败: {}", e);
    }
}

pub fn emit_recording_stats_updated(
    app: &AppHandle,
    session_id: Option<&str>,
    fps: u32,
    video_bitrate_kbps: u32,
    audio_bitrate_kbps: u32,
    dropped_video_frames: u64,
    audio_buffer_level_ms: u32,
) {
    let payload = json!({
        "sessionId": session_id,
        "fps": fps,
        "videoBitrateKbps": video_bitrate_kbps,
        "audioBitrateKbps": audio_bitrate_kbps,
        "droppedVideoFrames": dropped_video_frames,
        "audioBufferLevelMs": audio_buffer_level_ms
    });
    if let Err(e) = app.emit("recording-stats-updated", payload) {
        log::warn!("emit_recording_stats_updated 失败: {}", e);
    }
}

/// 发送音频合并进度事件
pub fn emit_recording_audio_merging(
    app: &AppHandle,
    session_id: Option<&str>,
    status: &str,         // "started", "progress", "completed", "failed"
    progress: Option<u8>, // 0-100
    message: Option<&str>,
) {
    let payload = json!({
        "sessionId": session_id,
        "status": status,
        "progress": progress,
        "message": message
    });
    if let Err(e) = app.emit("recording-audio-merging", payload) {
        log::warn!("emit_recording_audio_merging 失败: {}", e);
    }
}

/// 通知前端"实际生效的音频设备已变化"。
/// 触发场景：用户指定的设备打开失败、后端回退默认设备——
/// 前端据此同步本地选择并提示，消除双侧状态脱节（否则静音切换会重新断言过期 ID）。
pub fn emit_recording_effective_audio_device(
    app: &AppHandle,
    session_id: Option<&str>,
    kind: &str,           // "mic" | "system"
    effective_device_id: Option<&str>, // None = 默认设备
) {
    let payload = json!({
        "sessionId": session_id,
        "kind": kind,
        "effectiveDeviceId": effective_device_id
    });
    if let Err(e) = app.emit("recording-effective-audio-device", payload) {
        log::warn!("emit_recording_effective_audio_device 失败: {}", e);
    }
}
