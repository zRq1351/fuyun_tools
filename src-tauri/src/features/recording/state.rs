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
    pub wgc_stop_flag: Option<Arc<AtomicBool>>,
    pub wgc_pause_flag: Option<Arc<AtomicBool>>,
    pub wgc_first_frame_elapsed_ms: Option<Arc<AtomicU64>>,
    pub wgc_audio_sync_advance_ms: u64,
    pub wgc_thread: Option<JoinHandle<Result<(), String>>>,
    pub recording_pause_flag: Option<Arc<AtomicBool>>,
    pub window_video_segments: Vec<PathBuf>,
    pub window_segment_index: usize,
    pub system_audio_wav_path: Option<PathBuf>,
    pub system_audio_stop_flag: Option<Arc<AtomicBool>>,
    pub system_audio_thread: Option<JoinHandle<()>>,
    pub system_audio_enabled_flag: Option<Arc<AtomicBool>>,
    pub system_audio_device_id: Option<String>,
    pub system_audio_process_ids: Vec<u32>,
    pub system_audio_ever_enabled: bool,
    pub system_audio_stream_start_ms: Option<u64>,
    pub system_audio_segments: Vec<AudioSegment>,
    pub mic_audio_wav_path: Option<PathBuf>,
    pub mic_audio_stop_flag: Option<Arc<AtomicBool>>,
    pub mic_audio_thread: Option<JoinHandle<()>>,
    pub mic_audio_enabled_flag: Option<Arc<AtomicBool>>,
    pub mic_audio_device_id: Option<String>,
    pub mic_audio_ever_enabled: bool,
    pub mic_audio_stream_start_ms: Option<u64>,
    pub mic_audio_segments: Vec<AudioSegment>,
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
            wgc_thread: None,
            recording_pause_flag: None,
            window_video_segments: Vec::new(),
            window_segment_index: 0,
            system_audio_wav_path: None,
            system_audio_stop_flag: None,
            system_audio_thread: None,
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
        self.wgc_thread = None;
        self.recording_pause_flag = None;
        self.window_video_segments.clear();
        self.window_segment_index = 0;
        self.system_audio_wav_path = None;
        self.system_audio_stop_flag = None;
        self.system_audio_thread = None;
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
