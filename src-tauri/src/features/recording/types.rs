use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRecordingRequest {
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub target_x: Option<i32>,
    pub target_y: Option<i32>,
    pub target_width: Option<u32>,
    pub target_height: Option<u32>,
    pub capture_cursor: Option<bool>,
    pub capture_system_audio: Option<bool>,
    pub system_audio_device_id: Option<String>,
    pub capture_microphone: Option<bool>,
    pub microphone_device_id: Option<String>,
    pub fps: Option<u32>,
    pub video_bitrate_kbps: Option<u32>,
    pub audio_bitrate_kbps: Option<u32>,
    pub output_dir: Option<String>,
    pub container: Option<String>,
    pub op_id: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRequest {
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSessionInfo {
    pub session_id: String,
    pub started_at_ms: i64,
    pub output_path_tmp: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingRuntimeState {
    pub state: String,
    pub session_id: Option<String>,
    pub elapsed_ms: u64,
    pub dropped_video_frames: u64,
    pub audio_buffer_level_ms: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStopResult {
    pub session_id: String,
    pub output_path: String,
    pub duration_ms: u64,
    pub file_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingInputDeviceList {
    pub microphones: Vec<AudioInputDevice>,
}

// SystemAudioCapability removed in native WASAPI mode
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingRegressionReport {
    pub success: bool,
    pub session_id: Option<String>,
    pub output_path: Option<String>,
    pub duration_ms: u64,
    pub file_size_bytes: u64,
    pub steps: Vec<String>,
    pub message: String,
}
