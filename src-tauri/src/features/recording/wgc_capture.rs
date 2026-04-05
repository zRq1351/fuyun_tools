use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows_capture::capture::{CaptureControlError, Context, GraphicsCaptureApiHandler};
use windows_capture::encoder::{AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings, MinimumUpdateIntervalSettings,
    SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

pub struct WgcCaptureHandle {
    pub stop_flag: Arc<AtomicBool>,
    pub join: JoinHandle<Result<(), String>>,
}

#[derive(Clone)]
struct WgcCaptureFlags {
    stop_flag: Arc<AtomicBool>,
    width: u32,
    height: u32,
    output_path: String,
    fps: u32,
    bitrate_bps: u32,
}

struct WgcCaptureHandler {
    encoder: Option<VideoEncoder>,
    flags: WgcCaptureFlags,
}

impl GraphicsCaptureApiHandler for WgcCaptureHandler {
    type Flags = WgcCaptureFlags;
    type Error = String;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let video_settings = VideoSettingsBuilder::new(ctx.flags.width, ctx.flags.height)
            .frame_rate(ctx.flags.fps)
            .bitrate(ctx.flags.bitrate_bps);
        let encoder = VideoEncoder::new(
            video_settings,
            AudioSettingsBuilder::default().disabled(true),
            ContainerSettingsBuilder::default(),
            &ctx.flags.output_path,
        )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            encoder: Some(encoder),
            flags: ctx.flags,
        })
    }

    fn on_frame_arrived(&mut self, frame: &mut Frame, capture_control: InternalCaptureControl) -> Result<(), Self::Error> {
        if let Some(encoder) = self.encoder.as_mut() {
            encoder.send_frame(frame).map_err(|e| e.to_string())?;
        }
        if self.flags.stop_flag.load(Ordering::SeqCst) {
            capture_control.stop();
        }
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        if let Some(encoder) = self.encoder.take() {
            encoder.finish().map_err(|e| e.to_string())?;
        }
        self.flags.stop_flag.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn parse_window_target(target_id: &str) -> Result<Window, String> {
    let mut raw = target_id.trim().to_string();
    if raw.starts_with("hwnd=") {
        raw = raw.trim_start_matches("hwnd=").to_string();
    }
    if let Some(hex) = raw.strip_prefix("0x") {
        let hwnd = usize::from_str_radix(hex, 16).map_err(|e| format!("解析窗口句柄失败: {}", e))?;
        return Ok(Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void));
    }
    if !raw.is_empty() && raw.chars().all(|c| c.is_ascii_hexdigit()) {
        let hwnd = usize::from_str_radix(&raw, 16).map_err(|e| format!("解析窗口句柄失败: {}", e))?;
        return Ok(Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void));
    }
    Window::from_name(&raw)
        .or_else(|_| Window::from_contains_name(&raw))
        .map_err(|e| format!("定位录制窗口失败: {}", e))
}

pub fn start_window_capture_to_mp4(
    target_id: &str,
    output_path: PathBuf,
    fps: u32,
    video_bitrate_kbps: u32,
    capture_cursor: bool,
) -> Result<WgcCaptureHandle, String> {
    let window = parse_window_target(target_id)?;
    let rect = window.rect().map_err(|e| format!("读取窗口尺寸失败: {}", e))?;
    let width = (rect.right - rect.left).max(1) as u32;
    let height = (rect.bottom - rect.top).max(1) as u32;
    let stop_flag = Arc::new(AtomicBool::new(false));
    let flags = WgcCaptureFlags {
        stop_flag: stop_flag.clone(),
        width,
        height,
        output_path: output_path.to_string_lossy().to_string(),
        fps: fps.max(1),
        bitrate_bps: video_bitrate_kbps.saturating_mul(1000),
    };
    let cursor_setting = if capture_cursor {
        CursorCaptureSettings::WithCursor
    } else {
        CursorCaptureSettings::WithoutCursor
    };
    let stop_flag_for_thread = stop_flag.clone();
    let join = thread::spawn(move || {
        let settings = Settings::new(
            window,
            cursor_setting,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            flags,
        );
        let control = WgcCaptureHandler::start_free_threaded(settings).map_err(|e| format!("{:?}", e))?;
        let mut control_opt = Some(control);
        loop {
            if stop_flag_for_thread.load(Ordering::SeqCst) {
                if let Some(control) = control_opt.take() {
                    return control
                        .stop()
                        .map_err(|e: CaptureControlError<String>| format!("{:?}", e));
                }
                return Ok(());
            }
            if let Some(control) = control_opt.as_ref() {
                if control.is_finished() {
                    let control = control_opt.take().expect("control exists");
                    return control
                        .wait()
                        .map_err(|e: CaptureControlError<String>| format!("{:?}", e));
                }
            } else {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(30));
        }
    });
    Ok(WgcCaptureHandle { stop_flag, join })
}
