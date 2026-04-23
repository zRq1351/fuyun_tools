use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[cfg(target_os = "windows")]
use winapi::shared::windef::RECT;
#[cfg(target_os = "windows")]
use winapi::um::winuser::{GetWindowRect, IsIconic, IsWindow, IsWindowVisible};
use windows_capture::capture::{CaptureControlError, Context, GraphicsCaptureApiHandler};
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

static WGC_FORCE_DEFAULT_BORDER: AtomicBool = AtomicBool::new(false);
static WGC_FORCE_DEFAULT_DIRTY_REGION: AtomicBool = AtomicBool::new(false);

fn is_border_config_unsupported(details: &str) -> bool {
    let lower = details.to_lowercase();
    lower.contains("borderconfigunsupported")
        || lower.contains("graphicscaptureapierror(borderconfigunsupported)")
}

fn is_dirty_region_unsupported(details: &str) -> bool {
    let lower = details.to_lowercase();
    lower.contains("dirtyregionunsupported")
        || lower.contains("graphicscaptureapierror(dirtyregionunsupported)")
}

pub fn is_item_convert_failed(details: &str) -> bool {
    let lower = details.to_lowercase();
    lower.contains("itemconvertfailed")
        || lower.contains("graphicscaptureapierror(itemconvertfailed)")
}

fn parse_hwnd_value(raw: &str) -> Option<usize> {
    if let Some(hex) = raw.strip_prefix("0x") {
        return usize::from_str_radix(hex, 16).ok();
    }
    if !raw.is_empty() && raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return usize::from_str_radix(raw, 16).ok();
    }
    None
}

#[cfg(target_os = "windows")]
fn validate_hwnd_target(hwnd: usize) -> Result<(), String> {
    let hwnd = hwnd as winapi::shared::windef::HWND;
    unsafe {
        if IsWindow(hwnd) == 0 {
            return Err("目标窗口句柄已失效或窗口已关闭".to_string());
        }
        if IsWindowVisible(hwnd) == 0 {
            return Err("目标窗口当前不可见，请将窗口切回前台后重试".to_string());
        }
        if IsIconic(hwnd) != 0 {
            return Err("目标窗口已最小化，请恢复窗口后再开始录制".to_string());
        }
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return Err("读取目标窗口尺寸失败，请重新选择窗口后重试".to_string());
        }
        let width = (rect.right - rect.left).max(0);
        let height = (rect.bottom - rect.top).max(0);
        if width < 2 || height < 2 {
            return Err("目标窗口尺寸异常，当前无法进行窗口录制".to_string());
        }
    }
    Ok(())
}

pub fn get_window_rect_from_target(target_id: &str) -> Result<(i32, i32, u32, u32), String> {
    let window = parse_window_target(target_id)?;
    let rect = window
        .rect()
        .map_err(|e| format!("读取窗口尺寸失败: {}", e))?;
    let width = (rect.right - rect.left).max(1) as u32;
    let height = (rect.bottom - rect.top).max(1) as u32;
    Ok((rect.left, rect.top, width, height))
}

pub fn get_window_title_from_target(target_id: &str) -> Result<String, String> {
    let window = parse_window_target(target_id)?;
    window
        .title()
        .map_err(|e| format!("读取窗口标题失败: {}", e))
}

pub fn validate_window_capture_target(target_id: &str) -> Result<(), String> {
    let mut raw = target_id.trim().to_string();
    if raw.starts_with("hwnd=") {
        raw = raw.trim_start_matches("hwnd=").to_string();
    }
    #[cfg(target_os = "windows")]
    if let Some(hwnd) = parse_hwnd_value(&raw) {
        validate_hwnd_target(hwnd)?;
    }
    Ok(())
}

pub fn bootstrap_force_default_border_from_settings(force_default: bool) {
    if force_default {
        WGC_FORCE_DEFAULT_BORDER.store(true, Ordering::Relaxed);
    }
}

pub fn bootstrap_force_default_dirty_region_from_settings(force_default: bool) {
    if force_default {
        WGC_FORCE_DEFAULT_DIRTY_REGION.store(true, Ordering::Relaxed);
    }
}

pub fn is_force_default_border_enabled() -> bool {
    WGC_FORCE_DEFAULT_BORDER.load(Ordering::Relaxed)
}

pub fn is_force_default_dirty_region_enabled() -> bool {
    WGC_FORCE_DEFAULT_DIRTY_REGION.load(Ordering::Relaxed)
}

pub struct WgcCaptureHandle {
    pub stop_flag: Arc<AtomicBool>,
    pub pause_flag: Arc<AtomicBool>,
    pub first_frame_elapsed_ms: Arc<AtomicU64>,
    pub join: JoinHandle<Result<(), String>>,
}

