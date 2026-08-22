use crate::core::error_codes::AppErrorKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::RECT;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic, IsWindow, IsWindowVisible};
use windows_capture::capture::{CaptureControlError, Context, GraphicsCaptureApiHandler};
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    GraphicsCaptureItemType, MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

static WGC_FORCE_DEFAULT_BORDER: AtomicBool = AtomicBool::new(false);
static WGC_FORCE_DEFAULT_DIRTY_REGION: AtomicBool = AtomicBool::new(false);

/// 显示器捕获的裁剪区域（相对显示器左上角的像素坐标，已保证落在显示器内）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WgcCropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

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
    use windows::core::BOOL;
    let hwnd = windows::Win32::Foundation::HWND(hwnd as *mut core::ffi::c_void);
    unsafe {
        if IsWindow(Some(hwnd)) == BOOL(0) {
            return Err(AppErrorKind::RecordingWindowInvalid.to_frontend_json());
        }
        if IsWindowVisible(hwnd) == BOOL(0) {
            return Err(AppErrorKind::RecordingWindowInvisible.to_frontend_json());
        }
        if IsIconic(hwnd) != BOOL(0) {
            return Err(AppErrorKind::RecordingWindowMinimized.to_frontend_json());
        }
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return Err(AppErrorKind::InternalError.to_frontend_json());
        }
        let width = (rect.right - rect.left).max(0);
        let height = (rect.bottom - rect.top).max(0);
        if width < 2 || height < 2 {
            return Err(AppErrorKind::InternalError.to_frontend_json());
        }
    }
    Ok(())
}

pub fn get_window_rect_from_target(target_id: &str) -> Result<(i32, i32, u32, u32), String> {
    let window = parse_window_target(target_id)?;
    let rect = window
        .rect()
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
    let width = (rect.right - rect.left).max(1) as u32;
    let height = (rect.bottom - rect.top).max(1) as u32;
    Ok((rect.left, rect.top, width, height))
}

pub fn get_window_title_from_target(target_id: &str) -> Result<String, String> {
    let window = parse_window_target(target_id)?;
    window
        .title()
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))
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
    // M2 修复：每次录制开始时根据设置重置标志，避免永久降级
    WGC_FORCE_DEFAULT_BORDER.store(force_default, Ordering::Relaxed);
}

pub fn bootstrap_force_default_dirty_region_from_settings(force_default: bool) {
    // M2 修复：每次录制开始时根据设置重置标志，避免永久降级
    WGC_FORCE_DEFAULT_DIRTY_REGION.store(force_default, Ordering::Relaxed);
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
    /// 显示器捕获的裁剪区域；窗口捕获为 None
    crop: Option<WgcCropRect>,
    output_path: String,
    fps: u32,
    bitrate_bps: u32,
}

