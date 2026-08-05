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
        pub phase: String,
        pub region: LongshotRegion,
        pub frame_count: u64,
        pub dropped_frames: u64,
        pub stitched_height: u32,
        pub stitched_width: u32,
        pub last_confidence: f32,
        pub last_error: Option<String>,
        pub failure_kind: Option<String>,
        pub user_message: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ManualLongshotFailureRecord {
        pub failure_kind: String,
        pub message: String,
        pub occurred_at: i64,
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

    pub fn get_last_manual_longshot_failure() -> Option<ManualLongshotFailureRecord> {
        None
    }

    pub fn kill_active_ffmpeg_child() {
        // 未启用 longshot-opencv 时无需清理
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_default_values() {
            assert_eq!(default_longshot_fps(), 10);
            assert_eq!(default_longshot_min_confidence(), 0.82);
            assert_eq!(default_longshot_max_duration_sec(), 90);
            assert_eq!(default_longshot_preview_interval_ms(), 300);
        }

        #[test]
        fn test_request_defaults_applied_on_missing_fields() {
            let json = r#"{"region": {"x": 0, "y": 0, "width": 100, "height": 100}}"#;
            let req: StartManualLongshotRequest = serde_json::from_str(json).unwrap();
            assert_eq!(req.fps, 10);
            assert_eq!(req.min_confidence, 0.82);
            assert_eq!(req.max_duration_sec, 90);
            assert_eq!(req.preview_interval_ms, 300);
            assert_eq!(req.region.width, 100);
        }

        #[test]
        fn test_request_explicit_values_kept() {
            let json = r#"{"region": {"x": 1, "y": 2, "width": 3, "height": 4}, "fps": 30, "minConfidence": 0.5, "maxDurationSec": 60, "previewIntervalMs": 500}"#;
            let req: StartManualLongshotRequest = serde_json::from_str(json).unwrap();
            assert_eq!(req.fps, 30);
            assert_eq!(req.min_confidence, 0.5);
            assert_eq!(req.max_duration_sec, 60);
            assert_eq!(req.preview_interval_ms, 500);
        }

        #[test]
        fn test_opencv_disabled_error_message() {
            assert!(opencv_disabled_error().contains("longshot-opencv"));
        }

        #[test]
        fn test_fallback_functions_return_disabled_error() {
            let request = StartManualLongshotRequest {
                region: LongshotRegion {
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 10,
                },
                fps: 10,
                min_confidence: 0.82,
                max_duration_sec: 90,
                preview_interval_ms: 300,
            };
            // 需要 AppHandle 的函数无法在单测中构造，此处仅测不依赖 AppHandle 的
            assert!(active_manual_longshot_session_id().is_none());
            assert!(get_last_manual_longshot_failure().is_none());
            kill_active_ffmpeg_child(); // 不 panic 即通过
        }

        #[test]
        fn test_status_serialize_camel_case() {
            let status = ManualLongshotStatus {
                session_id: 1,
                state: "running".to_string(),
                phase: "capturing".to_string(),
                region: LongshotRegion {
                    x: 0,
                    y: 0,
                    width: 640,
                    height: 480,
                },
                frame_count: 10,
                dropped_frames: 2,
                stitched_height: 960,
                stitched_width: 640,
                last_confidence: 0.9,
                last_error: None,
                failure_kind: None,
                user_message: "ok".to_string(),
            };
            let v: serde_json::Value = serde_json::to_value(&status).unwrap();
            assert_eq!(v["sessionId"], 1);
            assert_eq!(v["frameCount"], 10);
            assert_eq!(v["droppedFrames"], 2);
            assert_eq!(v["stitchedHeight"], 960);
            // f32 -> f64 序列化存在精度误差，用近似断言
            let conf = v["lastConfidence"].as_f64().unwrap();
            assert!((conf - 0.9).abs() < 0.0001);
            assert_eq!(v["userMessage"], "ok");
        }
    }
}

#[cfg(not(feature = "longshot-opencv"))]
pub use fallback::*;