#[derive(Clone)]
struct WgcCaptureFlags {
    stop_flag: Arc<AtomicBool>,
    pause_flag: Arc<AtomicBool>,
    capture_origin_instant: std::time::Instant,
    first_frame_elapsed_ms: Arc<AtomicU64>,
    first_frame_timestamp: Arc<std::sync::atomic::AtomicI64>,
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

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if self.flags.pause_flag.load(Ordering::Relaxed) {
            return Ok(());
        }
        if self.flags.first_frame_elapsed_ms.load(Ordering::Relaxed) == u64::MAX {
            let elapsed_ms = self.flags.capture_origin_instant.elapsed().as_millis() as u64;
            self.flags
                .first_frame_elapsed_ms
                .store(elapsed_ms, Ordering::Relaxed);
        }
        if let Some(encoder) = self.encoder.as_mut() {
            let mut raw_timestamp = frame.timestamp().map_err(|e| e.to_string())?.Duration;
            
            let first_ts = self.flags.first_frame_timestamp.load(Ordering::Relaxed);
            if first_ts == i64::MAX {
                self.flags.first_frame_timestamp.store(raw_timestamp, Ordering::Relaxed);
                raw_timestamp = 0;
            } else {
                raw_timestamp -= first_ts;
            }

            let frame_w = frame.width();
            let frame_h = frame.height();
            let buffer = frame.buffer().map_err(|e| e.to_string())?;
            let mut vec = Vec::new();
            let pixels = buffer.as_nopadding_buffer(&mut vec);

            let target_w = self.flags.width as usize;
            let target_h = self.flags.height as usize;
            let src_w = frame_w as usize;
            let src_h = frame_h as usize;
            let mut resized = vec![0u8; target_w * target_h * 4];

            for y in 0..target_h {
                let src_y = (y * src_h) / target_h;
                let src_row_start = src_y * src_w * 4;
                
                // VideoEncoder::send_frame_buffer 期望的是 bottom-up 的 BGRA 数据
                // 因此需要垂直翻转图像
                let dst_y = target_h - 1 - y;
                let dst_row_start = dst_y * target_w * 4;

                for x in 0..target_w {
                    let src_x = (x * src_w) / target_w;
                    let src_idx = src_row_start + src_x * 4;
                    let dst_idx = dst_row_start + x * 4;

                    resized[dst_idx..dst_idx + 4]
                        .copy_from_slice(&pixels[src_idx..src_idx + 4]);
                }
            }

            encoder
                .send_frame_buffer(&resized, raw_timestamp)
                .map_err(|e| e.to_string())?;
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
    if let Some(hwnd) = parse_hwnd_value(&raw) {
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
    capture_origin_instant: std::time::Instant,
    prefer_default_border: bool,
) -> Result<WgcCaptureHandle, String> {
    let window = parse_window_target(target_id)?;
    let rect = window
        .rect()
        .map_err(|e| format!("读取窗口尺寸失败: {}", e))?;
    let width = (rect.right - rect.left).max(1) as u32;
    let height = (rect.bottom - rect.top).max(1) as u32;
    let target_id = target_id.trim().to_string();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let pause_flag = Arc::new(AtomicBool::new(false));
    let first_frame_elapsed_ms = Arc::new(AtomicU64::new(u64::MAX));
    let first_frame_timestamp = Arc::new(std::sync::atomic::AtomicI64::new(i64::MAX));
    let flags = WgcCaptureFlags {
        stop_flag: stop_flag.clone(),
        pause_flag: pause_flag.clone(),
        capture_origin_instant,
        first_frame_elapsed_ms: first_frame_elapsed_ms.clone(),
        first_frame_timestamp,
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
        let draw_border_setting =
            if prefer_default_border || WGC_FORCE_DEFAULT_BORDER.load(Ordering::Relaxed) {
                DrawBorderSettings::Default
            } else {
                DrawBorderSettings::WithoutBorder
            };
        let mut dirty_region_setting = if WGC_FORCE_DEFAULT_DIRTY_REGION.load(Ordering::Relaxed) {
            DirtyRegionSettings::Default
        } else {
            DirtyRegionSettings::ReportAndRender
        };
        let start_capture = |dirty_region_setting: DirtyRegionSettings| -> Result<_, String> {
            let window = parse_window_target(&target_id)?;
            let settings = Settings::new(
                window,
                cursor_setting,
                draw_border_setting,
                SecondaryWindowSettings::Default,
                MinimumUpdateIntervalSettings::Default,
                dirty_region_setting,
                ColorFormat::Bgra8,
                flags.clone(),
            );
            WgcCaptureHandler::start_free_threaded(settings).map_err(|e| format!("{:?}", e))
        };
        let control = match start_capture(dirty_region_setting) {
            Ok(control) => control,
            Err(details) => {
                if dirty_region_setting != DirtyRegionSettings::Default
                    && is_dirty_region_unsupported(&details)
                {
                    WGC_FORCE_DEFAULT_DIRTY_REGION.store(true, Ordering::Relaxed);
                    log::warn!(
                        "WGC start 命中 DirtyRegionUnsupported，当前会话与后续会话回退 DirtyRegionSettings::Default"
                    );
                    dirty_region_setting = DirtyRegionSettings::Default;
                    start_capture(dirty_region_setting)?
                } else {
                    return Err(details);
                }
            }
        };
        let mut control_opt = Some(control);
        loop {
            if stop_flag_for_thread.load(Ordering::SeqCst) {
                if let Some(control) = control_opt.take() {
                    return match control.stop() {
                        Ok(()) => Ok(()),
                        Err(e) => {
                            let details = format!("{:?}", e);
                            if is_border_config_unsupported(&details) {
                                WGC_FORCE_DEFAULT_BORDER.store(true, Ordering::Relaxed);
                                log::warn!(
                                    "WGC stop 命中 BorderConfigUnsupported，后续会话回退 DrawBorderSettings::Default"
                                );
                                Ok(())
                            } else {
                                Err(details)
                            }
                        }
                    };
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
            thread::sleep(Duration::from_millis(100));
        }
    });
    Ok(WgcCaptureHandle {
        stop_flag,
        pause_flag,
        first_frame_elapsed_ms,
        join,
    })
}