struct WgcCaptureHandler {
    encoder: Option<VideoEncoder>,
    flags: WgcCaptureFlags,
    /// 缓存的输出帧缓冲区，避免每帧重新分配
    resized_cache: Vec<u8>,
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
        let target_pixels = ctx.flags.width as usize * ctx.flags.height as usize;
        Ok(Self {
            encoder: Some(encoder),
            resized_cache: vec![0u8; target_pixels * 4],
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
                if raw_timestamp < 0 {
                    raw_timestamp = 0;
                }
            }

            let frame_w = frame.width() as usize;
            let frame_h = frame.height() as usize;
            let mut buffer = frame.buffer().map_err(|e| e.to_string())?;

            // 获取帧的原始像素数据和行步长
            // raw_buffer 包含行 padding，计算 stride = total_len / height
            let raw_pixels = buffer.as_raw_buffer();
            let stride = if frame_h > 0 && raw_pixels.len() % frame_h == 0 {
                raw_pixels.len() / frame_h
            } else if frame_h > 0 {
                log::warn!(
                    "WGC 帧缓冲行数不能整除，跳过本帧: len={} h={}",
                    raw_pixels.len(),
                    frame_h
                );
                return Ok(());
            } else {
                frame_w * 4
            };

            // 裁剪视口：把源限定为显示器内的子矩形（区域录制），窗口捕获为全帧
            let (src_w, src_h, src_x_off, src_y_off) = match self.flags.crop {
                Some(c) => (
                    c.width as usize,
                    c.height as usize,
                    c.x as usize,
                    c.y as usize,
                ),
                None => (frame_w, frame_h, 0usize, 0usize),
            };
            let target_w = src_w;
            let target_h = src_h;
            let resized = &mut self.resized_cache;

            // 安全检查：确保源缓冲区足够大（含裁剪偏移后的最后一行）
            let required_src_size = stride.saturating_mul(src_y_off + src_h);
            if required_src_size == 0
                || raw_pixels.len() < required_src_size
                || src_w == 0
                || src_h == 0
            {
                log::warn!("WGC 帧缓冲区大小不足，跳过本帧: raw={} required={} {}x{}",
                    raw_pixels.len(), required_src_size, src_w, src_h);
                return Ok(());
            }

            // 高性能路径：合并 nopadding + flip 为单次 unsafe 遍历
            // 消除 as_nopadding_buffer 的额外全量复制 + bounds checking
            if frame_w == target_w && src_x_off == 0 && src_y_off == 0 {
                // 尺寸匹配：单次遍历完成 nopadding + 垂直翻转
                let row_bytes = target_w * 4;
                // 运行时检查（release 下 debug_assert 无效）：缓冲不足时跳过本帧（#45）
                if resized.len() < target_w * target_h * 4 {
                    log::warn!("WGC resized 缓冲区不足，跳过本帧");
                    return Ok(());
                }
                if raw_pixels.len() < stride * frame_h {
                    log::warn!("WGC 源缓冲区不足，跳过本帧");
                    return Ok(());
                }
                // SAFETY: resized 已预分配为 target_w * target_h * 4，
                // raw_pixels 来自 WGC 帧缓冲区，保证至少有 stride * frame_h 字节
                unsafe {
                    let src_ptr = raw_pixels.as_ptr();
                    let dst_ptr = resized.as_mut_ptr();
                    for y in 0..target_h {
                        let src_offset = y * stride;
                        let dst_offset = (target_h - 1 - y) * row_bytes;
                        std::ptr::copy_nonoverlapping(
                            src_ptr.add(src_offset),
                            dst_ptr.add(dst_offset),
                            row_bytes,
                        );
                    }
                }
            } else {
                // 通用路径：裁剪/缩放 + nopadding + flip（P2-4: 预计算 X 映射表消除内层除法）
                let mut x_lut: Vec<usize> = Vec::with_capacity(target_w);
                for x in 0..target_w {
                    x_lut.push(src_x_off + (x * src_w) / target_w);
                }
                unsafe {
                    let src_ptr = raw_pixels.as_ptr();
                    let dst_ptr = resized.as_mut_ptr();
                    for y in 0..target_h {
                        let src_y = src_y_off + (y * src_h) / target_h;
                        let dst_y = target_h - 1 - y;
                        let src_row_base = src_y * stride;
                        let dst_row_base = dst_y * target_w * 4;
                        for (x, &src_x) in x_lut.iter().enumerate() {
                            let src_offset = src_row_base + src_x * 4;
                            let dst_offset = dst_row_base + x * 4;
                            std::ptr::copy_nonoverlapping(
                                src_ptr.add(src_offset),
                                dst_ptr.add(dst_offset),
                                4,
                            );
                        }
                    }
                }
            }

            encoder
                .send_frame_buffer(resized, raw_timestamp)
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

impl Drop for WgcCaptureHandler {
    fn drop(&mut self) {
        // 兜底：任何路径离开时若 encoder 未 finish，主动收尾容器（#9）
        if let Some(encoder) = self.encoder.take() {
            let _ = encoder.finish();
        }
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
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))
}

/// 共享的 WGC 采集线程：窗口/显示器两种目标仅 Settings 首参不同，
/// 其余（脏区回退、停止轮询、错误分类）完全一致，抽到这里避免双份漂移。
fn spawn_wgc_thread<T, F>(
    flags: WgcCaptureFlags,
    make_settings: F,
) -> JoinHandle<Result<(), String>>
where
    T: TryInto<GraphicsCaptureItemType> + Send + 'static,
    F: Fn(DirtyRegionSettings) -> Result<Settings<WgcCaptureFlags, T>, String> + Send + 'static,
{
    let stop_flag_for_thread = flags.stop_flag.clone();
    thread::spawn(move || {
        let mut dirty_region_setting = if WGC_FORCE_DEFAULT_DIRTY_REGION.load(Ordering::Relaxed) {
            DirtyRegionSettings::Default
        } else {
            DirtyRegionSettings::ReportAndRender
        };
        let start_capture = |dirty_region_setting: DirtyRegionSettings| -> Result<_, String> {
            let settings =
                make_settings(dirty_region_setting)?;
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
                    if let Some(control) = control_opt.take() {
                        return control
                            .wait()
                            .map_err(|e: CaptureControlError<String>| format!("{:?}", e));
                    }
                }
            } else {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
    })
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
        .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?;
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
        crop: None,
        output_path: output_path.to_string_lossy().to_string(),
        fps: fps.max(1),
        bitrate_bps: video_bitrate_kbps.saturating_mul(1000),
    };
    let cursor_setting = if capture_cursor {
        CursorCaptureSettings::WithCursor
    } else {
        CursorCaptureSettings::WithoutCursor
    };
    let join = spawn_wgc_thread(flags.clone(), move |dirty| {
        let window = parse_window_target(&target_id)?;
        Ok(Settings::new(
            window,
            cursor_setting,
            draw_border_setting_for(prefer_default_border),
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            dirty,
            ColorFormat::Bgra8,
            flags.clone(),
        ))
    });
    Ok(WgcCaptureHandle {
        stop_flag,
        pause_flag,
        first_frame_elapsed_ms,
        join,
    })
}

