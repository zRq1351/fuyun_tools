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
    pub system_audio_process_ids: Option<Vec<u32>>,
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
    /// 实际生效的麦克风设备（启动失败回退默认后与用户偏好不同，供前端同步显示）
    pub effective_mic_device_id: Option<String>,
    /// 实际生效的系统声输出设备（同上）
    pub effective_system_audio_device_id: Option<String>,
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
pub struct AudioProcessItem {
    pub pid: u32,
    pub name: String,
}

/// 可选录制显示器（多屏时供用户指定全屏录制的目标屏）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingMonitorItem {
    pub index: u32,
    /// 显示器名称/设备描述
    pub name: String,
    /// 虚拟屏幕坐标（用于前端标注相对位置）
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// 是否主屏（虚拟屏幕原点所在）
    pub is_primary: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_recording_request_deserializes_camel_case() {
        let json = r#"{
            "targetType": "window",
            "targetId": "hwnd1",
            "captureCursor": true,
            "captureSystemAudio": false,
            "fps": 30,
            "videoBitrateKbps": 6000,
            "audioBitrateKbps": 192,
            "systemAudioProcessIds": [1, 2],
            "opId": 99
        }"#;
        let req: StartRecordingRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.target_type.as_deref(), Some("window"));
        assert_eq!(req.target_id.as_deref(), Some("hwnd1"));
        assert_eq!(req.capture_cursor, Some(true));
        assert_eq!(req.capture_system_audio, Some(false));
        assert_eq!(req.fps, Some(30));
        assert_eq!(req.video_bitrate_kbps, Some(6000));
        assert_eq!(req.audio_bitrate_kbps, Some(192));
        assert_eq!(req.system_audio_process_ids, Some(vec![1, 2]));
        assert_eq!(req.op_id, Some(99));
    }

    #[test]
    fn test_start_recording_request_empty_json() {
        let req: StartRecordingRequest = serde_json::from_str("{}").unwrap();
        assert!(req.target_type.is_none());
        assert!(req.fps.is_none());
        assert!(req.op_id.is_none());
    }

    #[test]
    fn test_session_request() {
        let req: SessionRequest = serde_json::from_str(r#"{"sessionId": "abc"}"#).unwrap();
        assert_eq!(req.session_id.as_deref(), Some("abc"));
        let req2: SessionRequest = serde_json::from_str("{}").unwrap();
        assert!(req2.session_id.is_none());
    }

    #[test]
    fn test_recording_session_info_serializes_camel_case() {
        let info = RecordingSessionInfo {
            session_id: "s1".to_string(),
            started_at_ms: 123,
            output_path_tmp: "out.mp4".to_string(),
        };
        let v: serde_json::Value = serde_json::to_value(&info).unwrap();
        assert_eq!(v["sessionId"], "s1");
        assert_eq!(v["startedAtMs"], 123);
        assert_eq!(v["outputPathTmp"], "out.mp4");
    }

    #[test]
    fn test_recording_runtime_state_serializes() {
        let st = RecordingRuntimeState {
            state: "recording".to_string(),
            session_id: Some("s".to_string()),
            elapsed_ms: 42,
            dropped_video_frames: 1,
            audio_buffer_level_ms: 2,
            last_error: Some("err".to_string()),
            effective_mic_device_id: Some("mic-x".to_string()),
            effective_system_audio_device_id: None,
        };
        let v: serde_json::Value = serde_json::to_value(&st).unwrap();
        assert_eq!(v["state"], "recording");
        assert_eq!(v["elapsedMs"], 42);
        assert_eq!(v["droppedVideoFrames"], 1);
        assert_eq!(v["audioBufferLevelMs"], 2);
        assert_eq!(v["lastError"], "err");
        // 生效设备字段：camelCase 序列化
        assert_eq!(v["effectiveMicDeviceId"], "mic-x");
        assert_eq!(
            v["effectiveSystemAudioDeviceId"],
            serde_json::Value::Null
        );

        // None 字段序列化为 null
        let st2 = RecordingRuntimeState {
            last_error: None,
            ..st
        };
        let v2: serde_json::Value = serde_json::to_value(&st2).unwrap();
        assert_eq!(v2["lastError"], serde_json::Value::Null);
    }

    #[test]
    fn test_audio_device_and_process_serde() {
        let dev = AudioInputDevice {
            id: "mic-1".to_string(),
            name: "麦克风".to_string(),
            is_default: true,
        };
        let v: serde_json::Value = serde_json::to_value(&dev).unwrap();
        assert_eq!(v["id"], "mic-1");
        assert_eq!(v["isDefault"], true);

        let proc = AudioProcessItem {
            pid: 1234,
            name: "app.exe".to_string(),
        };
        let v2: serde_json::Value = serde_json::to_value(&proc).unwrap();
        assert_eq!(v2["pid"], 1234);
    }

    #[test]
    fn test_regression_report_serializes() {
        let report = RecordingRegressionReport {
            success: true,
            session_id: Some("s".to_string()),
            output_path: Some("out.mp4".to_string()),
            duration_ms: 1000,
            file_size_bytes: 2048,
            steps: vec!["a".to_string(), "b".to_string()],
            message: "ok".to_string(),
        };
        let v: serde_json::Value = serde_json::to_value(&report).unwrap();
        assert_eq!(v["success"], true);
        assert_eq!(v["sessionId"], "s");
        assert_eq!(v["outputPath"], "out.mp4");
        assert_eq!(v["durationMs"], 1000);
        assert_eq!(v["fileSizeBytes"], 2048);
        assert_eq!(v["steps"].as_array().unwrap().len(), 2);
        assert_eq!(v["message"], "ok");
    }
}
