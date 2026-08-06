use crate::features::recording::types::RecordingRuntimeState;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct AudioSegment {
    pub path: PathBuf,
    pub start_ms: u64,
    pub trim_start_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingPhase {
    Idle,
    Starting,
    Recording,
    Paused,
    Stopping,
    Error,
}

impl RecordingPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            RecordingPhase::Idle => "idle",
            RecordingPhase::Starting => "starting",
            RecordingPhase::Recording => "recording",
            RecordingPhase::Paused => "paused",
            RecordingPhase::Stopping => "stopping",
            RecordingPhase::Error => "error",
        }
    }
}

pub struct RecordingRuntime {
    // --- 录制阶段与基础元信息 ---
    pub phase: RecordingPhase,
    pub session_id: Option<String>,
    pub started_at_ms: i64,
    pub started_instant: Option<Instant>,
    pub paused_at_instant: Option<Instant>,
    pub paused_total_ms: u64,
    pub max_duration_ms: u64,
    pub auto_stop_requested: bool,
    pub fps: u32,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
    pub mic_enabled: bool,
    pub dropped_video_frames: u64,
    pub audio_buffer_level_ms: u32,
    pub last_error: Option<String>,
    pub output_path_tmp: Option<PathBuf>,
    pub output_path_final: Option<PathBuf>,
    pub target_type: String,
    pub target_id: String,
    pub capture_cursor: bool,
    pub process: Option<Child>,

    // --- WGC 窗口捕获 ---
    pub wgc_stop_flag: Option<Arc<AtomicBool>>,
    pub wgc_pause_flag: Option<Arc<AtomicBool>>,
    pub wgc_first_frame_elapsed_ms: Option<Arc<AtomicU64>>,
    pub wgc_audio_sync_advance_ms: u64,
    pub ffmpeg_start_delay_ms: u64,
    pub wgc_thread: Option<JoinHandle<Result<(), String>>>,

    // --- 录制暂停/分段 ---
    pub recording_pause_flag: Option<Arc<AtomicBool>>,
    pub window_video_segments: Vec<PathBuf>,
    pub window_segment_index: usize,
    /// 当前视频分段开始时间，用于看门狗判断分段存在时长（避免恢复后立即误判无画面）
    pub video_segment_started_at: Option<Instant>,

    // --- 系统音频 ---
    pub system_audio_wav_path: Option<PathBuf>,
    pub system_audio_stop_flag: Option<Arc<AtomicBool>>,
    pub system_audio_threads: Vec<JoinHandle<()>>,
    pub system_audio_enabled_flag: Option<Arc<AtomicBool>>,
    pub system_audio_device_id: Option<String>,
    pub system_audio_process_ids: Vec<u32>,
    pub system_audio_ever_enabled: bool,
    pub system_audio_stream_start_ms: Option<u64>,
    pub system_audio_segments: Vec<AudioSegment>,

    // --- 麦克风 ---
    pub mic_audio_wav_path: Option<PathBuf>,
    pub mic_audio_stop_flag: Option<Arc<AtomicBool>>,
    pub mic_audio_thread: Option<JoinHandle<()>>,
    pub mic_audio_enabled_flag: Option<Arc<AtomicBool>>,
    pub mic_audio_device_id: Option<String>,
    pub mic_audio_ever_enabled: bool,
    pub mic_audio_stream_start_ms: Option<u64>,
    pub mic_audio_segments: Vec<AudioSegment>,

    // --- FFmpeg 诊断 ---
    pub ffmpeg_stderr_tail: VecDeque<String>,
}

impl Default for RecordingRuntime {
    fn default() -> Self {
        Self {
            phase: RecordingPhase::Idle,
            session_id: None,
            started_at_ms: 0,
            started_instant: None,
            paused_at_instant: None,
            paused_total_ms: 0,
            max_duration_ms: 0,
            auto_stop_requested: false,
            fps: 0,
            video_bitrate_kbps: 0,
            audio_bitrate_kbps: 0,
            mic_enabled: false,
            dropped_video_frames: 0,
            audio_buffer_level_ms: 0,
            last_error: None,
            output_path_tmp: None,
            output_path_final: None,
            target_type: "screen".to_string(),
            target_id: String::new(),
            capture_cursor: true,
            process: None,
            wgc_stop_flag: None,
            wgc_pause_flag: None,
            wgc_first_frame_elapsed_ms: None,
            wgc_audio_sync_advance_ms: 80,
            ffmpeg_start_delay_ms: 0,
            wgc_thread: None,
            recording_pause_flag: None,
            window_video_segments: Vec::new(),
            window_segment_index: 0,
            video_segment_started_at: None,
            system_audio_wav_path: None,
            system_audio_stop_flag: None,
            system_audio_threads: Vec::new(),
            system_audio_enabled_flag: None,
            system_audio_device_id: None,
            system_audio_process_ids: Vec::new(),
            system_audio_ever_enabled: false,
            system_audio_stream_start_ms: None,
            system_audio_segments: Vec::new(),
            mic_audio_wav_path: None,
            mic_audio_stop_flag: None,
            mic_audio_thread: None,
            mic_audio_enabled_flag: None,
            mic_audio_device_id: None,
            mic_audio_ever_enabled: false,
            mic_audio_stream_start_ms: None,
            mic_audio_segments: Vec::new(),
            ffmpeg_stderr_tail: VecDeque::new(),
        }
    }
}