/// 线程内动态读取全局边框回退标志（stop 阶段可能已翻转）
fn draw_border_setting_for(prefer_default: bool) -> DrawBorderSettings {
    if prefer_default || WGC_FORCE_DEFAULT_BORDER.load(Ordering::Relaxed) {
        DrawBorderSettings::Default
    } else {
        DrawBorderSettings::WithoutBorder
    }
}

/// 枚举显示器并返回 (索引, 虚拟屏幕原点x, 原点y, 宽, 高)
#[cfg(target_os = "windows")]
pub fn enumerate_monitors_with_rects() -> Vec<(usize, i32, i32, u32, u32)> {
    let mut out = Vec::new();
    let monitors = match Monitor::enumerate() {
        Ok(v) => v,
        Err(_) => return out,
    };
    for (idx, m) in monitors.into_iter().enumerate() {
        if let Some((x, y)) = monitor_origin(&m) {
            if let (Ok(w), Ok(h)) = (m.width(), m.height()) {
                out.push((idx, x, y, w, h));
            }
        }
    }
    out
}

#[cfg(not(target_os = "windows"))]
pub fn enumerate_monitors_with_rects() -> Vec<(usize, i32, i32, u32, u32)> {
    Vec::new()
}

#[cfg(target_os = "windows")]
pub fn monitor_count() -> usize {
    Monitor::enumerate().map(|v| v.len()).unwrap_or(0)
}

#[cfg(not(target_os = "windows"))]
pub fn monitor_count() -> usize {
    0
}

/// 查询显示器在虚拟屏幕坐标系中的原点
#[cfg(target_os = "windows")]
pub fn monitor_origin(m: &Monitor) -> Option<(i32, i32)> {
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    let ok = unsafe { GetMonitorInfoW(HMONITOR(m.as_raw_hmonitor()), &mut info) };
    if ok.as_bool() {
        Some((info.rcMonitor.left, info.rcMonitor.top))
    } else {
        None
    }
}

/// 从候选显示器中选出与区域重叠面积最大的一块，并把区域裁剪/平移为该显示器的局部坐标。
/// 纯函数便于单测；返回 (显示器序号, 局部 x, 局部 y, 宽, 高)。
pub fn pick_monitor_and_local_rect(
    rect: (i32, i32, u32, u32),
    monitors: &[(usize, i32, i32, u32, u32)],
) -> Option<(usize, u32, u32, u32, u32)> {
    let (rx, ry, rw, rh) = rect;
    let (rx, ry) = (rx as i64, ry as i64);
    let (rw, rh) = (rw.max(1) as i64, rh.max(1) as i64);

    let mut best: Option<(i64, usize)> = None;
    for (idx, mx, my, mw, mh) in monitors.iter().copied() {
        let (mx, my) = (mx as i64, my as i64);
        let (mw, mh) = (mw as i64, mh as i64);
        // 与显示器矩形的交集
        let ix0 = rx.max(mx);
        let iy0 = ry.max(my);
        let ix1 = (rx + rw).min(mx + mw);
        let iy1 = (ry + rh).min(my + mh);
        if ix1 <= ix0 || iy1 <= iy0 {
            continue;
        }
        let area = (ix1 - ix0) * (iy1 - iy0);
        if best.map(|(a, _)| area > a).unwrap_or(true) {
            best = Some((area, idx));
        }
    }
    let (_, idx) = best?;
    let (_, mx, my, mw, mh) = monitors.iter().copied().find(|(i, ..)| *i == idx)?;
    // 区域裁剪到该显示器内并转为局部坐标
    let lx0 = (rx.max(mx as i64) - mx as i64).max(0);
    let ly0 = (ry.max(my as i64) - my as i64).max(0);
    let lx1 = ((rx + rw).min(mx as i64 + mw as i64) - mx as i64).max(0);
    let ly1 = ((ry + rh).min(my as i64 + mh as i64) - my as i64).max(0);
    let w = (lx1 - lx0).max(1).min(mw as i64) as u32;
    let h = (ly1 - ly0).max(1).min(mh as i64) as u32;
    let lx = lx0.min((mw as i64).saturating_sub(w as i64)).max(0) as u32;
    let ly = ly0.min((mh as i64).saturating_sub(h as i64)).max(0) as u32;
    Some((idx, lx, ly, w, h))
}

