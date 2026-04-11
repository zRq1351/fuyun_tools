#[cfg(feature = "longshot-opencv")]
#[path = "longshot_enabled.rs"]
mod enabled;

#[cfg(feature = "longshot-opencv")]
pub use enabled::*;

#[cfg(not(feature = "longshot-opencv"))]
mod fallback {
    use serde::{Deserialize, Serialize};
    use tauri::AppHandle;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct LongshotRegion {
        pub x: i32,
        pub y: i32,
        pub width: u32,
        pub height: u32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct StartManualLongshotRequest {
        pub region: LongshotRegion,
        #[serde(default = "default_longshot_fps")]
        pub fps: u32,
        #[serde(default = "default_longshot_min_confidence")]
        pub min_confidence: f32,
        #[serde(default = "default_longshot_max_duration_sec")]
        pub max_duration_sec: u32,
        #[serde(default = "default_longshot_preview_interval_ms")]
        pub preview_interval_ms: u32,
    }

    fn default_longshot_fps() -> u32 {
        10
    }

    fn default_longshot_min_confidence() -> f32 {
        0.82
    }

    fn default_longshot_max_duration_sec() -> u32 {
        90
    }

    fn default_longshot_preview_interval_ms() -> u32 {
        300
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ManualLongshotStatus {
        pub session_id: u64,
        pub state: String,
        pub region: LongshotRegion,
        pub frame_count: u64,
        pub dropped_frames: u64,
        pub stitched_height: u32,
        pub stitched_width: u32,
        pub last_confidence: f32,
        pub last_error: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ManualLongshotFinishResult {
        pub session_id: u64,
        pub width: u32,
        pub height: u32,
        pub png_base64: String,
    pub image_path: String,
        pub frame_count: u64,
        pub dropped_frames: u64,
    }

    fn opencv_disabled_error() -> String {
        "当前构建未启用 longshot-opencv。请安装 OpenCV 运行环境后，用 `--features longshot-opencv` 重新构建。".to_string()
    }

    pub fn start_manual_longshot(
        _app: AppHandle,
        _request: StartManualLongshotRequest,
    ) -> Result<serde_json::Value, String> {
        Err(opencv_disabled_error())
    }

    pub fn pause_manual_longshot(_session_id: u64, _app: AppHandle) -> Result<(), String> {
        Err(opencv_disabled_error())
    }

    pub fn resume_manual_longshot(_session_id: u64, _app: AppHandle) -> Result<(), String> {
        Err(opencv_disabled_error())
    }

    pub fn cancel_manual_longshot(_session_id: u64, _app: AppHandle) -> Result<(), String> {
        Err(opencv_disabled_error())
    }

    pub fn finish_manual_longshot(
        _session_id: u64,
        _app: AppHandle,
    ) -> Result<ManualLongshotFinishResult, String> {
        Err(opencv_disabled_error())
    }

    pub fn get_manual_longshot_status(_session_id: u64) -> Result<ManualLongshotStatus, String> {
        Err(opencv_disabled_error())
    }

    pub fn active_manual_longshot_session_id() -> Option<u64> {
        None
    }

}

#[cfg(not(feature = "longshot-opencv"))]
pub use fallback::*;