impl RecordingRuntime {
    pub fn snapshot(&self) -> RecordingRuntimeState {
        let elapsed_ms = match (self.started_instant, self.paused_at_instant) {
            (Some(start), Some(paused_at)) => {
                let total = paused_at.duration_since(start).as_millis() as u64;
                total.saturating_sub(self.paused_total_ms)
            }
            (Some(start), None) => {
                let total = start.elapsed().as_millis() as u64;
                total.saturating_sub(self.paused_total_ms)
            }
            _ => 0,
        };
        RecordingRuntimeState {
            state: self.phase.as_str().to_string(),
            session_id: self.session_id.clone(),
            elapsed_ms,
            dropped_video_frames: self.dropped_video_frames,
            audio_buffer_level_ms: self.audio_buffer_level_ms,
            last_error: self.last_error.clone(),
        }
    }

    pub fn reset_to_idle(&mut self) {
        // Signal stop flags before joining threads to ensure they exit promptly
        if let Some(flag) = self.wgc_stop_flag.as_ref() {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        if let Some(flag) = self.system_audio_stop_flag.as_ref() {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        if let Some(flag) = self.mic_audio_stop_flag.as_ref() {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }

        // Join threads with timeout to prevent resource leaks
        if let Some(join) = self.wgc_thread.take() {
            let mut wgc_exited = false;
            for _ in 0..500 {
                if join.is_finished() {
                    wgc_exited = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if wgc_exited {
                let _ = join.join();
            } else {
                log::warn!("reset_to_idle: WGC 线程超时，放弃等待（线程转为后台运行）");
                // 不 join：JoinHandle drop 后线程自动分离，避免永久阻塞调用方（P1-1）
            }
        }
        for join in self.system_audio_threads.drain(..) {
            let mut exited = false;
            for _ in 0..500 {
                if join.is_finished() {
                    exited = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if exited {
                let _ = join.join();
            } else {
                log::warn!("reset_to_idle: 系统音频线程超时，放弃等待（线程转为后台运行）");
                // 不 join：JoinHandle drop 后线程自动分离（P1-1）
            }
        }
        if let Some(join) = self.mic_audio_thread.take() {
            let mut exited = false;
            for _ in 0..500 {
                if join.is_finished() {
                    exited = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if exited {
                let _ = join.join();
            } else {
                log::warn!("reset_to_idle: 麦克风音频线程超时，放弃等待（线程转为后台运行）");
                // 不 join：JoinHandle drop 后线程自动分离（P1-1）
            }
        }
        self.phase = RecordingPhase::Idle;
        self.session_id = None;
        self.started_at_ms = 0;
        self.started_instant = None;
        self.paused_at_instant = None;
        self.paused_total_ms = 0;
        self.max_duration_ms = 0;
        self.auto_stop_requested = false;
        self.fps = 0;
        self.video_bitrate_kbps = 0;
        self.audio_bitrate_kbps = 0;
        self.mic_enabled = false;
        self.dropped_video_frames = 0;
        self.audio_buffer_level_ms = 0;
        self.last_error = None;
        self.output_path_tmp = None;
        self.output_path_final = None;
        self.target_type = "screen".to_string();
        self.target_id.clear();
        self.capture_cursor = true;
        self.process = None;
        self.wgc_stop_flag = None;
        self.wgc_pause_flag = None;
        self.wgc_first_frame_elapsed_ms = None;
        self.wgc_audio_sync_advance_ms = 80;
        self.ffmpeg_start_delay_ms = 0;
        self.wgc_thread = None;
        self.recording_pause_flag = None;
        self.window_video_segments.clear();
        self.window_segment_index = 0;
        self.video_segment_started_at = None;
        self.system_audio_wav_path = None;
        self.system_audio_stop_flag = None;
        self.system_audio_threads.clear();
        self.system_audio_enabled_flag = None;
        self.system_audio_device_id = None;
        self.system_audio_process_ids.clear();
        self.system_audio_ever_enabled = false;
        self.system_audio_stream_start_ms = None;
        self.system_audio_segments.clear();
        self.mic_audio_wav_path = None;
        self.mic_audio_stop_flag = None;
        self.mic_audio_thread = None;
        self.mic_audio_enabled_flag = None;
        self.mic_audio_device_id = None;
        self.mic_audio_ever_enabled = false;
        self.mic_audio_stream_start_ms = None;
        self.mic_audio_segments.clear();
        self.ffmpeg_stderr_tail.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_as_str_mapping() {
        assert_eq!(RecordingPhase::Idle.as_str(), "idle");
        assert_eq!(RecordingPhase::Starting.as_str(), "starting");
        assert_eq!(RecordingPhase::Recording.as_str(), "recording");
        assert_eq!(RecordingPhase::Paused.as_str(), "paused");
        assert_eq!(RecordingPhase::Stopping.as_str(), "stopping");
        assert_eq!(RecordingPhase::Error.as_str(), "error");
    }

    #[test]
    fn test_phase_equality() {
        assert_eq!(RecordingPhase::Idle, RecordingPhase::Idle);
        assert_ne!(RecordingPhase::Idle, RecordingPhase::Recording);
        assert_eq!(RecordingPhase::Recording, RecordingPhase::Recording);
    }

    #[test]
    fn test_default_runtime_is_idle() {
        let rt = RecordingRuntime::default();
        assert_eq!(rt.phase, RecordingPhase::Idle);
        assert!(rt.session_id.is_none());
        assert_eq!(rt.started_at_ms, 0);
        assert!(rt.started_instant.is_none());
        assert_eq!(rt.paused_total_ms, 0);
        assert_eq!(rt.max_duration_ms, 0);
        assert_eq!(rt.fps, 0);
        assert!(!rt.mic_enabled);
        assert!(rt.last_error.is_none());
        assert!(rt.output_path_tmp.is_none());
        assert!(rt.output_path_final.is_none());
        assert_eq!(rt.target_type, "screen");
        assert!(rt.process.is_none());
        assert!(rt.window_video_segments.is_empty());
        assert!(rt.system_audio_threads.is_empty());
        assert!(rt.mic_audio_segments.is_empty());
    }

    #[test]
    fn test_snapshot_idle() {
        let rt = RecordingRuntime::default();
        let snap = rt.snapshot();
        assert_eq!(snap.state, "idle");
        assert!(snap.session_id.is_none());
        assert_eq!(snap.elapsed_ms, 0);
        assert_eq!(snap.dropped_video_frames, 0);
        assert_eq!(snap.audio_buffer_level_ms, 0);
        assert!(snap.last_error.is_none());
    }

    #[test]
    fn test_snapshot_with_session_fields() {
        let mut rt = RecordingRuntime::default();
        rt.phase = RecordingPhase::Recording;
        rt.session_id = Some("sess-1".to_string());
        rt.started_at_ms = 1000;
        rt.started_instant = Some(Instant::now());
        rt.dropped_video_frames = 7;
        rt.audio_buffer_level_ms = 12;
        rt.last_error = None;

        let snap = rt.snapshot();
        assert_eq!(snap.state, "recording");
        assert_eq!(snap.session_id.as_deref(), Some("sess-1"));
        assert_eq!(snap.dropped_video_frames, 7);
        assert_eq!(snap.audio_buffer_level_ms, 12);
    }

    #[test]
    fn test_snapshot_error_state() {
        let mut rt = RecordingRuntime::default();
        rt.phase = RecordingPhase::Error;
        rt.last_error = Some("录音失败".to_string());
        let snap = rt.snapshot();
        assert_eq!(snap.state, "error");
        assert_eq!(snap.last_error.as_deref(), Some("录音失败"));
    }

    #[test]
    fn test_reset_to_idle_clears_state() {
        let mut rt = RecordingRuntime::default();
        rt.phase = RecordingPhase::Recording;
        rt.session_id = Some("sess-2".to_string());
        rt.started_at_ms = 5000;
        rt.started_instant = Some(Instant::now());
        rt.paused_total_ms = 100;
        rt.max_duration_ms = 300000;
        rt.fps = 30;
        rt.video_bitrate_kbps = 6000;
        rt.audio_bitrate_kbps = 192;
        rt.mic_enabled = true;
        rt.dropped_video_frames = 3;
        rt.last_error = Some("x".to_string());
        rt.output_path_tmp = Some(PathBuf::from("t.mp4"));
        rt.output_path_final = Some(PathBuf::from("f.mp4"));
        rt.target_type = "window".to_string();
        rt.target_id = "hwnd1".to_string();
        rt.window_video_segments = vec![PathBuf::from("seg.mp4")];
        rt.system_audio_process_ids = vec![1, 2];
        rt.ffmpeg_stderr_tail = VecDeque::from(vec!["line".to_string()]);

        rt.reset_to_idle();
        assert_eq!(rt.phase, RecordingPhase::Idle);
        assert!(rt.session_id.is_none());
        assert_eq!(rt.started_at_ms, 0);
        assert!(rt.started_instant.is_none());
        assert_eq!(rt.paused_total_ms, 0);
        assert_eq!(rt.max_duration_ms, 0);
        assert_eq!(rt.fps, 0);
        assert!(!rt.mic_enabled);
        assert_eq!(rt.dropped_video_frames, 0);
        assert!(rt.last_error.is_none());
        assert!(rt.output_path_tmp.is_none());
        assert!(rt.output_path_final.is_none());
        assert_eq!(rt.target_type, "screen");
        assert!(rt.target_id.is_empty());
        assert!(rt.window_video_segments.is_empty());
        assert!(rt.system_audio_process_ids.is_empty());
        assert!(rt.ffmpeg_stderr_tail.is_empty());
    }

    #[test]
    fn test_reset_to_idle_from_idle_is_noop_safe() {
        let mut rt = RecordingRuntime::default();
        rt.reset_to_idle();
        let snap = rt.snapshot();
        assert_eq!(snap.state, "idle");
    }
}