/// 显示器捕获入口：全屏（无裁剪）或区域（局部裁剪矩形，相对该显示器左上角）。
/// 输出编码尺寸 = 裁剪后区域大小；首帧锚点语义与窗口捕获一致。
pub fn start_monitor_capture_to_mp4(
    monitor_index: usize,
    crop_local: Option<(u32, u32, u32, u32)>,
    output_path: PathBuf,
    fps: u32,
    video_bitrate_kbps: u32,
    capture_cursor: bool,
    capture_origin_instant: std::time::Instant,
) -> Result<WgcCaptureHandle, String> {
    let monitor_size = || -> Result<(u32, u32), String> {
        let monitor = Monitor::from_index(monitor_index)
            .map_err(|e| format!("枚举显示器失败(index={}): {:?}", monitor_index, e))?;
        let w = monitor
            .width()
            .map_err(|e| format!("读取显示器宽度失败: {:?}", e))?;
        let h = monitor
            .height()
            .map_err(|e| format!("读取显示器高度失败: {:?}", e))?;
        Ok((w, h))
    };
    let (mw, mh) = monitor_size()?;
    let crop = match crop_local {
        None => None,
        Some((cx, cy, cw, ch)) => {
            // 裁剪必须落在显示器内：越界部分裁掉，最小 2x2
            let x = cx.min(mw.saturating_sub(2));
            let y = cy.min(mh.saturating_sub(2));
            let w = cw.clamp(2, mw.saturating_sub(x).max(2));
            let h = ch.clamp(2, mh.saturating_sub(y).max(2));
            if w < 2 || h < 2 {
                return Err(format!(
                    "录制区域超出显示器范围(index={}, crop=({},{},{},{}), size={}x{})",
                    monitor_index, cx, cy, cw, ch, mw, mh
                ));
            }
            Some(WgcCropRect {
                x,
                y,
                width: w,
                height: h,
            })
        }
    };
    let width = match crop {
        Some(c) => c.width,
        None => mw.max(2),
    };
    let height = match crop {
        Some(c) => c.height,
        None => mh.max(2),
    };
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
        crop,
        output_path: output_path.to_string_lossy().to_string(),
        fps: fps.max(1),
        bitrate_bps: video_bitrate_kbps.saturating_mul(1000),
    };
    let cursor_setting = if capture_cursor {
        CursorCaptureSettings::WithCursor
    } else {
        CursorCaptureSettings::WithoutCursor
    };
    // 显示器无边框概念，固定走 Default 分支以规避部分驱动的 WithoutBorder 兼容问题
    let join = spawn_wgc_thread(
        flags.clone(),
        move |dirty| {
            let monitor = Monitor::from_index(monitor_index)
                .map_err(|e| format!("枚举显示器失败(index={}): {:?}", monitor_index, e))?;
            Ok(Settings::new(
                monitor,
                cursor_setting,
                draw_border_setting_for(true),
                SecondaryWindowSettings::Default,
                MinimumUpdateIntervalSettings::Default,
                dirty,
                ColorFormat::Bgra8,
                flags.clone(),
            ))
        },
    );
    Ok(WgcCaptureHandle {
        stop_flag,
        pause_flag,
        first_frame_elapsed_ms,
        join,
    })
}
