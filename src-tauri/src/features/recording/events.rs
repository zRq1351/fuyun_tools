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
    let _ = app.emit("recording-state-changed", payload);
}

pub fn emit_recording_error(app: &AppHandle, session_id: Option<&str>, code: &str, message: &str) {
    let payload = json!({
        "sessionId": session_id,
        "code": code,
        "message": message,
        "recoverable": false
    });
    let _ = app.emit("recording-error", payload);
}

pub fn emit_recording_finished(app: &AppHandle, result: &RecordingStopResult) {
    let payload = json!({
        "sessionId": result.session_id,
        "outputPath": result.output_path,
        "durationMs": result.duration_ms,
        "fileSizeBytes": result.file_size_bytes
    });
    let _ = app.emit("recording-finished", payload);
}

pub fn emit_recording_device_list(
    app: &AppHandle,
    microphones: Vec<AudioInputDevice>,
) {
    let payload = json!({
        "microphones": microphones
    });
    let _ = app.emit("recording-device-list-updated", payload);
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
    let _ = app.emit("recording-stats-updated", payload);
}
