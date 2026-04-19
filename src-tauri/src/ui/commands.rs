use crate::core::app_state::AppState as SharedAppState;
use crate::core::config::{AIProvider, ProviderConfig};
use crate::core::error::{to_frontend_error_string, AppError, AppResult, ErrorCode};
use crate::core::perf_metrics::{
    get_perf_metrics_snapshot, record_perf_metric, reset_perf_metrics, timed_sync,
};
use crate::features;
use crate::services::ai_client::{AIClient, AIConfig};
use crate::services::clipboard_manager::set_clipboard_listener_enabled;
use crate::services::image_clipboard_manager::{
    emit_image_history_payload, set_image_clipboard_listener_enabled,
};
use crate::sync::Mutex;
use crate::ui::commands_recording::{
    toggle_microphone_from_shortcut, toggle_recording_from_shortcut,
};
use crate::ui::tray_menu::open_settings;
use crate::ui::window_manager::{
    bind_overlay_window_events, focus_overlay_window_by_label, hide_clipboard_window,
    hide_image_clipboard_window, hide_image_preview_window, hide_overlay_window_by_label,
    set_window_position, show_clipboard_window, show_image_clipboard_window,
    show_image_preview_loading_window, show_image_preview_window, show_overlay_window_by_label,
};
use crate::utils::backup_archive::{
    cleanup_dir, create_backup_temp_dir, read_manifest_from_package, write_backup_payload,
    zip_backup_dir,
};
use crate::utils::backup_model::{
    BackupBlobFile, BackupExportPreviewData, BackupExportPreviewResponse, BackupExportRequest,
    BackupExportResultData, BackupExportResultResponse, BackupHistoryItem, BackupImageHistoryFile,
    BackupImageHistoryItem, BackupPackagePreviewData, BackupPackagePreviewRequest,
    BackupPackagePreviewResponse, BackupRestoreOptions, BackupRestoreRequest,
    BackupRestoreResultResponse, BackupSettingsData, DeleteBackupHistoryItemRequest,
    PreparedBackupData, SaveBackupSettingsRequest,
};
use crate::utils::backup_restore::restore_backup_package as execute_restore_backup_package;
use crate::utils::clipboard::ClipboardManager;
use crate::utils::image_clipboard::get_image_persist_queue_metrics_snapshot;
use crate::utils::image_clipboard::{
    is_fast_fill_verify_mode_enabled, set_image_fill_verify_mode, ImageClipboardManager,
    ImageHistoryPageData, ImageHistoryPreviewItem,
};
#[cfg(debug_assertions)]
use crate::utils::utils_helpers::get_dedup_scan_metrics;
use crate::utils::utils_helpers::{
    default_explanation_prompt_template, default_translation_prompt_template,
    load_history_page_data_async, load_settings, save_settings, ClipboardHistoryPageData,
};
use futures_util::StreamExt;
use image::{imageops, Rgba, RgbaImage};
use imageproc::drawing::{
    draw_filled_circle_mut, draw_hollow_ellipse_mut, draw_hollow_rect_mut, draw_text_mut,
};
use imageproc::rect::Rect;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::sync::{Arc, Mutex as StdMutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_positioner::WindowExt;
use tokio::io::AsyncWriteExt;
use xxhash_rust::xxh3::xxh3_64;

static NEXT_SCREENSHOT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_PINNED_IMAGE_WINDOW_ID: AtomicU64 = AtomicU64::new(1);
static SCREENSHOT_LIFECYCLE_BOUND_FOR_BOOT_WINDOW: AtomicBool = AtomicBool::new(false);
static SCREENSHOT_BOOT_IMAGE_PATH: OnceLock<StdMutex<Option<PathBuf>>> = OnceLock::new();
static SCREENSHOT_BOOT_IMAGE_PATHS: OnceLock<StdMutex<HashSet<PathBuf>>> = OnceLock::new();
static RECENT_COPY_PASTE: OnceLock<StdMutex<Option<RecentCopyPaste>>> = OnceLock::new();
static COPY_PASTE_DEDUP_ENABLED: AtomicBool = AtomicBool::new(true);
static COPY_PASTE_DEDUP_WINDOW_MS: AtomicU64 = AtomicU64::new(1200);
static COPY_PASTE_DEDUP_LOG_ENABLED: AtomicBool = AtomicBool::new(true);
static COPY_PASTE_DEDUP_TOTAL_REQUESTS: AtomicU64 = AtomicU64::new(0);
static COPY_PASTE_DEDUP_HIT_COUNT: AtomicU64 = AtomicU64::new(0);
static COPY_PASTE_DEDUP_REQUEST_ID_HIT_COUNT: AtomicU64 = AtomicU64::new(0);
static COPY_PASTE_DEDUP_TEXT_HASH_HIT_COUNT: AtomicU64 = AtomicU64::new(0);
static COPY_PASTE_DEDUP_LOG_COUNT: AtomicU64 = AtomicU64::new(0);
static COPY_PASTE_DEDUP_WINDOW_STATS: OnceLock<StdMutex<DedupWindowStats>> = OnceLock::new();
#[cfg(debug_assertions)]
static VC_RUNTIME_FORCE_MISSING: AtomicBool = AtomicBool::new(false);
static SCREENSHOT_EXPORT_FONT_BYTES: OnceLock<StdMutex<HashMap<String, Arc<Vec<u8>>>>> =
    OnceLock::new();
static AUTO_BACKUP_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static BACKUP_JOB_MUTEX: OnceLock<tauri::async_runtime::Mutex<()>> = OnceLock::new();
static LAST_WRITEBACK_RESULT: OnceLock<StdMutex<Option<WriteBackExecutionResult>>> =
    OnceLock::new();

struct RecentCopyPaste {
    request_id: String,
    text_hash: u64,
    created_at_ms: u64,
}

struct DedupWindowStats {
    window_start_ms: u64,
    requests: u64,
    hits: u64,
    last_hit_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualLongshotAvailability {
    pub status: String,
    pub phase: String,
    pub summary: String,
    pub details: Vec<String>,
    pub session_id: Option<u64>,
    pub recent_failure_kind: Option<String>,
    pub recent_failure_message: Option<String>,
    pub recent_failure_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticAction {
    pub key: String,
    pub label: String,
    pub action_type: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticItem {
    pub key: String,
    pub title: String,
    pub status: String,
    pub summary: String,
    pub details: Vec<String>,
    pub actions: Vec<DiagnosticAction>,
    pub last_checked_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticOverview {
    pub overall_status: String,
    pub error_count: usize,
    pub warning_count: usize,
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticActionResult {
    pub success: bool,
    pub action_key: String,
    pub message: String,
    pub needs_refresh: bool,
    pub should_restart: bool,
    pub navigate_to: Option<String>,
    pub external_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticActionRequest {
    pub action_key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScreenshotExportSelection {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotExportTextItem {
    x: f32,
    y: f32,
    text: String,
    color: String,
    font_size: f32,
    font_family: Option<String>,
    bold: Option<bool>,
    stroke: Option<bool>,
    stroke_color: Option<String>,
    shadow: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotExportShapeItem {
    #[serde(rename = "type")]
    shape_type: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    x1: Option<f32>,
    y1: Option<f32>,
    x2: Option<f32>,
    y2: Option<f32>,
    color: String,
    line_width: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotExportRasterPoint {
    x: f32,
    y: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotExportRasterCommand {
    #[serde(rename = "type")]
    raster_type: String,
    color: String,
    line_width: f32,
    points: Vec<ScreenshotExportRasterPoint>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotExportRequest {
    source_image_path: String,
    output_path: String,
    is_longshot: bool,
    device_pixel_ratio: Option<f32>,
    viewport_width: Option<f32>,
    viewport_height: Option<f32>,
    selection: Option<ScreenshotExportSelection>,
    text_items: Vec<ScreenshotExportTextItem>,
    shape_items: Vec<ScreenshotExportShapeItem>,
    overlay_commands: Vec<ScreenshotExportRasterCommand>,
}

fn calc_text_hash(text: &str) -> u64 {
    xxh3_64(text.as_bytes())
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn screenshot_boot_image_slot() -> &'static StdMutex<Option<PathBuf>> {
    SCREENSHOT_BOOT_IMAGE_PATH.get_or_init(|| StdMutex::new(None))
}

fn screenshot_boot_image_paths() -> &'static StdMutex<HashSet<PathBuf>> {
    SCREENSHOT_BOOT_IMAGE_PATHS.get_or_init(|| StdMutex::new(HashSet::new()))
}

fn replace_screenshot_boot_image_path(next_path: Option<PathBuf>) {
    let mut slot = match screenshot_boot_image_slot().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    *slot = next_path.clone();
    if let Some(path) = next_path {
        if let Ok(mut paths) = screenshot_boot_image_paths().lock() {
            paths.insert(path);
        }
    }
}

fn cleanup_all_screenshot_boot_images() {
    if let Ok(mut slot) = screenshot_boot_image_slot().lock() {
        *slot = None;
    }
    let paths = match screenshot_boot_image_paths().lock() {
        Ok(mut guard) => std::mem::take(&mut *guard),
        Err(_) => return,
    };
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

fn build_screenshot_boot_image_path(session_id: u64) -> Result<PathBuf, String> {
    let mut dir = std::env::current_exe().map_err(|e| format!("获取程序目录失败: {}", e))?;
    dir.pop();
    dir.push("screenshot_boot");
    fs::create_dir_all(&dir).map_err(|e| format!("创建截图启动目录失败: {}", e))?;
    Ok(dir.join(format!("screenshot_boot_{}.png", session_id)))
}

fn write_screenshot_boot_image(
    rgba: &[u8],
    width: u32,
    height: u32,
    session_id: u64,
) -> Result<PathBuf, String> {
    let png_data = crate::features::screenshot::capture::rgba_to_png_bytes(rgba, width, height)?;
    let path = build_screenshot_boot_image_path(session_id)?;
    fs::write(&path, png_data).map_err(|e| format!("写入截图临时文件失败: {}", e))?;
    replace_screenshot_boot_image_path(Some(path.clone()));
    Ok(path)
}

fn screenshot_export_font_cache() -> &'static StdMutex<HashMap<String, Arc<Vec<u8>>>> {
    SCREENSHOT_EXPORT_FONT_BYTES.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn parse_hex_color(value: &str) -> Rgba<u8> {
    let hex = value.trim().trim_start_matches('#');
    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
            Rgba([r, g, b, 255])
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
            let a = u8::from_str_radix(&hex[6..8], 16).unwrap_or(255);
            Rgba([r, g, b, a])
        }
        _ => Rgba([255, 255, 255, 255]),
    }
}

fn pick_font_candidates(font_family: Option<&str>) -> Vec<&'static str> {
    let family = font_family.unwrap_or("Arial").to_ascii_lowercase();
    if family.contains("yahei") {
        vec![
            "msyh.ttc",
            "msyh.ttf",
            "msyhbd.ttc",
            "arial.ttf",
            "segoeui.ttf",
        ]
    } else if family.contains("hei") {
        vec!["simhei.ttf", "msyh.ttc", "arial.ttf"]
    } else if family.contains("song") {
        vec!["simsun.ttc", "arial.ttf"]
    } else if family.contains("consolas") {
        vec!["consola.ttf", "consolab.ttf", "arial.ttf"]
    } else if family.contains("times") {
        vec!["times.ttf", "timesbd.ttf", "arial.ttf"]
    } else {
        vec!["arial.ttf", "segoeui.ttf", "msyh.ttc", "simhei.ttf"]
    }
}

fn load_font_arc(font_family: Option<&str>) -> Result<ab_glyph::FontArc, String> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let fonts_dir = Path::new(&windir).join("Fonts");
    let cache_key = font_family.unwrap_or("Arial").to_string();
    if let Ok(cache) = screenshot_export_font_cache().lock() {
        if let Some(bytes) = cache.get(&cache_key) {
            return ab_glyph::FontArc::try_from_vec((**bytes).clone())
                .map_err(|_| format!("加载字体失败: {}", cache_key));
        }
    }
    for file_name in pick_font_candidates(font_family) {
        let path = fonts_dir.join(file_name);
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(font) = ab_glyph::FontArc::try_from_vec(bytes.clone()) {
                if let Ok(mut cache) = screenshot_export_font_cache().lock() {
                    cache.insert(cache_key.clone(), Arc::new(bytes));
                }
                return Ok(font);
            }
        }
    }
    Err(format!("未找到可用字体: {}", cache_key))
}

fn clamp_crop_rect(
    source: &RgbaImage,
    selection: &ScreenshotExportSelection,
    dpr: f32,
) -> (u32, u32, u32, u32) {
    let max_w = source.width().max(1);
    let max_h = source.height().max(1);
    let x = (selection.x.max(0.0) * dpr).round().max(0.0) as u32;
    let y = (selection.y.max(0.0) * dpr).round().max(0.0) as u32;
    let w = (selection.width.max(1.0) * dpr).round().max(1.0) as u32;
    let h = (selection.height.max(1.0) * dpr).round().max(1.0) as u32;
    let crop_x = x.min(max_w.saturating_sub(1));
    let crop_y = y.min(max_h.saturating_sub(1));
    let crop_w = w.min(max_w.saturating_sub(crop_x)).max(1);
    let crop_h = h.min(max_h.saturating_sub(crop_y)).max(1);
    (crop_x, crop_y, crop_w, crop_h)
}

fn longshot_viewport_fit(
    image_w: u32,
    image_h: u32,
    viewport_w: f32,
    viewport_h: f32,
) -> (f32, f32, f32) {
    let iw = image_w.max(1) as f32;
    let ih = image_h.max(1) as f32;
    let vw = viewport_w.max(1.0);
    let vh = viewport_h.max(1.0);
    let fit = (vw / iw).min(vh / ih).max(0.0001);
    let view_x = (vw - iw * fit) * 0.5;
    let view_y = (vh - ih * fit) * 0.5;
    (fit, view_x, view_y)
}

fn longshot_scene_to_image(x: f32, y: f32, fit: f32, view_x: f32, view_y: f32) -> (f32, f32) {
    ((x - view_x) / fit, (y - view_y) / fit)
}

fn draw_thick_line(
    canvas: &mut RgbaImage,
    from: (f32, f32),
    to: (f32, f32),
    color: Rgba<u8>,
    width: f32,
) {
    let radius = (width.max(1.0) * 0.5).ceil() as i32;
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let distance = dx.abs().max(dy.abs()).max(1.0);
    let steps = distance.ceil() as i32;
    for index in 0..=steps {
        let t = index as f32 / steps as f32;
        let x = from.0 + dx * t;
        let y = from.1 + dy * t;
        draw_filled_circle_mut(canvas, (x.round() as i32, y.round() as i32), radius, color);
    }
}

fn clamp_i32(value: i32, min_value: i32, max_value: i32) -> i32 {
    value.max(min_value).min(max_value)
}

fn apply_mosaic_at_image_point(
    target: &mut RgbaImage,
    source: &RgbaImage,
    image_x: f32,
    image_y: f32,
    stroke_width: f32,
    scale_factor: f32,
    target_offset_x: i32,
    target_offset_y: i32,
) {
    let size = (stroke_width.max(1.0) * 3.0 * scale_factor.max(0.0001))
        .round()
        .max(1.0) as i32;
    let block_size = (6.0 * scale_factor.max(0.0001)).round().max(1.0) as i32;
    let half = size / 2;
    let center_x = image_x.round() as i32;
    let center_y = image_y.round() as i32;
    let src_w = source.width() as i32;
    let src_h = source.height() as i32;
    let dst_w = target.width() as i32;
    let dst_h = target.height() as i32;
    for local_y in 0..size {
        for local_x in 0..size {
            let _ = (local_x, local_y);
        }
    }
    for block_y in (0..size).step_by(block_size as usize) {
        for block_x in (0..size).step_by(block_size as usize) {
            let src_x = clamp_i32(center_x - half + block_x, 0, src_w.saturating_sub(1));
            let src_y = clamp_i32(center_y - half + block_y, 0, src_h.saturating_sub(1));
            let sample = *source.get_pixel(src_x as u32, src_y as u32);
            for by in 0..block_size {
                for bx in 0..block_size {
                    let px = center_x - half + block_x + bx;
                    let py = center_y - half + block_y + by;
                    if px < 0 || py < 0 || px >= src_w || py >= src_h {
                        continue;
                    }
                    let dx = px - target_offset_x;
                    let dy = py - target_offset_y;
                    if dx < 0 || dy < 0 || dx >= dst_w || dy >= dst_h {
                        continue;
                    }
                    target.put_pixel(dx as u32, dy as u32, sample);
                }
            }
        }
    }
}

fn render_normal_raster_commands(
    canvas: &mut RgbaImage,
    source: &RgbaImage,
    request: &ScreenshotExportRequest,
    selection: &ScreenshotExportSelection,
    dpr: f32,
) {
    let crop_x = (selection.x * dpr).round() as i32;
    let crop_y = (selection.y * dpr).round() as i32;
    for command in &request.overlay_commands {
        if command.points.len() < 2 {
            continue;
        }
        match command.raster_type.as_str() {
            "pen" => {
                let color = parse_hex_color(&command.color);
                let width = (command.line_width * dpr).max(1.0);
                for segment in command.points.windows(2) {
                    let from = (
                        (segment[0].x - selection.x) * dpr,
                        (segment[0].y - selection.y) * dpr,
                    );
                    let to = (
                        (segment[1].x - selection.x) * dpr,
                        (segment[1].y - selection.y) * dpr,
                    );
                    draw_thick_line(canvas, from, to, color, width);
                }
            }
            "mosaic" => {
                for point in &command.points {
                    apply_mosaic_at_image_point(
                        canvas,
                        source,
                        point.x * dpr,
                        point.y * dpr,
                        command.line_width,
                        dpr,
                        crop_x,
                        crop_y,
                    );
                }
            }
            _ => {}
        }
    }
}

fn render_longshot_raster_commands(
    canvas: &mut RgbaImage,
    source: &RgbaImage,
    request: &ScreenshotExportRequest,
    fit: f32,
    view_x: f32,
    view_y: f32,
) {
    for command in &request.overlay_commands {
        if command.points.len() < 2 {
            continue;
        }
        match command.raster_type.as_str() {
            "pen" => {
                let color = parse_hex_color(&command.color);
                let width = (command.line_width / fit).max(1.0);
                for segment in command.points.windows(2) {
                    let from =
                        longshot_scene_to_image(segment[0].x, segment[0].y, fit, view_x, view_y);
                    let to =
                        longshot_scene_to_image(segment[1].x, segment[1].y, fit, view_x, view_y);
                    draw_thick_line(canvas, from, to, color, width);
                }
            }
            "mosaic" => {
                let scale_factor = 1.0 / fit.max(0.0001);
                for point in &command.points {
                    let (x, y) = longshot_scene_to_image(point.x, point.y, fit, view_x, view_y);
                    apply_mosaic_at_image_point(
                        canvas,
                        source,
                        x,
                        y,
                        command.line_width,
                        scale_factor,
                        0,
                        0,
                    );
                }
            }
            _ => {}
        }
    }
}

fn draw_rect_shape(
    canvas: &mut RgbaImage,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: Rgba<u8>,
    line_width: f32,
) {
    let left = x.round() as i32;
    let top = y.round() as i32;
    let rect_w = width.max(1.0).round() as u32;
    let rect_h = height.max(1.0).round() as u32;
    for offset in 0..line_width.max(1.0).round() as i32 {
        let inset = offset / 2;
        let w = rect_w.saturating_sub((offset as u32).min(rect_w.saturating_sub(1)));
        let h = rect_h.saturating_sub((offset as u32).min(rect_h.saturating_sub(1)));
        draw_hollow_rect_mut(
            canvas,
            Rect::at(left + inset, top + inset).of_size(w.max(1), h.max(1)),
            color,
        );
    }
}

fn draw_circle_shape(
    canvas: &mut RgbaImage,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: Rgba<u8>,
    line_width: f32,
) {
    let cx = (x + width * 0.5).round() as i32;
    let cy = (y + height * 0.5).round() as i32;
    let rx = (width * 0.5).round().max(1.0) as i32;
    let ry = (height * 0.5).round().max(1.0) as i32;
    let stroke = line_width.max(1.0).round() as i32;
    for offset in 0..stroke {
        let inner_rx = (rx - offset).max(1);
        let inner_ry = (ry - offset).max(1);
        draw_hollow_ellipse_mut(canvas, (cx, cy), inner_rx, inner_ry, color);
    }
}

fn draw_text_item(
    canvas: &mut RgbaImage,
    item: &ScreenshotExportTextItem,
    x: f32,
    y: f32,
    font_size: f32,
) -> Result<(), String> {
    let font = load_font_arc(item.font_family.as_deref())?;
    let scale = ab_glyph::PxScale::from(font_size.max(8.0));
    let color = parse_hex_color(&item.color);
    let shadow = item.shadow.unwrap_or(false);
    let stroke = item.stroke.unwrap_or(false);
    let stroke_color = parse_hex_color(item.stroke_color.as_deref().unwrap_or("#000000"));
    let line_height = (font_size.max(8.0) * 1.25).max(font_size + 4.0);
    for (line_index, line) in item.text.replace("\r", "").split('\n').enumerate() {
        let draw_x = x.round() as i32;
        let draw_y = (y + line_index as f32 * line_height).round() as i32;
        if shadow {
            draw_text_mut(
                canvas,
                Rgba([0, 0, 0, 96]),
                draw_x,
                draw_y + (font_size * 0.1).round() as i32,
                scale,
                &font,
                line,
            );
        }
        if stroke {
            let outline = (font_size / 14.0).round().max(1.0) as i32;
            for ox in -outline..=outline {
                for oy in -outline..=outline {
                    if ox == 0 && oy == 0 {
                        continue;
                    }
                    draw_text_mut(
                        canvas,
                        stroke_color,
                        draw_x + ox,
                        draw_y + oy,
                        scale,
                        &font,
                        line,
                    );
                }
            }
        }
        draw_text_mut(canvas, color, draw_x, draw_y, scale, &font, line);
        if item.bold.unwrap_or(false) {
            draw_text_mut(canvas, color, draw_x + 1, draw_y, scale, &font, line);
        }
    }
    Ok(())
}

fn render_normal_shapes(
    canvas: &mut RgbaImage,
    request: &ScreenshotExportRequest,
    selection: &ScreenshotExportSelection,
    dpr: f32,
) {
    for item in &request.shape_items {
        let color = parse_hex_color(&item.color);
        let x = (item.x - selection.x) * dpr;
        let y = (item.y - selection.y) * dpr;
        let width = item.width.max(1.0) * dpr;
        let height = item.height.max(1.0) * dpr;
        let line_width = item.line_width.max(1.0) * dpr;
        match item.shape_type.as_str() {
            "rect" => draw_rect_shape(canvas, x, y, width, height, color, line_width),
            "circle" => draw_circle_shape(canvas, x, y, width, height, color, line_width),
            "line" | "arrow" => {
                let from = (
                    x + item.x1.unwrap_or(0.0) * dpr,
                    y + item.y1.unwrap_or(0.0) * dpr,
                );
                let to = (
                    x + item.x2.unwrap_or(item.width.max(1.0)) * dpr,
                    y + item.y2.unwrap_or(item.height.max(1.0)) * dpr,
                );
                draw_thick_line(canvas, from, to, color, line_width);
                if item.shape_type == "arrow" {
                    let angle = (to.1 - from.1).atan2(to.0 - from.0);
                    let head = 12.0 * dpr;
                    let left = (
                        to.0 - head * (angle - std::f32::consts::PI / 6.0).cos(),
                        to.1 - head * (angle - std::f32::consts::PI / 6.0).sin(),
                    );
                    let right = (
                        to.0 - head * (angle + std::f32::consts::PI / 6.0).cos(),
                        to.1 - head * (angle + std::f32::consts::PI / 6.0).sin(),
                    );
                    draw_thick_line(canvas, to, left, color, line_width);
                    draw_thick_line(canvas, to, right, color, line_width);
                }
            }
            _ => {}
        }
    }
}

fn render_shape_item_for_longshot(
    canvas: &mut RgbaImage,
    item: &ScreenshotExportShapeItem,
    fit: f32,
    view_x: f32,
    view_y: f32,
) {
    let color = parse_hex_color(&item.color);
    let line_width = (item.line_width / fit).max(1.0);
    match item.shape_type.as_str() {
        "rect" => {
            let (x1, y1) = longshot_scene_to_image(item.x, item.y, fit, view_x, view_y);
            let (x2, y2) = longshot_scene_to_image(
                item.x + item.width,
                item.y + item.height,
                fit,
                view_x,
                view_y,
            );
            draw_rect_shape(canvas, x1, y1, x2 - x1, y2 - y1, color, line_width);
        }
        "circle" => {
            let (x1, y1) = longshot_scene_to_image(item.x, item.y, fit, view_x, view_y);
            let (x2, y2) = longshot_scene_to_image(
                item.x + item.width,
                item.y + item.height,
                fit,
                view_x,
                view_y,
            );
            draw_circle_shape(canvas, x1, y1, x2 - x1, y2 - y1, color, line_width);
        }
        "line" | "arrow" => {
            let (from_x, from_y) = longshot_scene_to_image(
                item.x + item.x1.unwrap_or(0.0),
                item.y + item.y1.unwrap_or(0.0),
                fit,
                view_x,
                view_y,
            );
            let (to_x, to_y) = longshot_scene_to_image(
                item.x + item.x2.unwrap_or(item.width),
                item.y + item.y2.unwrap_or(item.height),
                fit,
                view_x,
                view_y,
            );
            draw_thick_line(canvas, (from_x, from_y), (to_x, to_y), color, line_width);
            if item.shape_type == "arrow" {
                let angle = (to_y - from_y).atan2(to_x - from_x);
                let head = 12.0 / fit.max(0.0001);
                let left = (
                    to_x - head * (angle - std::f32::consts::PI / 6.0).cos(),
                    to_y - head * (angle - std::f32::consts::PI / 6.0).sin(),
                );
                let right = (
                    to_x - head * (angle + std::f32::consts::PI / 6.0).cos(),
                    to_y - head * (angle + std::f32::consts::PI / 6.0).sin(),
                );
                draw_thick_line(canvas, (to_x, to_y), left, color, line_width);
                draw_thick_line(canvas, (to_x, to_y), right, color, line_width);
            }
        }
        _ => {}
    }
}

fn render_screenshot_image(request: &ScreenshotExportRequest) -> Result<RgbaImage, String> {
    if request.source_image_path.trim().is_empty() {
        return Err("缺少源图路径".to_string());
    }
    let source = image::open(&request.source_image_path)
        .map_err(|e| format!("读取源图失败: {}", e))?
        .to_rgba8();
    let mut canvas = if request.is_longshot {
        source.clone()
    } else {
        let selection = request
            .selection
            .as_ref()
            .ok_or_else(|| "缺少裁剪区域".to_string())?;
        let dpr = request.device_pixel_ratio.unwrap_or(1.0).max(0.1);
        let (crop_x, crop_y, crop_w, crop_h) = clamp_crop_rect(&source, selection, dpr);
        imageops::crop_imm(&source, crop_x, crop_y, crop_w, crop_h).to_image()
    };

    if request.is_longshot {
        let viewport_w = request.viewport_width.unwrap_or(source.width() as f32);
        let viewport_h = request.viewport_height.unwrap_or(source.height() as f32);
        let (fit, view_x, view_y) =
            longshot_viewport_fit(source.width(), source.height(), viewport_w, viewport_h);
        render_longshot_raster_commands(&mut canvas, &source, request, fit, view_x, view_y);
        for item in &request.shape_items {
            render_shape_item_for_longshot(&mut canvas, item, fit, view_x, view_y);
        }
        for item in &request.text_items {
            let (x, y) = longshot_scene_to_image(item.x, item.y, fit, view_x, view_y);
            let font_size = (item.font_size / fit).max(8.0);
            draw_text_item(&mut canvas, item, x, y, font_size)?;
        }
    } else {
        let selection = request
            .selection
            .as_ref()
            .ok_or_else(|| "缺少裁剪区域".to_string())?;
        let dpr = request.device_pixel_ratio.unwrap_or(1.0).max(0.1);
        render_normal_raster_commands(&mut canvas, &source, request, selection, dpr);
        render_normal_shapes(&mut canvas, request, selection, dpr);
        for item in &request.text_items {
            draw_text_item(
                &mut canvas,
                item,
                (item.x - selection.x) * dpr,
                (item.y - selection.y) * dpr,
                item.font_size * dpr,
            )?;
        }
    }

    Ok(canvas)
}

fn export_screenshot_image(request: &ScreenshotExportRequest) -> Result<(), String> {
    if request.output_path.trim().is_empty() {
        return Err("缺少导出目标路径".to_string());
    }
    let canvas = render_screenshot_image(request)?;
    canvas
        .save(&request.output_path)
        .map_err(|e| format!("写入导出图片失败: {}", e))?;
    Ok(())
}

fn is_duplicate_copy_paste_request(text: &str, request_id: Option<&str>) -> bool {
    COPY_PASTE_DEDUP_TOTAL_REQUESTS.fetch_add(1, Ordering::Relaxed);
    if !COPY_PASTE_DEDUP_ENABLED.load(Ordering::Relaxed) {
        return false;
    }
    let request_id_trimmed = request_id.unwrap_or("").trim();
    let text_hash = calc_text_hash(text);
    let now_ms = now_unix_ms();
    let dedup_window_ms = COPY_PASTE_DEDUP_WINDOW_MS.load(Ordering::Relaxed);
    let lock = RECENT_COPY_PASTE.get_or_init(|| StdMutex::new(None));
    let mut guard = lock.lock().unwrap_or_else(|poisoned| {
        log::warn!("复制粘贴去重锁中毒，尝试恢复");
        poisoned.into_inner()
    });
    let mut is_hit = false;
    if let Some(last) = guard.as_ref() {
        let within_window = now_ms.saturating_sub(last.created_at_ms) <= dedup_window_ms;
        let same_request_id =
            !request_id_trimmed.is_empty() && request_id_trimmed == last.request_id;
        let same_text_hash = last.text_hash == text_hash;
        if within_window && (same_request_id || same_text_hash) {
            COPY_PASTE_DEDUP_HIT_COUNT.fetch_add(1, Ordering::Relaxed);
            if same_request_id {
                COPY_PASTE_DEDUP_REQUEST_ID_HIT_COUNT.fetch_add(1, Ordering::Relaxed);
            } else {
                COPY_PASTE_DEDUP_TEXT_HASH_HIT_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            is_hit = true;
        }
    }
    let stats_lock = COPY_PASTE_DEDUP_WINDOW_STATS.get_or_init(|| {
        StdMutex::new(DedupWindowStats {
            window_start_ms: now_ms,
            requests: 0,
            hits: 0,
            last_hit_at_ms: 0,
        })
    });
    let mut stats = stats_lock.lock().unwrap_or_else(|poisoned| {
        log::warn!("复制粘贴去重窗口统计锁中毒，尝试恢复");
        poisoned.into_inner()
    });
    if now_ms.saturating_sub(stats.window_start_ms) > dedup_window_ms {
        stats.window_start_ms = now_ms;
        stats.requests = 0;
        stats.hits = 0;
    }
    stats.requests = stats.requests.saturating_add(1);
    if is_hit {
        stats.hits = stats.hits.saturating_add(1);
        stats.last_hit_at_ms = now_ms;
        return true;
    }
    *guard = Some(RecentCopyPaste {
        request_id: request_id_trimmed.to_string(),
        text_hash,
        created_at_ms: now_ms,
    });
    false
}

fn get_copy_paste_dedup_debug_state_value() -> serde_json::Value {
    let now_ms = now_unix_ms();
    let dedup_window_ms = COPY_PASTE_DEDUP_WINDOW_MS.load(Ordering::Relaxed);
    let stats_lock = COPY_PASTE_DEDUP_WINDOW_STATS.get_or_init(|| {
        StdMutex::new(DedupWindowStats {
            window_start_ms: now_ms,
            requests: 0,
            hits: 0,
            last_hit_at_ms: 0,
        })
    });
    let stats = stats_lock.lock().unwrap_or_else(|poisoned| {
        log::warn!("复制粘贴去重窗口统计锁中毒，尝试恢复");
        poisoned.into_inner()
    });
    let mut window_requests = stats.requests;
    let mut window_hits = stats.hits;
    if now_ms.saturating_sub(stats.window_start_ms) > dedup_window_ms {
        window_requests = 0;
        window_hits = 0;
    }
    let window_hit_rate = if window_requests == 0 {
        0.0
    } else {
        (window_hits as f64 / window_requests as f64) * 100.0
    };
    serde_json::json!({
        "enabled": COPY_PASTE_DEDUP_ENABLED.load(Ordering::Relaxed),
        "window_ms": COPY_PASTE_DEDUP_WINDOW_MS.load(Ordering::Relaxed),
        "log_enabled": COPY_PASTE_DEDUP_LOG_ENABLED.load(Ordering::Relaxed),
        "metrics": {
            "total_requests": COPY_PASTE_DEDUP_TOTAL_REQUESTS.load(Ordering::Relaxed),
            "dedup_hits": COPY_PASTE_DEDUP_HIT_COUNT.load(Ordering::Relaxed),
            "request_id_hits": COPY_PASTE_DEDUP_REQUEST_ID_HIT_COUNT.load(Ordering::Relaxed),
            "text_hash_hits": COPY_PASTE_DEDUP_TEXT_HASH_HIT_COUNT.load(Ordering::Relaxed),
            "log_count": COPY_PASTE_DEDUP_LOG_COUNT.load(Ordering::Relaxed),
            "window_requests": window_requests,
            "window_hits": window_hits,
            "window_hit_rate_percent": window_hit_rate,
            "last_hit_at_ms": stats.last_hit_at_ms,
        }
    })
}

fn bind_screenshot_window_lifecycle(window: &tauri::WebviewWindow, app: &AppHandle) {
    bind_overlay_window_events(window, app.clone(), "screenshot");
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed => {
            features::screenshot::capture::set_screenshot_in_progress(false);
        }
        _ => {}
    });
}

#[derive(serde::Serialize)]
pub struct HistoryResponse {
    history: Vec<String>,
    categories: HashMap<String, String>,
    category_list: Vec<String>,
    pinned_items: Vec<String>,
}

/// 批量获取剪贴板完整快照（优化 IPC 通信）
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardFullSnapshot {
    pub text_history: Vec<String>,
    pub text_categories: HashMap<String, String>,
    pub text_category_list: Vec<String>,
    pub text_pinned_items: Vec<String>,
    pub image_history: Vec<ImageHistoryPreviewItem>,
    pub image_categories: HashMap<String, String>,
    pub image_category_list: Vec<String>,
    pub image_tags: HashMap<String, Vec<String>>,
    pub image_pinned_items: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardHistoryPageRequest {
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_history_page_limit")]
    limit: usize,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    pinned_only: bool,
    #[serde(default)]
    keyword: Option<String>,
    #[serde(default)]
    sort_by: Option<String>,
    #[serde(default)]
    sort_order: Option<String>,
}

#[tauri::command]
pub async fn get_image_clipboard_history_page(
    request: ImageHistoryPageRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<ImageHistoryPageData, String> {
    let started_at = std::time::Instant::now();
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };

    let page_request = crate::utils::image_clipboard::ImageHistoryPageRequest {
        offset: request.offset,
        limit: request.limit,
        category: request.category,
        keyword: request.keyword,
        pinned_only: request.pinned_only,
        sort_by: request.sort_by,
        sort_order: request.sort_order,
    };

    let result = manager.get_history_preview_page_async(page_request).await;
    record_perf_metric(
        "image.history_page",
        "图片历史分页加载耗时",
        started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    Ok(result)
}

fn default_history_page_limit() -> usize {
    50
}

#[derive(serde::Serialize)]
pub struct ImageHistoryResponse {
    history: Vec<ImageHistoryPreviewItem>,
    categories: HashMap<String, String>,
    category_list: Vec<String>,
    image_tags: HashMap<String, Vec<String>>,
    pinned_items: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectAndFillRequest {
    index: usize,
    #[serde(default)]
    op_id: Option<u64>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectAndFillImageByIdRequest {
    item_id: String,
    #[serde(default)]
    op_id: Option<u64>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemIdRequest {
    item_id: String,
}

#[tauri::command]
pub async fn open_image_preview_window_by_id(
    request: ItemIdRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_open_image_preview_window_by_id(request.item_id, state_arc, app)
            .map_err(to_frontend_error_string)
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "打开图片预览任务执行失败",
            e.to_string(),
        )
    })?
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FillKind {
    Text,
    Image,
}

impl FillKind {
    fn label(self) -> &'static str {
        match self {
            Self::Text => "文本",
            Self::Image => "图片",
        }
    }

    fn window_label(self) -> &'static str {
        match self {
            Self::Text => "clipboard",
            Self::Image => "image_clipboard",
        }
    }

    fn current_seq(self, state: &SharedAppState) -> u64 {
        match self {
            Self::Text => state.text_fill_seq,
            Self::Image => state.image_fill_seq,
        }
    }
}

fn lock_arc_mutex<T>(mutex: &Arc<Mutex<T>>) -> crate::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|never| match never {})
}

fn is_screenshot_feature_enabled(state: &Arc<Mutex<SharedAppState>>) -> bool {
    let guard = lock_arc_mutex(state);
    guard.settings.screenshot_enabled
}

fn recompute_selection_related_flags(state: &mut SharedAppState) {
    state.is_processing_selection = state.is_selection_capture_active
        || state.is_text_writeback_active
        || state.is_image_writeback_active;
    state.is_updating_clipboard = state.is_text_writeback_active || state.is_image_writeback_active;
}

fn emit_writeback_phase(
    app: &AppHandle,
    source: &str,
    stage: &str,
    operation_id: Option<u64>,
    detail: Option<String>,
) {
    let _ = app.emit(
        "writeback-phase",
        serde_json::json!({
            "source": source,
            "stage": stage,
            "operationId": operation_id,
            "detail": detail,
        }),
    );
}

fn writeback_metric_source_key(source: &str) -> &'static str {
    match source {
        "文本" => "text",
        "图片" => "image",
        "结果窗" => "result_window",
        _ => "unknown",
    }
}

fn record_writeback_stage_metric(
    source: &str,
    stage: &str,
    label: &str,
    duration_ms: u64,
    success: bool,
    error: Option<String>,
) {
    let key = format!(
        "writeback.{}.{}",
        writeback_metric_source_key(source),
        stage
    );
    record_perf_metric(&key, label, duration_ms, success, error);
}

fn perf_metric_group_label(key: &str) -> &'static str {
    if key.starts_with("ocr.") || key.starts_with("ai.") {
        "交互"
    } else if key.starts_with("backup.") {
        "备份"
    } else if key.starts_with("recording.") {
        "录屏"
    } else if key.starts_with("screenshot.") {
        "截图"
    } else if key.starts_with("writeback.") {
        "回写"
    } else if key.starts_with("image.") {
        "图片"
    } else if key.starts_with("text.") {
        "文本历史"
    } else {
        "其他"
    }
}

fn perf_metric_group_rank(group: &str) -> usize {
    match group {
        "交互" => 0,
        "回写" => 1,
        "图片" => 2,
        "截图" => 3,
        "录屏" => 4,
        "备份" => 5,
        "文本历史" => 6,
        _ => 9,
    }
}

fn perf_metric_is_slow(item: &crate::core::perf_metrics::PerfMetricSnapshot) -> bool {
    let (avg_threshold, max_threshold) = if item.key.contains("first_chunk") {
        (1200.0, 2500)
    } else if item.key.contains("history_page") || item.key.contains("wait_hidden") {
        (900.0, 2000)
    } else {
        (1500.0, 3000)
    };
    item.avg_duration_ms >= avg_threshold || item.max_duration_ms >= max_threshold
}

fn last_writeback_result() -> Option<WriteBackExecutionResult> {
    LAST_WRITEBACK_RESULT
        .get()
        .and_then(|slot| slot.lock().ok().and_then(|guard| guard.clone()))
}

fn register_text_shortcut(
    app: &AppHandle,
    state: Arc<Mutex<SharedAppState>>,
    hot_key: &str,
) -> Result<(), String> {
    let app_clone = app.clone();
    let hot_key_string = hot_key.to_string();
    app.global_shortcut()
        .on_shortcut(hot_key, move |_app, _shortcut, event| {
            if let ShortcutState::Pressed = event.state {
                let sg = lock_arc_mutex(&state);
                if !sg.settings.text_clipboard_enabled {
                    return;
                }
                if !sg.is_visible && !sg.is_image_visible {
                    let state_for_window = state.clone();
                    drop(sg);
                    interrupt_text_fill_flow(&state_for_window);
                    show_clipboard_window(app_clone.clone(), state_for_window);
                    features::mouse_listener::reset_ctrl_key_state();
                }
            }
        })
        .map_err(|e| {
            frontend_error(
                ErrorCode::ValidationError,
                format!("快捷键被占用或注册失败：{}", hot_key_string),
                e.to_string(),
            )
        })?;
    Ok(())
}

fn register_image_shortcut(
    app: &AppHandle,
    state: Arc<Mutex<SharedAppState>>,
    hot_key: &str,
) -> Result<(), String> {
    let app_clone = app.clone();
    let hot_key_string = hot_key.to_string();
    app.global_shortcut()
        .on_shortcut(hot_key, move |_app, _shortcut, event| {
            if let ShortcutState::Pressed = event.state {
                let sg = lock_arc_mutex(&state);
                if !sg.settings.image_clipboard_enabled {
                    return;
                }
                if !sg.is_visible && !sg.is_image_visible {
                    let state_for_window = state.clone();
                    drop(sg);
                    interrupt_image_fill_flow(&state_for_window);
                    show_image_clipboard_window(app_clone.clone(), state_for_window);
                }
            }
        })
        .map_err(|e| {
            frontend_error(
                ErrorCode::ValidationError,
                format!("图片窗口快捷键被占用或注册失败：{}", hot_key_string),
                e.to_string(),
            )
        })?;
    Ok(())
}

fn register_screenshot_shortcut(app: &AppHandle, hot_key: &str) -> Result<(), String> {
    let app_clone = app.clone();
    app.global_shortcut()
        .on_shortcut(hot_key, move |_app, _shortcut, event| {
            if let ShortcutState::Pressed = event.state {
                let app_handle_inner = app_clone.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = open_screenshot_editor(app_handle_inner, None).await {
                        log::error!("截图失败: {}", e);
                    }
                });
            }
        })
        .map_err(|e| frontend_error(ErrorCode::SystemError, "注册截图快捷键失败", e.to_string()))?;
    Ok(())
}

fn begin_fill_sequence(state: &Arc<Mutex<SharedAppState>>, kind: FillKind) -> u64 {
    let mut state_guard = lock_arc_mutex(state);
    state_guard.selection_guard_epoch = state_guard.selection_guard_epoch.wrapping_add(1);
    match kind {
        FillKind::Text => state_guard.is_text_writeback_active = true,
        FillKind::Image => state_guard.is_image_writeback_active = true,
    }
    recompute_selection_related_flags(&mut state_guard);
    match kind {
        FillKind::Text => {
            state_guard.text_fill_seq = state_guard.text_fill_seq.wrapping_add(1);
            state_guard.text_fill_seq
        }
        FillKind::Image => {
            state_guard.image_fill_seq = state_guard.image_fill_seq.wrapping_add(1);
            state_guard.image_fill_seq
        }
    }
}

fn is_fill_latest(state: &Arc<Mutex<SharedAppState>>, kind: FillKind, fill_seq: u64) -> bool {
    let guard = lock_arc_mutex(state);
    kind.current_seq(&guard) == fill_seq
}

fn finish_fill_if_latest(state: &Arc<Mutex<SharedAppState>>, kind: FillKind, fill_seq: u64) {
    let mut guard = lock_arc_mutex(state);
    if kind.current_seq(&guard) == fill_seq {
        match kind {
            FillKind::Text => guard.is_text_writeback_active = false,
            FillKind::Image => guard.is_image_writeback_active = false,
        }
        recompute_selection_related_flags(&mut guard);
    }
}

static IMAGE_PROMOTE_SENDER: OnceLock<Sender<String>> = OnceLock::new();

pub fn interrupt_text_fill_flow(state: &Arc<Mutex<SharedAppState>>) {
    let mut state_guard = lock_arc_mutex(state);
    state_guard.text_fill_seq = state_guard.text_fill_seq.wrapping_add(1);
    state_guard.is_text_writeback_active = false;
    recompute_selection_related_flags(&mut state_guard);
}

pub fn interrupt_image_fill_flow(state: &Arc<Mutex<SharedAppState>>) {
    let mut state_guard = lock_arc_mutex(state);
    state_guard.image_fill_seq = state_guard.image_fill_seq.wrapping_add(1);
    state_guard.is_image_writeback_active = false;
    recompute_selection_related_flags(&mut state_guard);
}

fn image_promote_worker(state: Arc<Mutex<SharedAppState>>, rx: Receiver<String>) {
    while let Ok(mut item_id) = rx.recv() {
        while let Ok(latest_item_id) = rx.try_recv() {
            item_id = latest_item_id;
        }
        let manager_arc = {
            let state_guard = lock_arc_mutex(&state);
            state_guard.image_clipboard_manager.clone()
        };
        let manager = lock_arc_mutex(&manager_arc);
        if let Err(e) = manager.promote_to_top_by_id(&item_id) {
            log::warn!("极速模式异步置顶图片失败: {}", e);
        } else {
            manager.sync_positions_to_store();
        }
    }
}

fn schedule_image_promote_to_top(state: Arc<Mutex<SharedAppState>>, item_id: String) {
    let sender = IMAGE_PROMOTE_SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<String>();
        let state_for_worker = state.clone();
        thread::spawn(move || image_promote_worker(state_for_worker, rx));
        tx
    });
    if let Err(e) = sender.send(item_id) {
        log::warn!("提交极速模式异步置顶任务失败: {}", e);
    }
}

fn wait_for_fill_window_hidden(
    app: &AppHandle,
    window_label: &str,
    label: &str,
    fast_path: bool,
) -> Result<(), String> {
    let timeout_ms = if fast_path { 220 } else { 900 };
    let state_arc = app.state::<Arc<Mutex<SharedAppState>>>().inner().clone();
    crate::ui::window_manager::wait_for_window_hidden(
        app,
        &state_arc,
        window_label,
        Duration::from_millis(timeout_ms),
    )
    .map_err(|e| {
        let message = e.to_string();
        log::warn!("等待{}窗口隐藏失败: {}", label, message);
        message
    })
}

fn spawn_fill_task<F>(
    kind: FillKind,
    app_handle: AppHandle,
    state: Arc<Mutex<SharedAppState>>,
    fill_seq: u64,
    operation_id: u64,
    write_stage: F,
) where
    F: FnOnce(&AppHandle, &Arc<Mutex<SharedAppState>>) -> Result<(), String> + Send + 'static,
{
    thread::spawn(move || {
        let started_at = std::time::Instant::now();
        let fast_path = kind == FillKind::Image && is_fast_fill_verify_mode_enabled();
        emit_writeback_phase(
            &app_handle,
            kind.label(),
            "waiting_window_hidden",
            Some(operation_id),
            None,
        );
        let wait_started_at = std::time::Instant::now();
        let wait_result =
            wait_for_fill_window_hidden(&app_handle, kind.window_label(), kind.label(), fast_path);
        match &wait_result {
            Ok(_) => record_writeback_stage_metric(
                kind.label(),
                "wait_hidden",
                &format!("{}回写等待窗口隐藏耗时", kind.label()),
                wait_started_at.elapsed().as_millis() as u64,
                true,
                None,
            ),
            Err(error) => record_writeback_stage_metric(
                kind.label(),
                "wait_hidden",
                &format!("{}回写等待窗口隐藏耗时", kind.label()),
                wait_started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            ),
        }

        if !is_fill_latest(&state, kind, fill_seq) {
            log::info!(
                "{}回填请求过期，跳过执行: op_id={}",
                kind.label(),
                operation_id
            );
            emit_writeback_phase(
                &app_handle,
                kind.label(),
                "cancelled_stale",
                Some(operation_id),
                Some("回填请求已被更新请求替代".to_string()),
            );
            return;
        }

        let clipboard_started_at = std::time::Instant::now();
        let fill_result = write_stage(&app_handle, &state);
        if fill_result.is_ok() {
            record_writeback_stage_metric(
                kind.label(),
                "write_clipboard",
                &format!("{}回写写入剪贴板耗时", kind.label()),
                clipboard_started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            emit_writeback_phase(
                &app_handle,
                kind.label(),
                "clipboard_written",
                Some(operation_id),
                None,
            );
            if !is_fill_latest(&state, kind, fill_seq) {
                log::info!(
                    "{}回填请求被新请求替代: op_id={}",
                    kind.label(),
                    operation_id
                );
                emit_writeback_phase(
                    &app_handle,
                    kind.label(),
                    "cancelled_stale",
                    Some(operation_id),
                    Some("回填请求已被更新请求替代".to_string()),
                );
                return;
            }
            emit_writeback_phase(
                &app_handle,
                kind.label(),
                "pasting",
                Some(operation_id),
                None,
            );
            let paste_started_at = std::time::Instant::now();
            let paste_result = simulate_paste_with_retry(
                &app_handle,
                kind.label(),
                Some(operation_id),
                started_at,
                fast_path,
            );
            match paste_result {
                Ok(result) => {
                    record_writeback_stage_metric(
                        kind.label(),
                        "paste",
                        &format!("{}回写粘贴耗时", kind.label()),
                        paste_started_at.elapsed().as_millis() as u64,
                        true,
                        None,
                    );
                    record_writeback_stage_metric(
                        kind.label(),
                        "total",
                        &format!("{}回写总耗时", kind.label()),
                        started_at.elapsed().as_millis() as u64,
                        true,
                        None,
                    );
                    emit_writeback_phase(
                        &app_handle,
                        kind.label(),
                        "completed",
                        Some(operation_id),
                        Some(result.detail.clone()),
                    );
                    emit_writeback_result(&app_handle, &result)
                }
                Err(result) => {
                    record_writeback_stage_metric(
                        kind.label(),
                        "paste",
                        &format!("{}回写粘贴耗时", kind.label()),
                        paste_started_at.elapsed().as_millis() as u64,
                        false,
                        Some(result.detail.clone()),
                    );
                    record_writeback_stage_metric(
                        kind.label(),
                        "total",
                        &format!("{}回写总耗时", kind.label()),
                        started_at.elapsed().as_millis() as u64,
                        false,
                        Some(result.detail.clone()),
                    );
                    emit_writeback_phase(
                        &app_handle,
                        kind.label(),
                        "failed",
                        Some(operation_id),
                        Some(result.detail.clone()),
                    );
                    emit_writeback_result(&app_handle, &result)
                }
            }
        } else if let Err(e) = fill_result {
            log::error!(
                "{}回填失败（写入阶段）: op_id={}, {}",
                kind.label(),
                operation_id,
                e
            );
            record_writeback_stage_metric(
                kind.label(),
                "write_clipboard",
                &format!("{}回写写入剪贴板耗时", kind.label()),
                clipboard_started_at.elapsed().as_millis() as u64,
                false,
                Some(e.clone()),
            );
            record_writeback_stage_metric(
                kind.label(),
                "total",
                &format!("{}回写总耗时", kind.label()),
                started_at.elapsed().as_millis() as u64,
                false,
                Some(e.clone()),
            );
            emit_writeback_phase(
                &app_handle,
                kind.label(),
                "failed",
                Some(operation_id),
                Some(e.clone()),
            );
            emit_writeback_result(
                &app_handle,
                &WriteBackExecutionResult {
                    source: kind.label().to_string(),
                    success: false,
                    stage: "write_clipboard_failed".to_string(),
                    target_window_title: String::new(),
                    target_window_pid: 0,
                    detail: e,
                    operation_id: Some(operation_id),
                },
            );
        }

        finish_fill_if_latest(&state, kind, fill_seq);
    });
}

fn simulate_paste_with_retry(
    app_handle: &AppHandle,
    label: &str,
    operation_id: Option<u64>,
    started_at: std::time::Instant,
    fast_path: bool,
) -> Result<WriteBackExecutionResult, WriteBackExecutionResult> {
    let is_post_paste_ctrl_release_error = |err: &str| err.contains("释放 Ctrl");
    let mode_name = if fast_path {
        "极速模式"
    } else {
        "普通模式"
    };
    let retry_delays: &[u64] = if fast_path { &[8, 16] } else { &[22, 40, 58] };

    match crate::ui::window_manager::simulate_paste(app_handle) {
        Ok(target) => {
            if let Some(op_id) = operation_id {
                log::info!(
                    "{}回填完成: op_id={}, 耗时: {}ms",
                    label,
                    op_id,
                    started_at.elapsed().as_millis()
                );
            } else {
                log::info!(
                    "{}回填完成，耗时: {}ms",
                    label,
                    started_at.elapsed().as_millis()
                );
            }
            Ok(WriteBackExecutionResult {
                source: label.to_string(),
                success: true,
                stage: "pasted".to_string(),
                target_window_title: target.title,
                target_window_pid: target.pid,
                detail: format!(
                    "{}回填成功，耗时 {}ms",
                    label,
                    started_at.elapsed().as_millis()
                ),
                operation_id,
            })
        }
        Err(first_error) => {
            if is_post_paste_ctrl_release_error(&first_error) {
                log::warn!(
                    "{}回填检测到粘贴后Ctrl释放异常，跳过二次粘贴以避免重复输入: {}",
                    label,
                    first_error
                );
                if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                    log::warn!("{}回填粘贴后Ctrl异常兜底释放失败: {}", label, release_error);
                }
                return Err(WriteBackExecutionResult {
                    source: label.to_string(),
                    success: false,
                    stage: "paste_ctrl_release_failed".to_string(),
                    target_window_title: String::new(),
                    target_window_pid: 0,
                    detail: first_error,
                    operation_id,
                });
            }
            let mut final_error = first_error.clone();
            for delay in retry_delays {
                thread::sleep(Duration::from_millis(*delay));
                if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                    log::warn!(
                        "{}回填{}重试前释放Ctrl失败: {}",
                        label,
                        mode_name,
                        release_error
                    );
                }
                match crate::ui::window_manager::simulate_paste(app_handle) {
                    Ok(target) => {
                        if let Some(op_id) = operation_id {
                            log::warn!(
                                "{}回填{}首次粘贴失败，状态驱动重试成功: op_id={}, {}，总耗时: {}ms",
                                label,
                                mode_name,
                                op_id,
                                first_error,
                                started_at.elapsed().as_millis()
                            );
                        } else {
                            log::warn!(
                                "{}回填{}首次粘贴失败，状态驱动重试成功: {}，总耗时: {}ms",
                                label,
                                mode_name,
                                first_error,
                                started_at.elapsed().as_millis()
                            );
                        }
                        return Ok(WriteBackExecutionResult {
                            source: label.to_string(),
                            success: true,
                            stage: "pasted_after_retry".to_string(),
                            target_window_title: target.title,
                            target_window_pid: target.pid,
                            detail: format!("首次失败后重试成功: {}", first_error),
                            operation_id,
                        });
                    }
                    Err(next_error) => {
                        final_error = next_error;
                        if is_post_paste_ctrl_release_error(&final_error) {
                            log::warn!(
                                "{}回填{}检测到粘贴后Ctrl释放异常，停止后续重试: {}",
                                label,
                                mode_name,
                                final_error
                            );
                            if let Err(release_error) =
                                crate::ui::window_manager::force_release_ctrl_key()
                            {
                                log::warn!(
                                    "{}回填粘贴后Ctrl异常兜底释放失败: {}",
                                    label,
                                    release_error
                                );
                            }
                            return Err(WriteBackExecutionResult {
                                source: label.to_string(),
                                success: false,
                                stage: "paste_ctrl_release_failed".to_string(),
                                target_window_title: String::new(),
                                target_window_pid: 0,
                                detail: final_error,
                                operation_id,
                            });
                        }
                    }
                }
            }
            if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                log::warn!(
                    "{}回填{}最终兜底释放Ctrl失败: {}",
                    label,
                    mode_name,
                    release_error
                );
            }
            if let Some(op_id) = operation_id {
                log::error!(
                    "{}回填{}粘贴失败: op_id={}, 首次错误: {}，最终错误: {}",
                    label,
                    mode_name,
                    op_id,
                    first_error,
                    final_error
                );
            } else {
                log::error!(
                    "{}回填{}粘贴失败，首次错误: {}，最终错误: {}",
                    label,
                    mode_name,
                    first_error,
                    final_error
                );
            }
            Err(WriteBackExecutionResult {
                source: label.to_string(),
                success: false,
                stage: "paste_failed".to_string(),
                target_window_title: String::new(),
                target_window_pid: 0,
                detail: format!("首次错误: {}，最终错误: {}", first_error, final_error),
                operation_id,
            })
        }
    }
}

fn set_updating_clipboard(state: &Arc<Mutex<SharedAppState>>, updating: bool) {
    let mut state_guard = lock_arc_mutex(state);
    if !updating {
        state_guard.is_text_writeback_active = false;
        state_guard.is_image_writeback_active = false;
    }
    recompute_selection_related_flags(&mut state_guard);
}

fn get_clipboard_manager_arc(state: &Arc<Mutex<SharedAppState>>) -> Arc<Mutex<ClipboardManager>> {
    let state_guard = lock_arc_mutex(state);
    state_guard.clipboard_manager.clone()
}

fn get_image_clipboard_manager_arc(
    state: &Arc<Mutex<SharedAppState>>,
) -> Arc<Mutex<ImageClipboardManager>> {
    let state_guard = lock_arc_mutex(state);
    state_guard.image_clipboard_manager.clone()
}

fn frontend_error(
    code: ErrorCode,
    message: impl Into<String>,
    details: impl Into<String>,
) -> String {
    to_frontend_error_string(AppError::new(code, message).with_details(details.into()))
}

fn with_updating_clipboard<T, F>(
    state: &Arc<Mutex<SharedAppState>>,
    operation: F,
) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    set_updating_clipboard(state, true);
    let result = operation();
    set_updating_clipboard(state, false);
    result
}

fn try_replace_text_clipboard_after_remove(
    state: &Arc<Mutex<SharedAppState>>,
    app: &AppHandle,
    removed_item: &str,
) {
    let manager_arc = get_clipboard_manager_arc(state);
    let current_clipboard = {
        let manager = lock_arc_mutex(&manager_arc);
        manager.get_content(app)
    };

    if current_clipboard.as_deref() != Some(removed_item) {
        return;
    }

    let next_item = {
        let manager = lock_arc_mutex(&manager_arc);
        manager.get_latest_item()
    };
    if let Some(next) = next_item {
        let manager = lock_arc_mutex(&manager_arc);
        if let Err(e) = manager.set_clipboard_content(app, &next) {
            log::warn!("删除文本后写入下一条到剪贴板失败: {}", e);
        }
    }
}

fn try_replace_image_clipboard_after_remove(
    state: &Arc<Mutex<SharedAppState>>,
    app: &AppHandle,
    removed_signature: &str,
) {
    let manager_arc = get_image_clipboard_manager_arc(state);
    let should_replace_clipboard = match ImageClipboardManager::read_clipboard_images_rgba(app) {
        Ok(images) if !images.is_empty() => {
            let (rgba, width, height, _) = &images[0];
            crate::utils::image_clipboard::compute_signature(rgba, *width, *height)
                == removed_signature
        }
        _ => false,
    };

    if !should_replace_clipboard {
        return;
    }

    let next_image = {
        let manager = lock_arc_mutex(&manager_arc);
        manager.get_image_by_index(0).ok()
    };
    if let Some(image) = next_image {
        if let Err(e) = ImageClipboardManager::write_clipboard_image(app, &image) {
            log::warn!("删除图片后写入下一张到剪贴板失败: {}", e);
        }
    }
}

fn execute_select_and_fill_text(
    request: SelectAndFillRequest,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> AppResult<String> {
    let index = request.index;
    let item_id = request.item_id;
    let fill_seq = begin_fill_sequence(&state, FillKind::Text);
    let operation_id = request.op_id.unwrap_or(fill_seq);
    let manager_arc = get_clipboard_manager_arc(&state);

    let item_content = {
        let manager = lock_arc_mutex(&manager_arc);
        manager.promote_to_top(index, item_id).map_err(|e| {
            AppError::new(
                ErrorCode::ClipboardError,
                format!("索引 {:?} 超出范围", index),
            )
            .with_details(e)
        })?
    };

    hide_clipboard_window(app.clone(), state.clone());

    let item_content_clone = item_content.clone();
    let manager_arc_for_fill = manager_arc.clone();
    spawn_fill_task(
        FillKind::Text,
        app,
        state,
        fill_seq,
        operation_id,
        move |app_handle, state_ref| {
            let _ = state_ref;
            let manager = lock_arc_mutex(&manager_arc_for_fill);
            manager.set_clipboard_content(app_handle, &item_content_clone)?;
            let item_id = crate::utils::database::stable_history_item_id(&item_content_clone);
            let _ = app_handle.emit(
                "text-item-promoted",
                serde_json::json!({
                    "id": item_id,
                }),
            );
            Ok(())
        },
    );

    Ok(item_content)
}

fn execute_remove_clipboard_item(
    index: Option<usize>,
    item_id: Option<String>,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> AppResult<()> {
    log::info!(
        "删除剪贴板项目，索引: {:?}, item_id存在: {}",
        index,
        item_id.is_some()
    );
    let manager_arc = get_clipboard_manager_arc(&state);
    with_updating_clipboard(&state, || -> Result<(), String> {
        let resolved_index = {
            let manager = lock_arc_mutex(&manager_arc);
            if let Some(target_id) = item_id.as_ref().filter(|v| !v.trim().is_empty()) {
                manager
                    .get_history()
                    .iter()
                    .position(|entry| &crate::utils::database::stable_history_item_id(entry) == target_id)
                    .or(index)
                    .ok_or_else(|| "索引超出范围".to_string())?
            } else {
                index.ok_or_else(|| "索引超出范围".to_string())?
            }
        };
        let removed_item = {
            let manager = lock_arc_mutex(&manager_arc);
            manager.remove_from_history(resolved_index)?
        };
        try_replace_text_clipboard_after_remove(&state, &app, &removed_item);
        Ok(())
    })
    .map_err(|e| AppError::new(ErrorCode::ClipboardError, "删除文本历史失败").with_details(e))
}

fn execute_open_image_preview_window_by_id(
    item_id: String,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> AppResult<()> {
    let started_at = std::time::Instant::now();
    let request_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .to_string();
    show_image_preview_loading_window(app.clone(), request_id.clone()).map_err(|e| {
        AppError::new(ErrorCode::SystemError, "打开预览加载窗口失败").with_details(e)
    })?;
    let state_clone = state;
    let app_clone = app;
    let request_id_clone = request_id;
    thread::spawn(move || {
        let result: Result<(), String> = (|| {
            let prepare_started_at = std::time::Instant::now();
            let manager_arc = get_image_clipboard_manager_arc(&state_clone);
            let image_path = {
                let manager = lock_arc_mutex(&manager_arc);
                manager.get_preview_image_path_by_id(&item_id)?
            };
            let preview_path = ensure_preview_image_path_for_asset(&item_id, &image_path)?;
            record_perf_metric(
                "image.preview_prepare",
                "图片预览资源准备耗时",
                prepare_started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            show_image_preview_window(app_clone.clone(), request_id_clone.clone(), preview_path)
        })();
        match result {
            Ok(()) => {
                record_perf_metric(
                    "image.preview_open",
                    "图片预览打开耗时",
                    started_at.elapsed().as_millis() as u64,
                    true,
                    None,
                );
            }
            Err(e) => {
                record_perf_metric(
                    "image.preview_open",
                    "图片预览打开耗时",
                    started_at.elapsed().as_millis() as u64,
                    false,
                    Some(e.clone()),
                );
                log::error!("加载预览图片失败: {}", e);
                let _ = app_clone.emit(
                    "show-image-preview",
                    serde_json::json!({
                        "request_id": request_id_clone,
                        "error_message": e,
                        "is_final": true
                    }),
                );
            }
        }
    });
    Ok(())
}

fn ensure_preview_image_path_for_asset(item_id: &str, image_path: &str) -> Result<String, String> {
    let started_at = std::time::Instant::now();
    let trimmed = image_path.trim();
    if trimmed.is_empty() {
        let error = "图片路径为空".to_string();
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        return Err(error);
    }
    let source_path = PathBuf::from(trimmed);
    if !source_path.exists() {
        let error = format!("图片文件不存在: {}", trimmed);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        return Err(error);
    }
    if !source_path.is_file() {
        let error = format!("图片路径不是文件: {}", trimmed);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        return Err(error);
    }
    let canonical_source = source_path.canonicalize().map_err(|e| {
        let error = format!("规范化图片路径失败: {}", e);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    let ext = canonical_source
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let allowed_ext = ["png", "jpg", "jpeg", "webp", "bmp", "gif"];
    if !allowed_ext.contains(&ext.as_str()) {
        let error = format!("不支持的图片格式: {}", ext);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        return Err(error);
    }

    let mut blobs_dir = std::env::current_exe().map_err(|e| {
        let error = format!("获取程序目录失败: {}", e);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    blobs_dir.pop();
    blobs_dir.push("image_history_blobs");
    fs::create_dir_all(&blobs_dir).map_err(|e| {
        let error = format!("创建图片目录失败: {}", e);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    let canonical_blobs = blobs_dir
        .canonicalize()
        .unwrap_or_else(|_| blobs_dir.clone());
    if canonical_source.starts_with(&canonical_blobs) {
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            true,
            None,
        );
        return Ok(canonical_source.to_string_lossy().to_string());
    }

    let normalized_item_id = sanitize_image_item_id(item_id);
    let target_name = if ext.is_empty() {
        format!("preview_external_{}.png", normalized_item_id)
    } else {
        format!("preview_external_{}.{}", normalized_item_id, ext)
    };
    let target_path = canonical_blobs.join(target_name);
    fs::copy(&canonical_source, &target_path).map_err(|e| {
        let error = format!("复制预览图片到受控目录失败: {}", e);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    record_perf_metric(
        "image.preview_asset_path",
        "图片预览路径准备耗时",
        started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    Ok(target_path.to_string_lossy().to_string())
}

fn sanitize_image_item_id(raw: &str) -> String {
    let sanitized: String = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

fn execute_warmup_image_clipboard_item_by_id(
    item_id: String,
    state: Arc<Mutex<SharedAppState>>,
) -> AppResult<()> {
    let started_at = std::time::Instant::now();
    let manager_arc = get_image_clipboard_manager_arc(&state);
    let manager = lock_arc_mutex(&manager_arc);
    manager.warmup_image_by_id(&item_id).map_err(|e| {
        record_perf_metric(
            "image.warmup",
            "图片预热耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(e.clone()),
        );
        AppError::new(ErrorCode::ClipboardError, "预热图片失败").with_details(e)
    })?;
    record_perf_metric(
        "image.warmup",
        "图片预热耗时",
        started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    Ok(())
}

fn execute_promote_image_clipboard_item_by_id(
    item_id: String,
    state: Arc<Mutex<SharedAppState>>,
) -> AppResult<()> {
    let manager_arc = get_image_clipboard_manager_arc(&state);
    let manager = lock_arc_mutex(&manager_arc);
    manager
        .promote_to_top_by_id(&item_id)
        .map_err(|e| AppError::new(ErrorCode::ClipboardError, "置顶图片失败").with_details(e))
}

fn execute_remove_image_clipboard_item_by_id(
    item_id: String,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> AppResult<()> {
    let manager_arc = get_image_clipboard_manager_arc(&state);
    with_updating_clipboard(&state, || -> Result<(), String> {
        let removed_signature = {
            let manager = lock_arc_mutex(&manager_arc);
            let (_, _, signature) = manager.remove_from_history_by_id(&item_id)?;
            signature
        };
        try_replace_image_clipboard_after_remove(&state, &app, &removed_signature);
        Ok(())
    })
    .map_err(|e| AppError::new(ErrorCode::ClipboardError, "删除图片历史失败").with_details(e))
}

fn execute_select_and_fill_image_by_id(
    request: SelectAndFillImageByIdRequest,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> AppResult<()> {
    let item_id = request.item_id;
    let fill_seq = begin_fill_sequence(&state, FillKind::Image);
    let operation_id = request.op_id.unwrap_or(fill_seq);
    let manager_arc = get_image_clipboard_manager_arc(&state);

    hide_image_clipboard_window(app.clone(), state.clone());

    spawn_fill_task(
        FillKind::Image,
        app,
        state,
        fill_seq,
        operation_id,
        move |app_handle, state_ref| {
            let prepare_started_at = std::time::Instant::now();
            let fast_mode = is_fast_fill_verify_mode_enabled();
            let image = {
                let _ = state_ref;
                let manager = lock_arc_mutex(&manager_arc);
                if fast_mode {
                    manager.promote_to_top_in_memory_by_id(&item_id)?;
                    manager.get_image_by_index_for_fill(0)?
                } else {
                    manager.promote_to_top_by_id(&item_id)?;
                    manager.get_image_by_index_for_fill(0)?
                }
            };
            record_perf_metric(
                "image.fill_prepare",
                "图片回填准备耗时",
                prepare_started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            ImageClipboardManager::write_clipboard_image(app_handle, &image)?;
            let _ = app_handle.emit(
                "image-item-pinned",
                serde_json::json!({
                    "itemId": item_id,
                    "pinned": true,
                }),
            );
            if fast_mode {
                schedule_image_promote_to_top(state_ref.clone(), item_id.clone());
            }
            Ok(())
        },
    );

    Ok(())
}

#[tauri::command]
pub async fn get_clipboard_history(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<HistoryResponse, String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = lock_arc_mutex(&manager_arc);
    Ok(HistoryResponse {
        history: manager.get_history(),
        categories: manager.get_categories(),
        category_list: manager.get_category_list(),
        pinned_items: manager.get_pinned_items(),
    })
}

/// 批量获取剪贴板完整快照（优化 IPC 通信）
/// 一次 IPC 调用获取所有需要的数据，减少通信开销
#[tauri::command]
pub async fn get_clipboard_full_snapshot(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<ClipboardFullSnapshot, String> {
    let (text_manager_arc, image_manager_arc) = {
        let state_guard = lock_arc_mutex(state.inner());
        (
            state_guard.clipboard_manager.clone(),
            state_guard.image_clipboard_manager.clone(),
        )
    };

    let text_manager = lock_arc_mutex(&text_manager_arc);
    let text_history = text_manager.get_history();
    let text_categories = text_manager.get_categories();
    let text_category_list = text_manager.get_category_list();
    let text_pinned_items = text_manager.get_pinned_items();
    drop(text_manager);

    let image_manager = lock_arc_mutex(&image_manager_arc);
    let image_history = image_manager.get_history_preview();
    let image_categories = image_manager.get_categories();
    let image_category_list = image_manager.get_category_list();
    let image_tags = image_manager.get_image_tags();
    let image_pinned_items = image_manager.get_pinned_items();
    drop(image_manager);

    Ok(ClipboardFullSnapshot {
        text_history,
        text_categories,
        text_category_list,
        text_pinned_items,
        image_history,
        image_categories,
        image_category_list,
        image_tags,
        image_pinned_items,
    })
}

#[tauri::command]
pub async fn get_clipboard_history_page(
    request: ClipboardHistoryPageRequest,
) -> Result<ClipboardHistoryPageData, String> {
    let started_at = std::time::Instant::now();
    let result = load_history_page_data_async(
        request.offset,
        request.limit,
        request.category,
        request.pinned_only,
        request.keyword,
        request.sort_by,
        request.sort_order,
    )
    .await;
    match &result {
        Ok(_) => record_perf_metric(
            "text.history_page",
            "文本历史分页加载耗时",
            started_at.elapsed().as_millis() as u64,
            true,
            None,
        ),
        Err(error) => record_perf_metric(
            "text.history_page",
            "文本历史分页加载耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        ),
    }
    result
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageHistoryPageRequest {
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_image_page_limit")]
    limit: usize,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    keyword: Option<String>,
    #[serde(default)]
    pinned_only: bool,
    #[serde(default)]
    sort_by: Option<String>,
    #[serde(default)]
    sort_order: Option<String>,
}

fn default_image_page_limit() -> usize {
    50
}

#[tauri::command]
pub async fn set_item_category(
    item_id: String,
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .set_category_async(item_id, category)
        .await
        .map_err(|e| {
            to_frontend_error_string(
                AppError::new(ErrorCode::ClipboardError, "设置文本分类失败").with_details(e),
            )
        })
}

#[tauri::command]
pub async fn remove_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager.remove_category_async(category).await.map_err(|e| {
        to_frontend_error_string(
            AppError::new(ErrorCode::ClipboardError, "删除文本分类失败").with_details(e),
        )
    })
}

#[tauri::command]
pub async fn add_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager.add_category_async(category).await.map_err(|e| {
        to_frontend_error_string(
            AppError::new(ErrorCode::ClipboardError, "新增文本分类失败").with_details(e),
        )
    })
}

#[tauri::command]
pub async fn get_image_clipboard_history(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<ImageHistoryResponse, String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    Ok(ImageHistoryResponse {
        history: manager.get_history_preview(),
        categories: manager.get_categories(),
        category_list: manager.get_category_list(),
        image_tags: manager.get_image_tags(),
        pinned_items: manager.get_pinned_items(),
    })
}

#[tauri::command]
pub async fn close_image_preview_window(app: AppHandle) -> Result<(), String> {
    hide_image_preview_window(app);
    Ok(())
}

#[tauri::command]
pub async fn start_image_preview_window_drag(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("image_preview")
        .ok_or_else(|| "图片预览窗口不存在".to_string())?;
    window
        .start_dragging()
        .map_err(|e| format!("拖动窗口失败: {}", e))
}

#[tauri::command]
pub async fn warmup_image_clipboard_item_by_id(
    request: ItemIdRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_warmup_image_clipboard_item_by_id(request.item_id, state_arc)
            .map_err(to_frontend_error_string)
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "预热图片任务执行失败",
            e.to_string(),
        )
    })?
}

/// 优化方案 5：批量预热多个图片到内存缓存，用于滚动时提前加载
#[tauri::command]
pub async fn warmup_multiple_images(
    item_ids: Vec<String>,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let manager_arc = get_image_clipboard_manager_arc(&state_arc);
        let manager = lock_arc_mutex(&manager_arc);
        for item_id in item_ids {
            if let Some(index) = manager
                .get_history()
                .iter()
                .position(|item| item.id == item_id)
            {
                if index < 6 {
                    let _ = manager.warmup_image_by_id(&item_id);
                }
            }
        }
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "批量预热图片任务执行失败",
            e.to_string(),
        )
    })?;
    Ok(())
}

#[tauri::command]
pub async fn set_image_item_category(
    item_id: String,
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .set_category_async(item_id, category)
        .await
        .map_err(|e| {
            to_frontend_error_string(
                AppError::new(ErrorCode::ClipboardError, "设置图片分类失败").with_details(e),
            )
        })
}

#[tauri::command]
pub async fn remove_image_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager.remove_category_async(category).await.map_err(|e| {
        to_frontend_error_string(
            AppError::new(ErrorCode::ClipboardError, "删除图片分类失败").with_details(e),
        )
    })
}

#[tauri::command]
pub async fn set_image_item_tags(
    item_id: String,
    tags: Vec<String>,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager.set_tags_async(item_id, tags).await.map_err(|e| {
        to_frontend_error_string(
            AppError::new(ErrorCode::ClipboardError, "设置图片标签失败").with_details(e),
        )
    })
}

#[tauri::command]
pub async fn set_clipboard_item_pinned(
    index: Option<usize>,
    item_id: Option<String>,
    pinned: bool,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .set_pinned_by_selector_async(index, item_id, pinned)
        .await
        .map_err(|e| {
            if e == "索引超出范围" {
                to_frontend_error_string(AppError::new(ErrorCode::ValidationError, "索引超出范围"))
            } else {
                to_frontend_error_string(
                    AppError::new(ErrorCode::ClipboardError, "设置置顶状态失败").with_details(e),
                )
            }
        })
}

#[tauri::command]
pub async fn set_image_item_pinned(
    item_id: String,
    pinned: bool,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .set_pinned_async(item_id, pinned)
        .await
        .map_err(|e| {
            to_frontend_error_string(
                AppError::new(ErrorCode::ClipboardError, "设置图片置顶状态失败").with_details(e),
            )
        })
}

#[tauri::command]
pub async fn promote_clipboard_item(
    index: Option<usize>,
    item_id: Option<String>,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<String, String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .promote_to_top_async(index, item_id)
        .await
        .map(|item| crate::utils::database::stable_history_item_id(&item))
        .map_err(|e| {
            to_frontend_error_string(
                AppError::new(ErrorCode::ClipboardError, "置顶文本失败").with_details(e),
            )
        })
}

#[tauri::command]
pub async fn promote_image_clipboard_item_by_id(
    request: ItemIdRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_promote_image_clipboard_item_by_id(request.item_id, state_arc)
            .map_err(to_frontend_error_string)
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "置顶图片任务执行失败",
            e.to_string(),
        )
    })?
}

#[tauri::command]
pub async fn clear_text_history(
    mode: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<usize, String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .clear_history_by_mode_async(mode.as_str())
        .await
        .map_err(|e| {
            to_frontend_error_string(
                AppError::new(ErrorCode::ClipboardError, "清理文本历史失败").with_details(e),
            )
        })
}

#[tauri::command]
pub async fn clear_image_history(
    mode: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<usize, String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    let removed = manager
        .clear_history_by_mode_async(mode.as_str())
        .await
        .map_err(|e| {
            to_frontend_error_string(
                AppError::new(ErrorCode::ClipboardError, "清理图片历史失败").with_details(e),
            )
        })?;

    // 清理操作必须强制通知前端
    let is_visible = {
        let mut state_guard = lock_arc_mutex(state.inner());
        // 无论窗口是否可见，都标记为 dirty，确保下次打开时会重新加载
        state_guard.image_history_dirty = true;
        state_guard.is_image_visible
    };

    // 如果窗口当前可见，立即发送事件通知前端刷新
    if is_visible {
        emit_image_history_payload(&app, state.inner().clone());
    }

    Ok(removed)
}

#[tauri::command]
pub async fn import_image_files(
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<usize, String> {
    if paths.is_empty() {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "未选择任何文件或文件夹",
            "paths is empty",
        ));
    }
    let image_paths = collect_import_image_paths(paths)
        .map_err(|e| frontend_error(ErrorCode::IoError, "收集可导入图片路径失败", e))?;
    if image_paths.is_empty() {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "未找到可导入的图片",
            "collected image paths is empty",
        ));
    }
    let total = image_paths.len();
    let manager = {
        let state_guard = lock_arc_mutex(state.inner());
        let manager_guard = lock_arc_mutex(&state_guard.image_clipboard_manager);
        manager_guard.clone()
    };
    let _ = app.emit(
        "image-import-progress",
        serde_json::json!({
            "status": "start",
            "total": total,
            "processed": 0,
            "imported": 0,
            "failed": 0
        }),
    );
    let mut imported = 0usize;
    let mut failed = 0usize;
    let mut processed = 0usize;
    let mut last_error = String::new();
    for path in image_paths {
        match manager
            .import_local_image_paths_async(vec![path.clone()])
            .await
        {
            Ok(count) => {
                imported = imported.saturating_add(count);
            }
            Err(e) => {
                failed = failed.saturating_add(1);
                last_error = e;
            }
        }
        processed = processed.saturating_add(1);
        let _ = app.emit(
            "image-import-progress",
            serde_json::json!({
                "status": "progress",
                "total": total,
                "processed": processed,
                "imported": imported,
                "failed": failed
            }),
        );
    }
    let _ = app.emit(
        "image-import-progress",
        serde_json::json!({
            "status": "finish",
            "total": total,
            "processed": processed,
            "imported": imported,
            "failed": failed
        }),
    );
    if imported > 0 {
        emit_image_history_payload(&app, state.inner().clone());
    }
    if imported == 0 {
        if last_error.is_empty() {
            Err(frontend_error(
                ErrorCode::ClipboardError,
                "未导入任何图片",
                "imported == 0 and no detailed error",
            ))
        } else {
            Err(frontend_error(
                ErrorCode::ClipboardError,
                "导入图片失败",
                last_error,
            ))
        }
    } else {
        Ok(imported)
    }
}

#[tauri::command]
pub async fn count_import_image_files(paths: Vec<String>) -> Result<usize, String> {
    if paths.is_empty() {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "未选择任何文件或文件夹",
            "paths is empty",
        ));
    }
    let image_paths = collect_import_image_paths(paths)
        .map_err(|e| frontend_error(ErrorCode::IoError, "统计可导入图片路径失败", e))?;
    Ok(image_paths.len())
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct RecordingRegionSelectedPayload {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[tauri::command]
pub async fn notify_recording_region_selected(
    app: AppHandle,
    payload: RecordingRegionSelectedPayload,
) -> Result<(), String> {
    app.emit("recording-region-selected", payload)
        .map_err(|e| e.to_string())
}

fn collect_import_image_paths(entries: Vec<String>) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for raw in entries {
        let path = raw.trim();
        if path.is_empty() {
            continue;
        }
        let p = Path::new(path);
        if p.is_file() {
            if is_importable_image_file(p) {
                out.push(path.to_string());
            }
            continue;
        }
        if p.is_dir() {
            collect_images_from_dir(p, &mut out)?;
        }
    }
    Ok(out)
}

fn collect_images_from_dir(dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("读取目录失败: {}", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
        let path = entry.path();
        if path.is_dir() {
            collect_images_from_dir(&path, out)?;
        } else if path.is_file() && is_importable_image_file(&path) {
            out.push(path.to_string_lossy().to_string());
        }
    }
    Ok(())
}

fn is_importable_image_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase());
    matches!(
        ext.as_deref(),
        Some("png")
            | Some("jpg")
            | Some("jpeg")
            | Some("bmp")
            | Some("gif")
            | Some("webp")
            | Some("tif")
            | Some("tiff")
    )
}

#[tauri::command]
pub async fn add_image_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager.add_category_async(category).await.map_err(|e| {
        to_frontend_error_string(
            AppError::new(ErrorCode::ClipboardError, "新增图片分类失败").with_details(e),
        )
    })
}

#[tauri::command]
pub async fn get_clipboard_bottom_offset(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<i32, String> {
    let state_guard = lock_arc_mutex(state.inner());
    Ok(state_guard.settings.clipboard_bottom_offset)
}

#[tauri::command]
pub async fn preview_clipboard_bottom_offset(offset: i32, app: AppHandle) -> Result<(), String> {
    let final_offset = offset.max(0);
    if let Some(window) = app.get_webview_window("clipboard") {
        set_window_position(&window, final_offset);
    }
    if let Some(window) = app.get_webview_window("image_clipboard") {
        set_window_position(&window, final_offset);
    }
    Ok(())
}

#[tauri::command]
pub async fn save_clipboard_bottom_offset(
    offset: i32,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let final_offset = offset.clamp(0, 400);
    let mut settings = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.settings.clone()
    };
    settings.clipboard_bottom_offset = final_offset;
    save_settings(&settings).map_err(|e| e.to_string())?;

    {
        let mut state_guard = lock_arc_mutex(state.inner());
        state_guard.settings = settings;
    }

    if let Some(window) = app.get_webview_window("clipboard") {
        set_window_position(&window, final_offset);
    }
    if let Some(window) = app.get_webview_window("image_clipboard") {
        set_window_position(&window, final_offset);
    }
    Ok(())
}

#[tauri::command]
pub async fn select_and_fill(
    request: SelectAndFillRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<String, String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_select_and_fill_text(request, state_arc, app).map_err(to_frontend_error_string)
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "文本回填任务执行失败",
            e.to_string(),
        )
    })?
}

#[tauri::command]
pub async fn remove_clipboard_item(
    index: Option<usize>,
    item_id: Option<String>,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_remove_clipboard_item(index, item_id, state_arc, app).map_err(to_frontend_error_string)
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "删除文本历史任务执行失败",
            e.to_string(),
        )
    })?
}

#[tauri::command]
pub async fn remove_image_clipboard_item_by_id(
    item_id: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_remove_image_clipboard_item_by_id(item_id, state_arc, app)
            .map_err(to_frontend_error_string)
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "删除图片历史任务执行失败",
            e.to_string(),
        )
    })?
}

#[tauri::command]
pub async fn select_and_fill_image_by_id(
    request: SelectAndFillImageByIdRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_select_and_fill_image_by_id(request, state_arc, app)
            .map_err(to_frontend_error_string)
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "图片回填任务执行失败",
            e.to_string(),
        )
    })?
}

#[tauri::command]
pub async fn window_blur(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let is_visible = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.is_visible
    };
    if is_visible {
        let state_clone = state.inner().clone();
        hide_clipboard_window(app, state_clone);
    }
    Ok(())
}

#[tauri::command]
pub async fn image_window_blur(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let is_visible = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.is_image_visible
    };
    if is_visible {
        let state_clone = state.inner().clone();
        hide_image_clipboard_window(app, state_clone);
    }
    Ok(())
}

#[tauri::command]
pub async fn selection_toolbar_blur(app: AppHandle) -> Result<(), String> {
    let _ = hide_overlay_window_by_label(&app, "selection_toolbar");
    Ok(())
}

#[tauri::command]
pub async fn open_settings_window(
    tab: Option<String>,
    reason: Option<String>,
    app: AppHandle,
) -> Result<(), String> {
    open_settings(&app);
    if let Some(settings_window) = app.get_webview_window("settings") {
        let payload = serde_json::json!({
            "tab": tab.unwrap_or_else(|| "ai".to_string()),
            "reason": reason.unwrap_or_default()
        });
        let _ = settings_window.emit("navigate-settings-tab", payload);
    }
    Ok(())
}

fn register_recording_shortcut(
    app: &AppHandle,
    state: Arc<Mutex<SharedAppState>>,
    hot_key: &str,
) -> Result<(), String> {
    let app_clone = app.clone();
    app.global_shortcut()
        .on_shortcut(hot_key, move |_app, _shortcut, event| {
            if let ShortcutState::Pressed = event.state {
                let app_handle_inner = app_clone.clone();
                let state_inner = state.clone();
                tauri::async_runtime::spawn(async move {
                    toggle_recording_from_shortcut(app_handle_inner, state_inner).await;
                });
            }
        })
        .map_err(|e| frontend_error(ErrorCode::SystemError, "注册录屏快捷键失败", e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn show_selection_toolbar_with_text(
    app: AppHandle,
    text: String,
    x: i32,
    y: i32,
) -> Result<(), String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        log::warn!("show_selection_toolbar_with_text 收到空文本，忽略");
        return Ok(());
    }
    let content = trimmed.to_string();
    log::info!(
        "show_selection_toolbar_with_text: len={}, x={}, y={}",
        content.chars().count(),
        x,
        y
    );
    if app.get_webview_window("selection_toolbar").is_none() {
        let toolbar_window = tauri::WebviewWindowBuilder::new(
            &app,
            "selection_toolbar",
            tauri::WebviewUrl::App("selection_toolbar.html".into()),
        )
        .title("fuyun_tools")
        .visible(false)
        .resizable(false)
        .decorations(false)
        .shadow(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .build()
        .map_err(|e| format!("创建划词工具栏窗口失败: {}", e))?;
        bind_overlay_window_events(&toolbar_window, app.clone(), "selection_toolbar");
        log::info!("show_selection_toolbar_with_text: 已创建selection_toolbar窗口");
    }
    crate::ui::window_manager::show_selection_toolbar_force_impl(
        app.clone(),
        content.clone(),
        Some((x, y)),
    );
    if let Some(toolbar_window) = app.get_webview_window("selection_toolbar") {
        let payload =
            serde_json::to_string(&content).map_err(|e| format!("序列化文本失败: {}", e))?;
        let script = format!(
            "window.__SELECTION_TOOLBAR_TEXT__ = {payload}; window.dispatchEvent(new CustomEvent('selection-toolbar-text', {{ detail: {payload} }}));"
        );
        let _ = toolbar_window.eval(&script);
    }
    Ok(())
}

#[tauri::command]
pub async fn show_ocr_text_window(
    app: AppHandle,
    source_label: String,
    text: String,
) -> Result<(), String> {
    let content = text.trim().to_string();
    if content.is_empty() {
        return Ok(());
    }

    let source = app
        .get_webview_window(&source_label)
        .ok_or_else(|| "源窗口不存在".to_string())?;
    let source_pos = source
        .outer_position()
        .map_err(|e| format!("获取源窗口位置失败: {}", e))?;
    let source_size = source
        .outer_size()
        .map_err(|e| format!("获取源窗口尺寸失败: {}", e))?;
    let monitor = source
        .current_monitor()
        .map_err(|e| format!("获取显示器信息失败: {}", e))?
        .ok_or_else(|| "未找到显示器信息".to_string())?;

    let result_label = format!("ocr_text_{}", source_label.replace('-', "_"));
    let window = if let Some(existing) = app.get_webview_window(&result_label) {
        existing
    } else {
        let window = tauri::WebviewWindowBuilder::new(
            &app,
            result_label.clone(),
            tauri::WebviewUrl::App("ocr_text.html".into()),
        )
        .title("OCR识别结果")
        .visible(false)
        .decorations(false)
        .always_on_top(false)
        .resizable(true)
        .inner_size(560.0, 240.0)
        .build()
        .map_err(|e| format!("创建OCR结果窗口失败: {}", e))?;
        bind_overlay_window_events(&window, app.clone(), result_label.clone());
        window
    };

    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let target_width = (source_size.width as i32).min(monitor_size.width as i32);
    let target_height = 240i32;
    let gap = 8i32;
    let min_x = monitor_pos.x;
    let min_y = monitor_pos.y;
    let max_x = monitor_pos.x + monitor_size.width as i32 - target_width;
    let max_y = monitor_pos.y + monitor_size.height as i32 - target_height;

    let mut target_x = source_pos.x.clamp(min_x, max_x.max(min_x));
    let below_y = source_pos.y + source_size.height as i32 + gap;
    let above_y = source_pos.y - target_height - gap;
    let target_y = if below_y <= max_y {
        below_y
    } else if above_y >= min_y {
        above_y
    } else {
        below_y.clamp(min_y, max_y.max(min_y))
    };
    target_x = target_x.clamp(min_x, max_x.max(min_x));

    let _ = window.set_size(tauri::PhysicalSize::new(
        target_width as u32,
        target_height as u32,
    ));
    let _ = window.set_always_on_top(false);
    let _ = window.set_position(tauri::PhysicalPosition::new(target_x, target_y));
    let _ = show_overlay_window_by_label(&app, &result_label, true);

    let payload = serde_json::json!({"text": content});
    let script = format!(
        "window.__OCR_TEXT_PAYLOAD__ = {payload}; window.dispatchEvent(new CustomEvent('ocr-text-data', {{ detail: {payload} }}));"
    );
    let _ = window.eval(&script);
    Ok(())
}

#[tauri::command]
pub async fn get_ai_settings() -> Result<HashMap<String, serde_json::Value>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings()
            .map_err(|e| frontend_error(ErrorCode::ConfigError, "读取AI设置失败", e))?;

        // 转换为HashMap格式，便于前端处理
        let mut result = HashMap::new();

        // 添加基本设置
        result.insert(
            "version".to_string(),
            serde_json::Value::String(settings.version.clone()),
        );
        result.insert(
            "max_items".to_string(),
            serde_json::Value::Number(serde_json::Number::from(settings.max_items)),
        );
        result.insert(
            "text_max_items".to_string(),
            serde_json::Value::Number(serde_json::Number::from(settings.text_max_items)),
        );
        result.insert(
            "image_max_items".to_string(),
            serde_json::Value::Number(serde_json::Number::from(settings.image_max_items)),
        );
        result.insert(
            "image_disk_limit_mb".to_string(),
            serde_json::Value::Number(serde_json::Number::from(settings.image_disk_limit_mb)),
        );
        result.insert(
            "ai_provider".to_string(),
            serde_json::Value::String(settings.ai_provider.clone()),
        );
        result.insert(
            "hot_key".to_string(),
            serde_json::Value::String(settings.hot_key.clone()),
        );
        result.insert(
            "text_clipboard_enabled".to_string(),
            serde_json::Value::Bool(settings.text_clipboard_enabled),
        );
        result.insert(
            "image_hot_key".to_string(),
            serde_json::Value::String(settings.image_hot_key.clone()),
        );
        result.insert(
            "image_clipboard_enabled".to_string(),
            serde_json::Value::Bool(settings.image_clipboard_enabled),
        );
        result.insert(
            "screenshot_hot_key".to_string(),
            serde_json::Value::String(settings.screenshot_hot_key.clone()),
        );
        result.insert(
            "screenshot_enabled".to_string(),
            serde_json::Value::Bool(settings.screenshot_enabled),
        );
        result.insert(
            "recording_hot_key".to_string(),
            serde_json::Value::String(settings.recording_hot_key.clone()),
        );
        result.insert(
            "recording_mic_toggle_hot_key".to_string(),
            serde_json::Value::String(settings.recording_mic_toggle_hot_key.clone()),
        );
        result.insert(
            "recording_enabled".to_string(),
            serde_json::Value::Bool(settings.recording_enabled),
        );
        result.insert(
            "recording_default_fps".to_string(),
            serde_json::Value::Number(serde_json::Number::from(settings.recording_default_fps)),
        );
        result.insert(
            "recording_default_video_bitrate_kbps".to_string(),
            serde_json::Value::Number(serde_json::Number::from(
                settings.recording_default_video_bitrate_kbps,
            )),
        );
        result.insert(
            "recording_default_audio_bitrate_kbps".to_string(),
            serde_json::Value::Number(serde_json::Number::from(
                settings.recording_default_audio_bitrate_kbps,
            )),
        );
        result.insert(
            "recording_capture_cursor".to_string(),
            serde_json::Value::Bool(settings.recording_capture_cursor),
        );
        result.insert(
            "recording_capture_system_audio".to_string(),
            serde_json::Value::Bool(settings.recording_capture_system_audio),
        );
        result.insert(
            "recording_capture_microphone".to_string(),
            serde_json::Value::Bool(settings.recording_capture_microphone),
        );
        result.insert(
            "recording_microphone_device_id".to_string(),
            serde_json::Value::String(settings.recording_microphone_device_id.clone()),
        );
        result.insert(
            "recording_output_dir".to_string(),
            serde_json::Value::String(settings.recording_output_dir.clone()),
        );
        result.insert(
            "recording_auto_open_folder".to_string(),
            serde_json::Value::Bool(settings.recording_auto_open_folder),
        );
        result.insert(
            "recording_toolbar_content_protected".to_string(),
            serde_json::Value::Bool(settings.recording_toolbar_content_protected),
        );
        result.insert(
            "recording_max_duration_minutes".to_string(),
            serde_json::Value::Number(serde_json::Number::from(
                settings.recording_max_duration_minutes,
            )),
        );
        result.insert(
            "recording_file_name_template".to_string(),
            serde_json::Value::String(settings.recording_file_name_template.clone()),
        );
        result.insert(
            "recording_ffmpeg_download_url".to_string(),
            serde_json::Value::String(settings.recording_ffmpeg_download_url.clone()),
        );
        result.insert(
            "recording_window_audio_sync_advance_ms".to_string(),
            serde_json::Value::Number(serde_json::Number::from(
                settings.recording_window_audio_sync_advance_ms,
            )),
        );
        result.insert(
            "selection_enabled".to_string(),
            serde_json::Value::Bool(settings.selection_enabled),
        );
        result.insert(
            "grouped_items_protected_from_limit".to_string(),
            serde_json::Value::Bool(settings.grouped_items_protected_from_limit),
        );
        result.insert(
            "translation_prompt_template".to_string(),
            serde_json::Value::String(settings.translation_prompt_template.clone()),
        );
        result.insert(
            "explanation_prompt_template".to_string(),
            serde_json::Value::String(settings.explanation_prompt_template.clone()),
        );
        result.insert(
            "image_fill_verify_mode".to_string(),
            serde_json::Value::String(settings.image_fill_verify_mode.clone()),
        );

        // 处理provider_configs，将encrypted_api_key替换为解密后的api_key
        let mut provider_configs_map: HashMap<String, serde_json::Value> = HashMap::new();

        let provider_keys: Vec<String> = settings.provider_configs.keys().cloned().collect();

        for provider_key in provider_keys.iter() {
            if let Ok(api_key) = settings.get_provider_api_key(provider_key) {
                if let Some(decrypted_config) = settings.provider_configs.get(provider_key) {
                    let mut config_map = HashMap::new();
                    config_map.insert(
                        "api_url".to_string(),
                        serde_json::Value::String(decrypted_config.api_url.clone()),
                    );
                    config_map.insert(
                        "model_name".to_string(),
                        serde_json::Value::String(decrypted_config.model_name.clone()),
                    );
                    config_map.insert(
                        "api_key".to_string(),
                        serde_json::Value::String(if api_key.is_empty() {
                            "".to_string()
                        } else {
                            "********".to_string()
                        }),
                    );

                    provider_configs_map.insert(
                        provider_key.clone(),
                        serde_json::Value::Object(config_map.into_iter().collect()),
                    );
                }
            }
        }

        result.insert(
            "provider_configs".to_string(),
            serde_json::Value::Object(provider_configs_map.into_iter().collect()),
        );

        Ok(result)
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "读取AI设置任务执行失败",
            e.to_string(),
        )
    })?
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_text_dedup_metrics() -> Result<serde_json::Value, String> {
    serde_json::to_value(get_dedup_scan_metrics())
        .map_err(|e| frontend_error(ErrorCode::SystemError, "序列化去重指标失败", e.to_string()))
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_image_storage_metrics(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    let metrics = manager.get_storage_metrics();
    serde_json::to_value(metrics).map_err(|e| {
        to_frontend_error_string(
            AppError::new(ErrorCode::SystemError, "序列化图片存储指标失败")
                .with_details(e.to_string()),
        )
    })
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_copy_paste_dedup_debug_state() -> Result<serde_json::Value, String> {
    Ok(get_copy_paste_dedup_debug_state_value())
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_image_persist_queue_metrics() -> Result<serde_json::Value, String> {
    serde_json::to_value(get_image_persist_queue_metrics_snapshot()).map_err(|e| {
        to_frontend_error_string(
            AppError::new(ErrorCode::SystemError, "序列化图片持久化队列指标失败")
                .with_details(e.to_string()),
        )
    })
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn set_copy_paste_dedup_debug_config(
    enabled: Option<bool>,
    window_ms: Option<u64>,
    log_enabled: Option<bool>,
    reset_metrics: Option<bool>,
) -> Result<serde_json::Value, String> {
    if let Some(enabled) = enabled {
        COPY_PASTE_DEDUP_ENABLED.store(enabled, Ordering::Relaxed);
    }
    if let Some(window_ms) = window_ms {
        let clamped = window_ms.clamp(50, 10_000);
        COPY_PASTE_DEDUP_WINDOW_MS.store(clamped, Ordering::Relaxed);
        if let Some(lock) = COPY_PASTE_DEDUP_WINDOW_STATS.get() {
            let mut stats = lock.lock().unwrap_or_else(|poisoned| {
                log::warn!("复制粘贴去重窗口统计锁中毒，尝试恢复");
                poisoned.into_inner()
            });
            stats.window_start_ms = now_unix_ms();
            stats.requests = 0;
            stats.hits = 0;
        }
    }
    if let Some(log_enabled) = log_enabled {
        COPY_PASTE_DEDUP_LOG_ENABLED.store(log_enabled, Ordering::Relaxed);
    }
    if reset_metrics.unwrap_or(false) {
        COPY_PASTE_DEDUP_TOTAL_REQUESTS.store(0, Ordering::Relaxed);
        COPY_PASTE_DEDUP_HIT_COUNT.store(0, Ordering::Relaxed);
        COPY_PASTE_DEDUP_REQUEST_ID_HIT_COUNT.store(0, Ordering::Relaxed);
        COPY_PASTE_DEDUP_TEXT_HASH_HIT_COUNT.store(0, Ordering::Relaxed);
        COPY_PASTE_DEDUP_LOG_COUNT.store(0, Ordering::Relaxed);
        if let Some(lock) = COPY_PASTE_DEDUP_WINDOW_STATS.get() {
            let mut stats = lock.lock().unwrap_or_else(|poisoned| {
                log::warn!("复制粘贴去重窗口统计锁中毒，尝试恢复");
                poisoned.into_inner()
            });
            stats.window_start_ms = now_unix_ms();
            stats.requests = 0;
            stats.hits = 0;
            stats.last_hit_at_ms = 0;
        }
    }
    Ok(get_copy_paste_dedup_debug_state_value())
}

#[tauri::command]
pub async fn save_app_settings(
    text_max_items: Option<usize>,
    image_max_items: Option<usize>,
    image_disk_limit_mb: Option<u64>,
    ai_provider: Option<String>,
    ai_api_url: Option<String>,
    ai_model_name: Option<String>,
    ai_api_key: Option<String>,
    hot_key: Option<String>,
    image_hot_key: Option<String>,
    screenshot_hot_key: Option<String>,
    recording_hot_key: Option<String>,
    recording_mic_toggle_hot_key: Option<String>,
    text_clipboard_enabled: Option<bool>,
    image_clipboard_enabled: Option<bool>,
    screenshot_enabled: Option<bool>,
    recording_enabled: Option<bool>,
    selection_enabled: Option<bool>,
    grouped_items_protected_from_limit: Option<bool>,
    translation_prompt_template: Option<String>,
    explanation_prompt_template: Option<String>,
    image_fill_verify_mode: Option<String>,
    recording_default_fps: Option<u32>,
    recording_default_video_bitrate_kbps: Option<u32>,
    recording_default_audio_bitrate_kbps: Option<u32>,
    recording_capture_cursor: Option<bool>,
    recording_capture_system_audio: Option<bool>,
    recording_capture_microphone: Option<bool>,
    recording_microphone_device_id: Option<String>,
    recording_output_dir: Option<String>,
    recording_auto_open_folder: Option<bool>,
    recording_toolbar_content_protected: Option<bool>,
    recording_max_duration_minutes: Option<u32>,
    recording_file_name_template: Option<String>,
    recording_ffmpeg_download_url: Option<String>,
    recording_window_audio_sync_advance_ms: Option<u32>,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let version = app.package_info().version.to_string();

    let mut settings = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.settings.clone()
    };

    settings.version = version;

    // 部分更新：只更新传入的字段
    if let Some(val) = text_max_items {
        settings.max_items = val;
        settings.text_max_items = val;
    }
    if let Some(val) = image_max_items {
        settings.image_max_items = val;
    }
    if let Some(val) = image_disk_limit_mb {
        settings.image_disk_limit_mb = val;
    }
    if let Some(val) = text_clipboard_enabled {
        settings.text_clipboard_enabled = val;
    }
    if let Some(val) = image_clipboard_enabled {
        settings.image_clipboard_enabled = val;
    }
    if let Some(val) = screenshot_enabled {
        settings.screenshot_enabled = val;
    }
    if let Some(val) = recording_enabled {
        settings.recording_enabled = val;
    }
    if let Some(val) = selection_enabled {
        settings.selection_enabled = val;
    }
    if let Some(val) = grouped_items_protected_from_limit {
        settings.grouped_items_protected_from_limit = val;
    }
    if let Some(val) = translation_prompt_template {
        settings.translation_prompt_template = if val.trim().is_empty() {
            default_translation_prompt_template()
        } else {
            val
        };
    }
    if let Some(val) = explanation_prompt_template {
        settings.explanation_prompt_template = if val.trim().is_empty() {
            default_explanation_prompt_template()
        } else {
            val
        };
    }
    if let Some(val) = image_fill_verify_mode {
        settings.image_fill_verify_mode = if val == "fast" {
            "fast".to_string()
        } else {
            "strict".to_string()
        };
    }
    if let Some(val) = recording_default_fps {
        settings.recording_default_fps = val.clamp(1, 120);
    }
    if let Some(val) = recording_default_video_bitrate_kbps {
        settings.recording_default_video_bitrate_kbps = val.clamp(500, 50000);
    }
    if let Some(val) = recording_default_audio_bitrate_kbps {
        settings.recording_default_audio_bitrate_kbps = val.clamp(32, 512);
    }
    if let Some(val) = recording_capture_cursor {
        settings.recording_capture_cursor = val;
    }
    if let Some(val) = recording_capture_system_audio {
        settings.recording_capture_system_audio = val;
    }
    if let Some(val) = recording_capture_microphone {
        settings.recording_capture_microphone = val;
    }
    if let Some(val) = recording_microphone_device_id {
        settings.recording_microphone_device_id = val.trim().to_string();
    }
    if let Some(val) = recording_output_dir {
        settings.recording_output_dir = val.trim().to_string();
    }
    if let Some(val) = recording_auto_open_folder {
        settings.recording_auto_open_folder = val;
    }
    if let Some(val) = recording_toolbar_content_protected {
        settings.recording_toolbar_content_protected = val;
    }
    if let Some(val) = recording_max_duration_minutes {
        settings.recording_max_duration_minutes = val.clamp(1, 1440);
    }
    if let Some(val) = recording_file_name_template {
        settings.recording_file_name_template = if val.trim().is_empty() {
            "{timestamp}".to_string()
        } else {
            val
        };
    }
    if let Some(val) = recording_ffmpeg_download_url {
        settings.recording_ffmpeg_download_url = if val.trim().is_empty() {
            settings.recording_ffmpeg_download_url
        } else {
            val.trim().to_string()
        };
    }
    if let Some(val) = recording_window_audio_sync_advance_ms {
        settings.recording_window_audio_sync_advance_ms = val.clamp(0, 500);
    }

    // 处理快捷键更新
    if let Some(ref hot_key_val) = hot_key {
        if hot_key_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "快捷键不能为空",
                "hot_key is empty",
            ));
        }

        if hot_key_val != &settings.hot_key {
            let old_hot_key = settings.hot_key.clone();
            if settings.text_clipboard_enabled {
                register_text_shortcut(&app, state.inner().clone(), hot_key_val.as_str())?;
            }
            if let Err(e) = app.global_shortcut().unregister(old_hot_key.as_str()) {
                log::warn!(
                    "注销旧快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_hot_key,
                    e
                );
            }
            settings.hot_key = hot_key_val.clone();
        }
    }

    // 处理图片快捷键更新
    if let Some(ref image_hot_key_val) = image_hot_key {
        if image_hot_key_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "图片窗口快捷键不能为空",
                "image_hot_key is empty",
            ));
        }

        if image_hot_key_val != &settings.image_hot_key {
            // 检查是否与文字快捷键冲突
            if let Some(ref hot_key_val) = hot_key {
                if image_hot_key_val == hot_key_val {
                    return Err(frontend_error(
                        ErrorCode::ValidationError,
                        "文字与图片窗口快捷键不能相同",
                        format!(
                            "hot_key={}, image_hot_key={}",
                            hot_key_val, image_hot_key_val
                        ),
                    ));
                }
            } else if image_hot_key_val == &settings.hot_key {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    "文字与图片窗口快捷键不能相同",
                    format!(
                        "hot_key={}, image_hot_key={}",
                        settings.hot_key, image_hot_key_val
                    ),
                ));
            }

            let old_image_hot_key = settings.image_hot_key.clone();
            if settings.image_clipboard_enabled {
                register_image_shortcut(&app, state.inner().clone(), image_hot_key_val.as_str())?;
            }
            if let Err(e) = app.global_shortcut().unregister(old_image_hot_key.as_str()) {
                log::warn!(
                    "注销旧图片快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_image_hot_key,
                    e
                );
            }
            settings.image_hot_key = image_hot_key_val.clone();
        }
    }

    if let Some(ref screenshot_hot_key_val) = screenshot_hot_key {
        if screenshot_hot_key_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "截图快捷键不能为空",
                "screenshot_hot_key is empty",
            ));
        }

        if screenshot_hot_key_val != &settings.screenshot_hot_key {
            let effective_hot_key = hot_key.clone().unwrap_or_else(|| settings.hot_key.clone());
            let effective_image_hot_key = image_hot_key
                .clone()
                .unwrap_or_else(|| settings.image_hot_key.clone());
            if screenshot_hot_key_val == &effective_hot_key
                || screenshot_hot_key_val == &effective_image_hot_key
            {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    "截图快捷键不能与文字或图片窗口快捷键相同",
                    format!(
                        "hot_key={}, image_hot_key={}, screenshot_hot_key={}",
                        effective_hot_key, effective_image_hot_key, screenshot_hot_key_val
                    ),
                ));
            }

            if app
                .global_shortcut()
                .is_registered(screenshot_hot_key_val.as_str())
            {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    format!("截图快捷键被占用：{}", screenshot_hot_key_val),
                    "screenshot global shortcut already registered",
                ));
            }

            if let Err(e) = app
                .global_shortcut()
                .unregister(settings.screenshot_hot_key.as_str())
            {
                log::warn!(
                    "注销旧截图快捷键 '{}' 失败 (可能从未注册成功): {}",
                    settings.screenshot_hot_key,
                    e
                );
            }
            if settings.screenshot_enabled {
                register_screenshot_shortcut(&app, screenshot_hot_key_val.as_str())?;
            }
            settings.screenshot_hot_key = screenshot_hot_key_val.clone();
        }
    }

    if let Some(ref recording_hot_key_val) = recording_hot_key {
        if recording_hot_key_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "录屏快捷键不能为空",
                "recording_hot_key is empty",
            ));
        }
        if recording_hot_key_val != &settings.recording_hot_key {
            let effective_hot_key = hot_key.clone().unwrap_or_else(|| settings.hot_key.clone());
            let effective_image_hot_key = image_hot_key
                .clone()
                .unwrap_or_else(|| settings.image_hot_key.clone());
            let effective_screenshot_hot_key = screenshot_hot_key
                .clone()
                .unwrap_or_else(|| settings.screenshot_hot_key.clone());
            if recording_hot_key_val == &effective_hot_key
                || recording_hot_key_val == &effective_image_hot_key
                || recording_hot_key_val == &effective_screenshot_hot_key
            {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    "录屏快捷键不能与文字、图片或截图快捷键相同",
                    format!(
                        "hot_key={}, image_hot_key={}, screenshot_hot_key={}, recording_hot_key={}",
                        effective_hot_key,
                        effective_image_hot_key,
                        effective_screenshot_hot_key,
                        recording_hot_key_val
                    ),
                ));
            }

            if app
                .global_shortcut()
                .is_registered(recording_hot_key_val.as_str())
            {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    format!("录屏快捷键被占用：{}", recording_hot_key_val),
                    "recording global shortcut already registered",
                ));
            }

            if let Err(e) = app
                .global_shortcut()
                .unregister(settings.recording_hot_key.as_str())
            {
                log::warn!(
                    "注销旧录屏快捷键 '{}' 失败 (可能从未注册成功): {}",
                    settings.recording_hot_key,
                    e
                );
            }
            if settings.recording_enabled {
                register_recording_shortcut(
                    &app,
                    state.inner().clone(),
                    recording_hot_key_val.as_str(),
                )?;
            }
            settings.recording_hot_key = recording_hot_key_val.clone();
        }
    }

    // 处理麦克风切换快捷键更新
    if let Some(ref mic_toggle_hot_key_val) = recording_mic_toggle_hot_key {
        if mic_toggle_hot_key_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "麦克风切换快捷键不能为空",
                "recording_mic_toggle_hot_key is empty",
            ));
        }
        if mic_toggle_hot_key_val != &settings.recording_mic_toggle_hot_key {
            // 检查是否与其他快捷键冲突
            let effective_hot_key = hot_key.clone().unwrap_or_else(|| settings.hot_key.clone());
            let effective_image_hot_key = image_hot_key
                .clone()
                .unwrap_or_else(|| settings.image_hot_key.clone());
            let effective_screenshot_hot_key = screenshot_hot_key
                .clone()
                .unwrap_or_else(|| settings.screenshot_hot_key.clone());
            let effective_recording_hot_key = recording_hot_key
                .clone()
                .unwrap_or_else(|| settings.recording_hot_key.clone());

            if mic_toggle_hot_key_val == &effective_hot_key
                || mic_toggle_hot_key_val == &effective_image_hot_key
                || mic_toggle_hot_key_val == &effective_screenshot_hot_key
                || mic_toggle_hot_key_val == &effective_recording_hot_key
            {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    "麦克风切换快捷键不能与其他快捷键相同",
                    format!(
                        "hot_key={}, image_hot_key={}, screenshot_hot_key={}, recording_hot_key={}, mic_toggle_hot_key={}",
                        effective_hot_key,
                        effective_image_hot_key,
                        effective_screenshot_hot_key,
                        effective_recording_hot_key,
                        mic_toggle_hot_key_val
                    ),
                ));
            }

            if app
                .global_shortcut()
                .is_registered(mic_toggle_hot_key_val.as_str())
            {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    format!("麦克风切换快捷键被占用：{}", mic_toggle_hot_key_val),
                    "mic toggle global shortcut already registered",
                ));
            }

            // 注销旧快捷键
            if let Err(e) = app
                .global_shortcut()
                .unregister(settings.recording_mic_toggle_hot_key.as_str())
            {
                log::warn!(
                    "注销旧麦克风切换快捷键 '{}' 失败: {}",
                    settings.recording_mic_toggle_hot_key,
                    e
                );
            }

            // 注册新快捷键（按住开启，松开关闭）
            if settings.recording_enabled {
                let app_handle_for_mic = app.clone();
                if let Err(e) = app.global_shortcut().on_shortcut(
                    mic_toggle_hot_key_val.as_str(),
                    move |_app, _shortcut, event| {
                        let app_handle_inner = app_handle_for_mic.clone();
                        match event.state {
                            ShortcutState::Pressed => {
                                // 按下：开启麦克风
                                tauri::async_runtime::spawn(async move {
                                    toggle_microphone_from_shortcut(app_handle_inner, true).await;
                                });
                            }
                            ShortcutState::Released => {
                                // 松开：关闭麦克风
                                tauri::async_runtime::spawn(async move {
                                    toggle_microphone_from_shortcut(app_handle_inner, false).await;
                                });
                            }
                        }
                    },
                ) {
                    log::warn!(
                        "注册麦克风切换快捷键 '{}' 失败: {}",
                        mic_toggle_hot_key_val,
                        e
                    );
                    return Err(format!("注册麦克风切换快捷键失败: {}", e));
                }
            }

            settings.recording_mic_toggle_hot_key = mic_toggle_hot_key_val.clone();
        }
    }

    if let Some(enabled) = text_clipboard_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.hot_key.as_str())
            {
                register_text_shortcut(&app, state.inner().clone(), settings.hot_key.as_str())?;
            }
        } else if let Err(e) = app.global_shortcut().unregister(settings.hot_key.as_str()) {
            log::warn!("注销文字快捷键 '{}' 失败: {}", settings.hot_key, e);
        }
    }

    if let Some(enabled) = image_clipboard_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.image_hot_key.as_str())
            {
                register_image_shortcut(
                    &app,
                    state.inner().clone(),
                    settings.image_hot_key.as_str(),
                )?;
            }
        } else if let Err(e) = app
            .global_shortcut()
            .unregister(settings.image_hot_key.as_str())
        {
            log::warn!("注销图片快捷键 '{}' 失败: {}", settings.image_hot_key, e);
        }
    }

    if let Some(enabled) = screenshot_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.screenshot_hot_key.as_str())
            {
                register_screenshot_shortcut(&app, settings.screenshot_hot_key.as_str())?;
            }
        } else if let Err(e) = app
            .global_shortcut()
            .unregister(settings.screenshot_hot_key.as_str())
        {
            log::warn!(
                "注销截图快捷键 '{}' 失败: {}",
                settings.screenshot_hot_key,
                e
            );
        }
    }

    if let Some(enabled) = recording_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.recording_hot_key.as_str())
            {
                register_recording_shortcut(
                    &app,
                    state.inner().clone(),
                    settings.recording_hot_key.as_str(),
                )?;
            }
        } else if let Err(e) = app
            .global_shortcut()
            .unregister(settings.recording_hot_key.as_str())
        {
            log::warn!(
                "注销录屏快捷键 '{}' 失败: {}",
                settings.recording_hot_key,
                e
            );
        }
    }

    // 处理 AI 提供商更新
    if let Some(ref ai_provider_val) = ai_provider {
        if ai_provider_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "提供商名称不能为空",
                "ai_provider is empty",
            ));
        }
        settings.ai_provider = ai_provider_val.clone();

        // 处理 API 配置
        let mut need_update_config = false;
        let config = settings
            .provider_configs
            .entry(ai_provider_val.clone())
            .or_insert_with(|| {
                need_update_config = true;
                ProviderConfig::default()
            });

        if let Some(ref api_url) = ai_api_url {
            config.api_url = api_url.clone();
        }
        if let Some(ref model_name) = ai_model_name {
            config.model_name = model_name.clone();
        }

        // 处理 API 密钥
        if let Some(ref api_key) = ai_api_key {
            if api_key != "********" {
                settings
                    .save_current_provider_config(api_key)
                    .map_err(|e| frontend_error(ErrorCode::ConfigError, "保存提供商配置失败", e))?;

                if api_key.trim().is_empty() {
                    log::info!("提供商 {} 的API密钥已清空", ai_provider_val);
                } else {
                    match settings.get_provider_api_key(ai_provider_val) {
                        Ok(key) if key == *api_key => {
                            log::info!("密钥保存验证通过");
                        }
                        Ok(_) => {
                            log::warn!("密钥保存验证失败: 读取到的密钥与保存的不一致");
                            return Err(frontend_error(
                                ErrorCode::SystemError,
                                "系统凭据管理器异常: 密钥保存验证失败，请重试",
                                "saved key mismatch",
                            ));
                        }
                        Err(e) => {
                            log::error!("密钥保存验证错误: {}", e);
                            return Err(frontend_error(
                                ErrorCode::SystemError,
                                "系统凭据管理器错误: 无法读取刚保存的密钥",
                                e,
                            ));
                        }
                    }
                }
            }
        }
    }

    settings.migrate_from_old();

    settings
        .validate()
        .map_err(|e| frontend_error(ErrorCode::ValidationError, "设置验证失败", e))?;

    save_settings(&settings)
        .map_err(|e| frontend_error(ErrorCode::ConfigError, "保存设置失败", e))?;
    set_image_fill_verify_mode(&settings.image_fill_verify_mode);

    let selection_enabled = settings.selection_enabled;
    let text_clipboard_feature_enabled = settings.text_clipboard_enabled;
    let image_clipboard_feature_enabled = settings.image_clipboard_enabled;
    let screenshot_feature_enabled = settings.screenshot_enabled;
    let recording_feature_enabled = settings.recording_enabled;
    let (clipboard_manager_arc, image_manager_arc) = {
        let mut state_guard = lock_arc_mutex(state.inner());
        state_guard.settings = settings.clone();
        (
            state_guard.clipboard_manager.clone(),
            state_guard.image_clipboard_manager.clone(),
        )
    };
    {
        let mut manager = lock_arc_mutex(&clipboard_manager_arc);
        if let Some(val) = text_max_items {
            manager.set_max_items(val);
        }
        if let Some(val) = grouped_items_protected_from_limit {
            manager.set_grouped_items_protected_from_limit(val);
        }
    }
    {
        let mut manager = lock_arc_mutex(&image_manager_arc);
        if let Some(val) = image_max_items {
            manager.set_max_items(val);
        }
        if let Some(val) = image_disk_limit_mb {
            manager.set_disk_limit_mb(val);
        }
        if let Some(val) = grouped_items_protected_from_limit {
            manager.set_grouped_items_protected_from_limit(val);
        }
    }

    features::mouse_listener::set_selection_listener_enabled(
        app.clone(),
        state.inner().clone(),
        selection_enabled,
    );
    set_clipboard_listener_enabled(
        app.clone(),
        state.inner().clone(),
        text_clipboard_feature_enabled,
    );
    set_image_clipboard_listener_enabled(
        app.clone(),
        state.inner().clone(),
        image_clipboard_feature_enabled,
    );
    if !screenshot_feature_enabled {
        let _ = close_screenshot_window(app.clone()).await;
    }
    if !recording_feature_enabled {
        let _ = crate::features::recording::recorder_service::cancel_recording(
            &app,
            state.inner().clone(),
            crate::features::recording::types::SessionRequest { session_id: None },
        );
    }
    if let Some(content_protected) = recording_toolbar_content_protected {
        if let Some(window) = app.get_webview_window("recording_toolbar") {
            let _ = window.set_content_protected(content_protected);
        }
    }

    log::info!("设置保存成功（部分更新）");
    Ok(())
}

#[tauri::command]
pub async fn test_ai_connection(
    ai_provider: Option<String>,
    ai_api_url: String,
    ai_model_name: String,
    ai_api_key: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<String, String> {
    let mut real_api_key = ai_api_key;

    // 如果前端传过来的是脱敏的密钥，则从状态中获取真实的密钥
    if real_api_key == "********" {
        let (provider, settings_snapshot) = {
            let state_guard = lock_arc_mutex(state.inner());
            (
                ai_provider.unwrap_or_else(|| state_guard.settings.ai_provider.clone()),
                state_guard.settings.clone(),
            )
        };
        let key = settings_snapshot.get_provider_api_key(&provider);
        match key {
            Ok(key) if !key.is_empty() => {
                real_api_key = key;
            }
            _ => {
                return Err(frontend_error(
                    ErrorCode::ConfigError,
                    "未能获取到真实的 API 密钥",
                    "real api key not found",
                ));
            }
        }
    }

    let config = AIConfig {
        api_key: real_api_key,
        base_url: ai_api_url,
        model: ai_model_name,
    };

    let client = AIClient::new(config)
        .map_err(|e| frontend_error(ErrorCode::NetworkError, "客户端初始化失败", e.to_string()))?;

    match client.test_connection().await {
        Ok(success) => {
            if success {
                Ok("连接成功".to_string())
            } else {
                Err(frontend_error(
                    ErrorCode::NetworkError,
                    "连接测试未返回预期结果",
                    "test_connection returned false",
                ))
            }
        }
        Err(e) => {
            log::error!("AI连接测试失败: {}", e);
            Err(frontend_error(
                ErrorCode::NetworkError,
                "连接测试失败",
                e.to_string(),
            ))
        }
    }
}

#[tauri::command]
pub async fn copy_text(text: String, app: AppHandle) -> Result<(), String> {
    match app.clipboard().write_text(text) {
        Ok(()) => {
            log::info!("文本已复制到剪贴板");
            Ok(())
        }
        Err(e) => {
            let error_msg =
                frontend_error(ErrorCode::ClipboardError, "复制文本失败", e.to_string());
            log::error!("{}", error_msg);
            Err(error_msg)
        }
    }
}

#[tauri::command]
pub async fn copy_and_paste_text(
    text: String,
    request_id: Option<String>,
    app: AppHandle,
) -> Result<WriteBackExecutionResult, String> {
    let started_at = std::time::Instant::now();
    if is_duplicate_copy_paste_request(&text, request_id.as_deref()) {
        if COPY_PASTE_DEDUP_LOG_ENABLED.load(Ordering::Relaxed) {
            log::warn!("检测到短时重复回写请求，已跳过执行");
            COPY_PASTE_DEDUP_LOG_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        return Ok(WriteBackExecutionResult {
            source: "结果窗".to_string(),
            success: true,
            stage: "deduplicated".to_string(),
            target_window_title: String::new(),
            target_window_pid: 0,
            detail: "检测到重复回写请求，已跳过".to_string(),
            operation_id: None,
        });
    }
    let clipboard_started_at = std::time::Instant::now();
    app.clipboard().write_text(text).map_err(|e| {
        let error = frontend_error(ErrorCode::ClipboardError, "复制文本失败", e.to_string());
        record_writeback_stage_metric(
            "结果窗",
            "write_clipboard",
            "结果窗回写写入剪贴板耗时",
            clipboard_started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        record_writeback_stage_metric(
            "结果窗",
            "total",
            "结果窗回写总耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    record_writeback_stage_metric(
        "结果窗",
        "write_clipboard",
        "结果窗回写写入剪贴板耗时",
        clipboard_started_at.elapsed().as_millis() as u64,
        true,
        None,
    );

    let _ = hide_overlay_window_by_label(&app, "result_translation");
    let _ = hide_overlay_window_by_label(&app, "result_explanation");

    emit_writeback_phase(&app, "结果窗", "clipboard_written", None, None);
    emit_writeback_phase(&app, "结果窗", "pasting", None, None);
    let paste_started_at = std::time::Instant::now();
    let app_for_paste = app.clone();
    let paste_result = tauri::async_runtime::spawn_blocking(move || {
        simulate_paste_with_retry(&app_for_paste, "结果窗", None, started_at, false)
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "自动粘贴任务执行失败",
            e.to_string(),
        )
    })?;
    match paste_result {
        Ok(result) => {
            record_writeback_stage_metric(
                "结果窗",
                "paste",
                "结果窗回写粘贴耗时",
                paste_started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            record_writeback_stage_metric(
                "结果窗",
                "total",
                "结果窗回写总耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            emit_writeback_phase(
                &app,
                "结果窗",
                "completed",
                result.operation_id,
                Some(result.detail.clone()),
            );
            emit_writeback_result(&app, &result);
            Ok(result)
        }
        Err(result) => {
            record_writeback_stage_metric(
                "结果窗",
                "paste",
                "结果窗回写粘贴耗时",
                paste_started_at.elapsed().as_millis() as u64,
                false,
                Some(result.detail.clone()),
            );
            record_writeback_stage_metric(
                "结果窗",
                "total",
                "结果窗回写总耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(result.detail.clone()),
            );
            if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                log::warn!("复制后自动粘贴失败时兜底释放Ctrl失败: {}", release_error);
            }
            emit_writeback_phase(
                &app,
                "结果窗",
                "failed",
                result.operation_id,
                Some(result.detail.clone()),
            );
            emit_writeback_result(&app, &result);
            Err(frontend_error(
                ErrorCode::ClipboardError,
                "自动粘贴失败",
                result.detail,
            ))
        }
    }
}

#[tauri::command]
pub async fn get_provider_config(provider: AIProvider) -> Result<(String, String), String> {
    let (url, model) = provider.get_default_config();
    Ok((url, model))
}

#[tauri::command]
pub async fn remove_ai_provider(
    provider: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    if provider.is_empty() {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "提供商名称不能为空",
            "provider is empty",
        ));
    }

    let is_builtin = matches!(provider.as_str(), "deepseek" | "qwen" | "xiaomimimo");
    if is_builtin {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "内置提供商不支持删除",
            provider.clone(),
        ));
    }

    let mut settings = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.settings.clone()
    };

    if settings.provider_configs.remove(&provider).is_none() {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "未找到该提供商配置",
            provider.clone(),
        ));
    }

    if settings.ai_provider == provider {
        let fallback = "deepseek".to_string();
        if settings.provider_configs.contains_key(&fallback) {
            settings.ai_provider = fallback;
        } else if let Some(first_provider) = settings.provider_configs.keys().next() {
            settings.ai_provider = first_provider.clone();
        } else {
            settings.ai_provider = "deepseek".to_string();
        }
    }

    save_settings(&settings)
        .map_err(|e| frontend_error(ErrorCode::ConfigError, "保存设置失败", e))?;

    {
        let mut state_guard = lock_arc_mutex(state.inner());
        state_guard.settings = settings;
    }

    Ok(())
}

/// 获取所有已配置的提供商列表（包括自定义提供商）
#[tauri::command]
pub async fn get_all_configured_providers(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<Vec<(String, String)>, String> {
    let state_guard = lock_arc_mutex(state.inner());
    let settings = &state_guard.settings;

    let mut providers: Vec<(String, String)> = Vec::new();

    for provider_key in settings.provider_configs.keys() {
        providers.push((provider_key.clone(), provider_key.clone()));
    }

    Ok(providers)
}

/// 获取图片预览（优先使用已生成的，否则尝试从异步缓存获取）
#[tauri::command]
pub async fn get_image_preview_by_id(
    item_id: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<Option<(u32, u32, String)>, String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = lock_arc_mutex(&manager_arc);

    match manager.get_image_preview(&item_id) {
        Ok((width, height, base64)) => Ok(Some((width, height, base64))),
        Err(e) if e == "预览正在生成中" => Ok(None),
        Err(e) => Err(e),
    }
}

/// 批量检查预览是否已就绪
#[tauri::command]
pub async fn check_previews_ready(
    item_ids: Vec<String>,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<Vec<(String, bool)>, String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let manager_arc = get_image_clipboard_manager_arc(&state_arc);
        let manager = lock_arc_mutex(&manager_arc);

        let mut results = Vec::new();
        for item_id in item_ids {
            let ready = manager.is_image_preview_ready(&item_id);
            results.push((item_id, ready));
        }

        Ok(results)
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "检查预览状态任务执行失败",
            e.to_string(),
        )
    })?
}

// ========================================
// 截图相关命令
// ========================================

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualLongshotSessionRequest {
    session_id: u64,
}

#[tauri::command]
pub async fn check_vc_runtime_dependencies() -> Result<serde_json::Value, String> {
    #[cfg(windows)]
    {
        let win_dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
        let system32 = PathBuf::from(win_dir).join("System32");
        let app_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|x| x.to_path_buf()));
        let required = ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"];
        let missing: Vec<String> = required
            .iter()
            .filter_map(|name| {
                let in_system32 = system32.join(name).exists();
                let in_app_dir = app_dir
                    .as_ref()
                    .map(|dir| dir.join(name).exists())
                    .unwrap_or(false);
                if in_system32 || in_app_dir {
                    None
                } else {
                    Some((*name).to_string())
                }
            })
            .collect();
        #[cfg(debug_assertions)]
        if VC_RUNTIME_FORCE_MISSING.load(Ordering::Relaxed) {
            return Ok(serde_json::json!({
                "ok": false,
                "missing": required,
                "installUrl": "https://aka.ms/vs/17/release/vc_redist.x64.exe",
                "forcedByDev": true
            }));
        }
        return Ok(serde_json::json!({
            "ok": missing.is_empty(),
            "missing": missing,
            "installUrl": "https://aka.ms/vs/17/release/vc_redist.x64.exe",
            "forcedByDev": false
        }));
    }
    #[cfg(not(windows))]
    {
        Ok(serde_json::json!({
            "ok": true,
            "missing": [],
            "installUrl": ""
        }))
    }
}

/// 🔧 视频硬件加速编码器检测：自动检测可用的硬件编码器
/// 返回: Some("h264_nvenc") 或 Some("h264_qsv") 等，如果没有硬件编码器则返回 None
fn detect_video_hw_accel_encoder(ffmpeg_path: &std::path::Path) -> Option<String> {
    use std::process::Command;

    // 硬件编码器优先级：NVIDIA > Intel > AMD
    let encoders = ["h264_nvenc", "h264_qsv", "h264_amf"];

    let mut cmd = Command::new(ffmpeg_path);
    cmd.arg("-encoders");

    // 🔧 修复：隐藏控制台窗口，避免黑框一闪而过
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd.output().ok()?;

    let encoder_list = String::from_utf8_lossy(&output.stdout);

    for encoder in &encoders {
        if encoder_list.contains(encoder) {
            return Some(encoder.to_string());
        }
    }

    None
}

fn sanitize_settings_for_backup(
    settings: &crate::utils::settings_model::AppSettingsData,
) -> crate::utils::settings_model::AppSettingsData {
    let mut sanitized = settings.clone();
    for config in sanitized.provider_configs.values_mut() {
        config.encrypted_api_key.clear();
    }
    sanitized
}

async fn build_prepared_backup_data(
    state: &Arc<Mutex<SharedAppState>>,
) -> Result<PreparedBackupData, String> {
    let (settings, clipboard_manager_arc, image_manager_arc) = {
        let guard = state.lock().unwrap_or_else(|never| match never {});
        (
            sanitize_settings_for_backup(&guard.settings),
            guard.clipboard_manager.clone(),
            guard.image_clipboard_manager.clone(),
        )
    };

    let text_history = {
        let clipboard = lock_arc_mutex(&clipboard_manager_arc);
        crate::utils::database::ClipboardHistoryData {
            items: clipboard.get_history(),
            categories: clipboard.get_categories(),
            category_list: clipboard.get_category_list(),
            pinned_items: clipboard.get_pinned_items(),
        }
    };

    let (image_items, image_categories, image_category_list, image_tags, pinned_items) = {
        let manager = lock_arc_mutex(&image_manager_arc);
        (
            manager.get_history(),
            manager.get_categories(),
            manager.get_category_list(),
            manager.get_image_tags(),
            manager.get_pinned_items(),
        )
    };

    let mut warnings = vec!["API Key 不会被导出，恢复后需要重新填写".to_string()];
    let mut blobs = Vec::new();
    let mut backup_items = Vec::new();

    for item in image_items {
        if item.image_path.trim().is_empty() {
            warnings.push(format!("图片 {} 缺少实体文件路径，已跳过", item.id));
            continue;
        }
        let source = PathBuf::from(&item.image_path);
        if !source.exists() {
            warnings.push(format!("图片 {} 的实体文件不存在，已跳过", item.id));
            continue;
        }
        let metadata = fs::metadata(&source)
            .map_err(|e| format!("读取图片文件失败 {}: {}", source.display(), e))?;
        let extension = source
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("png");
        let package_path = format!("image_history/blobs/{}.{}", item.id, extension);
        backup_items.push(BackupImageHistoryItem {
            id: item.id.clone(),
            width: item.width,
            height: item.height,
            blob_path: format!("blobs/{}.{}", item.id, extension),
        });
        blobs.push(BackupBlobFile {
            item_id: item.id,
            source_path: source.to_string_lossy().to_string(),
            package_path,
            file_size: metadata.len(),
        });
    }

    let image_history = BackupImageHistoryFile {
        items: backup_items,
        categories: image_categories,
        category_list: image_category_list,
        image_tags,
        pinned_items,
    };

    let settings_bytes = serde_json::to_vec(&crate::utils::backup_model::BackupSettingsFile {
        settings: settings.clone(),
    })
    .map_err(|e| format!("序列化设置失败: {}", e))?;
    let text_bytes = serde_json::to_vec(&crate::utils::backup_model::BackupTextHistoryFile {
        snapshot: text_history.clone(),
    })
    .map_err(|e| format!("序列化文字历史失败: {}", e))?;
    let image_bytes =
        serde_json::to_vec(&image_history).map_err(|e| format!("序列化图片历史失败: {}", e))?;
    let blob_bytes = blobs.iter().map(|blob| blob.file_size).sum::<u64>();

    Ok(PreparedBackupData {
        settings,
        text_history: text_history.clone(),
        image_history: image_history.clone(),
        blobs,
        includes: crate::utils::backup_model::BackupIncludes {
            settings: true,
            text_history: true,
            image_history: true,
            image_blobs: true,
            api_keys: false,
            recordings: false,
        },
        stats: crate::utils::backup_model::BackupStats {
            text_item_count: text_history.items.len(),
            image_item_count: image_history.items.len(),
            image_blob_count: image_history.items.len(),
        },
        estimated_bytes: settings_bytes.len() as u64
            + text_bytes.len() as u64
            + image_bytes.len() as u64
            + blob_bytes,
        warnings,
    })
}

fn backup_preview_warnings_from_manifest(
    manifest: &crate::utils::backup_model::BackupManifest,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if !manifest.includes.api_keys {
        warnings.push("恢复后不会自动恢复 API Key".to_string());
    }
    if manifest.includes.image_history {
        warnings.push("图片预览缓存会在恢复后重新生成".to_string());
    }
    warnings
}

fn default_backup_file_name() -> String {
    format!("fuyun_tools_{}.fytbk.zip", now_unix_ms())
}

fn backup_frequency_interval_ms(frequency: &str) -> Option<i64> {
    match frequency {
        "daily" => Some(24 * 60 * 60 * 1000),
        "weekly" => Some(7 * 24 * 60 * 60 * 1000),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteBackExecutionResult {
    pub source: String,
    pub success: bool,
    pub stage: String,
    pub target_window_title: String,
    pub target_window_pid: u32,
    pub detail: String,
    pub operation_id: Option<u64>,
}

fn emit_writeback_result(app: &AppHandle, result: &WriteBackExecutionResult) {
    let slot = LAST_WRITEBACK_RESULT.get_or_init(|| StdMutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(result.clone());
    }
    let _ = app.emit("writeback-result", result);
}

fn list_backup_history_items(target_dir: &Path) -> Result<Vec<BackupHistoryItem>, String> {
    if !target_dir.exists() {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    for entry in fs::read_dir(target_dir).map_err(|e| format!("读取备份目录失败: {}", e))? {
        let entry = entry.map_err(|e| format!("读取备份目录项失败: {}", e))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !file_name.ends_with(".fytbk.zip") {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|e| format!("读取备份文件信息失败: {}", e))?;
        let created_at = metadata
            .modified()
            .unwrap_or_else(|_| SystemTime::now())
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        items.push(BackupHistoryItem {
            file_name: file_name.to_string(),
            file_path: path.to_string_lossy().to_string(),
            file_size_bytes: metadata.len(),
            created_at,
        });
    }
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(items)
}

fn current_backup_settings() -> Result<BackupSettingsData, String> {
    let settings = load_settings()?;
    Ok(BackupSettingsData {
        enabled: settings.backup_enabled,
        frequency: settings.backup_frequency,
        target_dir: settings.backup_target_dir,
        max_backup_count: settings.backup_max_count,
        last_run_at: settings.backup_last_run_at,
        last_run_status: settings.backup_last_run_status,
    })
}

fn update_backup_run_state(target_path: &str, status: &str) -> Result<(), String> {
    let mut settings = load_settings()?;
    if let Some(parent) = Path::new(target_path).parent() {
        settings.backup_target_dir = parent.to_string_lossy().to_string();
    }
    settings.backup_last_run_at = now_unix_ms() as i64;
    settings.backup_last_run_status = status.to_string();
    save_settings(&settings)
}

async fn export_backup_internal(
    target_path: &Path,
    state: &Arc<Mutex<SharedAppState>>,
) -> Result<BackupExportResultData, String> {
    let prepare_started_at = std::time::Instant::now();
    let prepared = build_prepared_backup_data(state).await.map_err(|error| {
        record_perf_metric(
            "backup.export_stage.prepare_data",
            "备份导出准备数据耗时",
            prepare_started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    record_perf_metric(
        "backup.export_stage.prepare_data",
        "备份导出准备数据耗时",
        prepare_started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    let temp_dir = create_backup_temp_dir()?;
    let app_version = {
        let guard = state.lock().unwrap_or_else(|never| match never {});
        guard.settings.version.clone()
    };
    let payload_started_at = std::time::Instant::now();
    let manifest_result = write_backup_payload(&temp_dir, &prepared, &app_version).await;
    if let Err(err) = manifest_result {
        record_perf_metric(
            "backup.export_stage.write_payload",
            "备份导出写入载荷耗时",
            payload_started_at.elapsed().as_millis() as u64,
            false,
            Some(err.clone()),
        );
        cleanup_dir(&temp_dir);
        return Err(err);
    }
    let manifest = manifest_result?;
    record_perf_metric(
        "backup.export_stage.write_payload",
        "备份导出写入载荷耗时",
        payload_started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    let zip_started_at = std::time::Instant::now();
    let zip_result = zip_backup_dir(&temp_dir, target_path).await;
    cleanup_dir(&temp_dir);
    let file_size_bytes = match zip_result {
        Ok(value) => {
            record_perf_metric(
                "backup.export_stage.zip_package",
                "备份导出打包压缩耗时",
                zip_started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            value
        }
        Err(error) => {
            record_perf_metric(
                "backup.export_stage.zip_package",
                "备份导出打包压缩耗时",
                zip_started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            );
            return Err(error);
        }
    };
    update_backup_run_state(&target_path.to_string_lossy(), "success")?;
    Ok(BackupExportResultData {
        file_path: target_path.to_string_lossy().to_string(),
        file_size_bytes,
        created_at: manifest.created_at,
        stats: manifest.stats,
    })
}

pub async fn run_auto_backup_tick(state: Arc<Mutex<SharedAppState>>) -> Result<bool, String> {
    let settings = current_backup_settings()?;
    if !settings.enabled {
        return Ok(false);
    }
    let Some(interval_ms) = backup_frequency_interval_ms(&settings.frequency) else {
        return Ok(false);
    };
    if settings.target_dir.trim().is_empty() {
        let mut raw_settings = load_settings()?;
        raw_settings.backup_last_run_at = now_unix_ms() as i64;
        raw_settings.backup_last_run_status = "misconfigured".to_string();
        save_settings(&raw_settings)?;
        return Err("自动备份目录未配置".to_string());
    }

    let now_ms = now_unix_ms() as i64;
    let due =
        settings.last_run_at <= 0 || now_ms.saturating_sub(settings.last_run_at) >= interval_ms;
    if !due {
        return Ok(false);
    }
    if AUTO_BACKUP_IN_FLIGHT.swap(true, Ordering::AcqRel) {
        return Ok(false);
    }
    let _backup_job_guard = BACKUP_JOB_MUTEX
        .get_or_init(|| tauri::async_runtime::Mutex::new(()))
        .lock()
        .await;
    let mut raw_settings = load_settings()?;
    raw_settings.backup_last_run_at = now_unix_ms() as i64;
    raw_settings.backup_last_run_status = "running".to_string();
    save_settings(&raw_settings)?;

    let run_result = async {
        let target_path = Path::new(&settings.target_dir).join(default_backup_file_name());
        let response = export_backup_internal(&target_path, &state).await?;
        let history_items = list_backup_history_items(Path::new(&settings.target_dir))?;
        if history_items.len() > settings.max_backup_count {
            for item in history_items.iter().skip(settings.max_backup_count) {
                let _ = fs::remove_file(&item.file_path);
            }
        }
        Ok::<BackupExportResultData, String>(response)
    }
    .await;

    AUTO_BACKUP_IN_FLIGHT.store(false, Ordering::Release);

    match run_result {
        Ok(_) => Ok(true),
        Err(err) => {
            let mut raw_settings = load_settings()?;
            raw_settings.backup_last_run_at = now_unix_ms() as i64;
            raw_settings.backup_last_run_status = "failed".to_string();
            save_settings(&raw_settings)?;
            Err(err)
        }
    }
}

async fn build_diagnostic_items_inner(
    state: &Arc<Mutex<SharedAppState>>,
) -> Result<Vec<DiagnosticItem>, String> {
    let checked_at = now_unix_ms() as i64;
    let (
        settings,
        image_manager_arc,
        active_overlay_window,
        last_overlay_lifecycle,
        overlay_lifecycle_history,
    ) = {
        let guard = state.lock().unwrap_or_else(|never| match never {});
        (
            guard.settings.clone(),
            guard.image_clipboard_manager.clone(),
            guard.active_overlay_window.clone(),
            guard.last_overlay_lifecycle.clone(),
            guard.overlay_lifecycle_history.clone(),
        )
    };
    let storage_metrics = {
        let manager = lock_arc_mutex(&image_manager_arc);
        manager.get_storage_metrics()
    };
    let queue_metrics = get_image_persist_queue_metrics_snapshot();
    let mut perf_metrics = get_perf_metrics_snapshot();
    let dedup_state = get_copy_paste_dedup_debug_state_value();
    let vc_runtime = check_vc_runtime_dependencies().await?;
    let longshot = get_manual_longshot_availability().await?;

    let memory_ratio = if storage_metrics.memory_budget_bytes == 0 {
        0.0
    } else {
        storage_metrics.memory_bytes as f64 / storage_metrics.memory_budget_bytes as f64
    };
    let disk_ratio = if storage_metrics.disk_limit_bytes == 0 {
        0.0
    } else {
        storage_metrics.disk_bytes as f64 / storage_metrics.disk_limit_bytes as f64
    };
    let image_storage_status = if memory_ratio >= 1.0 || disk_ratio >= 1.0 {
        "error"
    } else if memory_ratio >= 0.8 || disk_ratio >= 0.8 {
        "warning"
    } else {
        "healthy"
    };

    let persist_status = if queue_metrics.timeout_drop_count > 0 || queue_metrics.full_count > 20 {
        "error"
    } else if queue_metrics.full_count > 0 {
        "warning"
    } else {
        "healthy"
    };

    let vc_ok = vc_runtime
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let vc_missing = vc_runtime
        .get("missing")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let dependency_status = if vc_ok { "healthy" } else { "error" };
    let dependency_summary = if vc_ok {
        "运行依赖检查正常".to_string()
    } else {
        format!("VC Runtime 缺失 {} 项依赖", vc_missing.len())
    };

    let recording_status = if settings.dev_force_ffmpeg_window_capture {
        "warning"
    } else {
        "healthy"
    };
    let recording_summary = if settings.dev_force_ffmpeg_window_capture {
        "当前录屏处于强制 FFmpeg 降级模式".to_string()
    } else {
        "当前录屏主链路未强制降级".to_string()
    };

    // 🔧 性能优化：检测视频硬件加速编码器
    let hw_encoder_info =
        if let Ok(ffmpeg_path) = crate::features::recording::ffmpeg_runner::resolve_ffmpeg_path() {
            detect_video_hw_accel_encoder(&ffmpeg_path)
        } else {
            None
        };
    perf_metrics.sort_by(|a, b| {
        b.avg_duration_ms
            .partial_cmp(&a.avg_duration_ms)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.key.cmp(&b.key))
    });
    let perf_error_items = perf_metrics
        .iter()
        .filter(|item| item.last_status == "error")
        .collect::<Vec<_>>();
    let perf_slow_items = perf_metrics
        .iter()
        .filter(|item| perf_metric_is_slow(item))
        .collect::<Vec<_>>();
    let perf_status = if perf_metrics.is_empty() {
        "unknown"
    } else if !perf_error_items.is_empty() {
        "warning"
    } else if !perf_slow_items.is_empty() {
        "warning"
    } else {
        "healthy"
    };
    let perf_summary = if let Some(item) = perf_metrics.first() {
        format!(
            "已采样 {} 条链路，慢项 {} 条，异常 {} 条，当前平均最慢项 {} {:.0} ms",
            perf_metrics.len(),
            perf_slow_items.len(),
            perf_error_items.len(),
            item.label,
            item.avg_duration_ms
        )
    } else {
        "尚无性能采样记录，触发 OCR、AI 或截图保存后会出现数据".to_string()
    };
    let mut perf_grouped: BTreeMap<String, Vec<&crate::core::perf_metrics::PerfMetricSnapshot>> =
        BTreeMap::new();
    for item in &perf_metrics {
        perf_grouped
            .entry(perf_metric_group_label(&item.key).to_string())
            .or_default()
            .push(item);
    }
    let mut perf_group_summaries = perf_grouped.into_iter().collect::<Vec<_>>();
    perf_group_summaries.sort_by(|(left, _), (right, _)| {
        perf_metric_group_rank(left)
            .cmp(&perf_metric_group_rank(right))
            .then_with(|| left.cmp(right))
    });
    let mut perf_details = Vec::new();
    if !perf_error_items.is_empty() {
        perf_details.extend(perf_error_items.iter().take(3).map(|item| {
            format!(
                "[最近异常] {}: last {} ms / error {}",
                item.label,
                item.last_duration_ms,
                item.last_error
                    .clone()
                    .unwrap_or_else(|| "未知错误".to_string())
            )
        }));
    } else {
        perf_details.push("最近异常: 无".to_string());
    }
    if !perf_slow_items.is_empty() {
        perf_details.extend(perf_slow_items.iter().take(4).map(|item| {
            format!(
                "[慢项] {}: avg {:.0} ms / max {} ms / samples {}",
                item.label, item.avg_duration_ms, item.max_duration_ms, item.sample_count
            )
        }));
    } else if !perf_metrics.is_empty() {
        perf_details.push("慢项提示: 当前没有超阈值链路".to_string());
    }
    perf_details.extend(perf_group_summaries.into_iter().map(|(group, items)| {
        let slow_count = items
            .iter()
            .filter(|item| perf_metric_is_slow(item))
            .count();
        let error_count = items
            .iter()
            .filter(|item| item.last_status == "error")
            .count();
        let slowest = items
            .iter()
            .max_by(|left, right| {
                left.avg_duration_ms
                    .partial_cmp(&right.avg_duration_ms)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied();
        match slowest {
            Some(item) => format!(
                "[分组] {}: {} 条 / 慢项 {} / 异常 {} / 最慢 {} {:.0} ms",
                group,
                items.len(),
                slow_count,
                error_count,
                item.label,
                item.avg_duration_ms
            ),
            None => format!("[分组] {}: 0 条", group),
        }
    }));

    let window_hit_rate = dedup_state
        .get("metrics")
        .and_then(|value| value.get("windowHitRate"))
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    let dedup_enabled = dedup_state
        .get("enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let dedup_status = if !dedup_enabled {
        "warning"
    } else if window_hit_rate > 0.8 {
        "warning"
    } else {
        "healthy"
    };
    let dedup_summary = if !dedup_enabled {
        "回写去重已关闭".to_string()
    } else {
        format!("当前命中率 {:.1}%", window_hit_rate * 100.0)
    };
    let last_writeback = last_writeback_result();
    let writeback_status = match last_writeback.as_ref() {
        Some(item) if item.success => "healthy",
        Some(_) => "warning",
        None => "unknown",
    };
    let writeback_summary = match last_writeback.as_ref() {
        Some(item) if item.success => format!(
            "{} 最近一次回写成功{}",
            item.source,
            if item.target_window_title.is_empty() {
                String::new()
            } else {
                format!("，目标窗口 {}", item.target_window_title)
            }
        ),
        Some(item) => format!("{} 最近一次回写失败：{}", item.source, item.detail),
        None => "最近还没有回写执行记录".to_string(),
    };

    let longshot_status = match longshot.status.as_str() {
        "available" => "healthy",
        "busy" => "warning",
        "unavailable_missing_dependency" | "unavailable_runtime_error" => "error",
        _ => "unknown",
    };

    let mut longshot_details = longshot.details.clone();
    longshot_details.push(format!("当前阶段: {}", longshot.phase));
    if let Some(kind) = longshot.recent_failure_kind.as_ref() {
        longshot_details.push(format!("最近失败类型: {}", kind));
    }
    if let Some(message) = longshot.recent_failure_message.as_ref() {
        longshot_details.push(format!("最近失败原因: {}", message));
    }
    if let Some(at) = longshot.recent_failure_at {
        longshot_details.push(format!("最近失败时间: {}", at));
    }
    if longshot.status == "unavailable_missing_dependency" {
        longshot_details.push(
            "修复建议: 先确认 FFmpeg 可执行文件可用，再检查 longshot-opencv 构建能力".to_string(),
        );
    } else if longshot.status == "busy" {
        longshot_details.push("修复建议: 完成或取消当前长截图会话后再重试".to_string());
    } else if longshot.recent_failure_kind.as_deref() == Some("runtime_error") {
        longshot_details
            .push("修复建议: 重新开始一次长截图，若仍失败请打开诊断并检查最近失败原因".to_string());
    }

    let mut longshot_actions = vec![
        DiagnosticAction {
            key: "diagnostic.refresh".to_string(),
            label: "重新检查".to_string(),
            action_type: "refresh".to_string(),
            target: None,
        },
        DiagnosticAction {
            key: "longshot.open-settings".to_string(),
            label: "查看截图设置".to_string(),
            action_type: "open_settings".to_string(),
            target: Some("screenshot".to_string()),
        },
    ];
    if longshot.status == "unavailable_missing_dependency" {
        longshot_actions.push(DiagnosticAction {
            key: "longshot.show-help".to_string(),
            label: "查看修复说明".to_string(),
            action_type: "show_help".to_string(),
            target: None,
        });
        longshot_actions.push(DiagnosticAction {
            key: "longshot.download-ffmpeg".to_string(),
            label: "下载 FFmpeg".to_string(),
            action_type: "open_external".to_string(),
            target: None,
        });
        longshot_actions.push(DiagnosticAction {
            key: "longshot.show-build-help".to_string(),
            label: "查看构建要求".to_string(),
            action_type: "show_help".to_string(),
            target: None,
        });
    } else if longshot.recent_failure_kind.as_deref() == Some("runtime_error") {
        longshot_actions.push(DiagnosticAction {
            key: "longshot.show-runtime-help".to_string(),
            label: "查看失败说明".to_string(),
            action_type: "show_help".to_string(),
            target: None,
        });
    }

    Ok(vec![
        DiagnosticItem {
            key: "image-storage".to_string(),
            title: "图片存储占用".to_string(),
            status: image_storage_status.to_string(),
            summary: format!(
                "当前 {} 张图片，磁盘 {:.0}% / 内存 {:.0}%",
                storage_metrics.item_count,
                disk_ratio * 100.0,
                memory_ratio * 100.0
            ),
            details: vec![
                format!(
                    "磁盘占用 {} / {} 字节",
                    storage_metrics.disk_bytes, storage_metrics.disk_limit_bytes
                ),
                format!(
                    "内存占用 {} / {} 字节",
                    storage_metrics.memory_bytes, storage_metrics.memory_budget_bytes
                ),
                format!("置顶图片 {} 张", storage_metrics.pinned_count),
            ],
            actions: vec![
                DiagnosticAction {
                    key: "diagnostic.refresh".to_string(),
                    label: "刷新".to_string(),
                    action_type: "refresh".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "image-storage.open-settings".to_string(),
                    label: "打开设置".to_string(),
                    action_type: "open_settings".to_string(),
                    target: Some("clipboard".to_string()),
                },
            ],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "image-persist-queue".to_string(),
            title: "图片持久化队列".to_string(),
            status: persist_status.to_string(),
            summary: format!(
                "队列容量 {}，满队 {} 次，超时丢弃 {} 次",
                queue_metrics.queue_size,
                queue_metrics.full_count,
                queue_metrics.timeout_drop_count
            ),
            details: vec![
                format!("发送超时 {} ms", queue_metrics.send_timeout_ms),
                format!("重试间隔 {} ms", queue_metrics.retry_interval_ms),
                format!("平均等待 {:.1} ms", queue_metrics.avg_wait_ms),
            ],
            actions: vec![DiagnosticAction {
                key: "diagnostic.refresh".to_string(),
                label: "刷新".to_string(),
                action_type: "refresh".to_string(),
                target: None,
            }],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "dependencies".to_string(),
            title: "依赖检查状态".to_string(),
            status: dependency_status.to_string(),
            summary: dependency_summary,
            details: vec![
                format!("VC Runtime: {}", if vc_ok { "已就绪" } else { "缺失" }),
                format!(
                    "FFmpeg: {}",
                    if crate::features::recording::ffmpeg_runner::resolve_ffmpeg_path().is_ok() {
                        "已就绪"
                    } else {
                        "未检测到"
                    }
                ),
                if vc_missing.is_empty() {
                    "无缺失的 VC Runtime 组件".to_string()
                } else {
                    format!(
                        "缺失组件: {}",
                        vc_missing
                            .into_iter()
                            .filter_map(|value| value.as_str().map(|s| s.to_string()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
            ],
            actions: vec![
                DiagnosticAction {
                    key: "diagnostic.refresh".to_string(),
                    label: "重新检查".to_string(),
                    action_type: "refresh".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "dependencies.download-vc-runtime".to_string(),
                    label: "下载依赖".to_string(),
                    action_type: "download_dependency".to_string(),
                    target: vc_runtime
                        .get("installUrl")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string()),
                },
            ],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "recording-degrade".to_string(),
            title: "录屏降级状态".to_string(),
            status: recording_status.to_string(),
            summary: recording_summary,
            details: vec![
                format!(
                    "强制 FFmpeg 降级: {}",
                    if settings.dev_force_ffmpeg_window_capture {
                        "已开启"
                    } else {
                        "未开启"
                    }
                ),
                format!(
                    "录屏开关: {}",
                    if settings.recording_enabled {
                        "已启用"
                    } else {
                        "未启用"
                    }
                ),
                format!(
                    "视频硬件加速: {}",
                    match &hw_encoder_info {
                        Some(encoder) => format!("已检测到 {}", encoder),
                        None => "未检测到（使用软件编码）".to_string(),
                    }
                ),
            ],
            actions: vec![
                DiagnosticAction {
                    key: "diagnostic.refresh".to_string(),
                    label: "刷新".to_string(),
                    action_type: "refresh".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "recording-degrade.open-settings".to_string(),
                    label: "打开录屏设置".to_string(),
                    action_type: "open_settings".to_string(),
                    target: Some("recording".to_string()),
                },
            ],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "performance-metrics".to_string(),
            title: "关键链路性能观测".to_string(),
            status: perf_status.to_string(),
            summary: perf_summary,
            details: if perf_metrics.is_empty() {
                vec![
                    "当前还没有运行时性能采样".to_string(),
                    "可先触发 OCR、AI 翻译/解释、截图保存等链路".to_string(),
                ]
            } else {
                perf_details
            },
            actions: vec![
                DiagnosticAction {
                    key: "diagnostic.refresh".to_string(),
                    label: "刷新".to_string(),
                    action_type: "refresh".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "perf-metrics.reset".to_string(),
                    label: "清零采样".to_string(),
                    action_type: "reset_metrics".to_string(),
                    target: None,
                },
            ],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "overlay-window".to_string(),
            title: "覆盖层窗口状态".to_string(),
            status: if active_overlay_window.is_some() {
                "warning"
            } else {
                "healthy"
            }
            .to_string(),
            summary: match active_overlay_window.as_deref() {
                Some(label) => format!("当前活动覆盖层窗口: {}", label),
                None => "当前没有活动覆盖层窗口".to_string(),
            },
            details: vec![
                "用于观察工具栏、剪贴板窗、结果窗、预览窗的生命周期一致性".to_string(),
                match active_overlay_window.as_deref() {
                    Some(label) => format!("活动窗口标签: {}", label),
                    None => "活动窗口标签: 无".to_string(),
                },
                match last_overlay_lifecycle.as_ref() {
                    Some(item) => format!(
                        "最近动作: {} -> {} (focused={}, at={})",
                        item.label, item.action, item.focused, item.occurred_at
                    ),
                    None => "最近动作: 无".to_string(),
                },
            ]
            .into_iter()
            .chain(overlay_lifecycle_history.iter().rev().take(5).map(|item| {
                format!(
                    "历史: {} -> {} (focused={}, at={})",
                    item.label, item.action, item.focused, item.occurred_at
                )
            }))
            .collect(),
            actions: vec![DiagnosticAction {
                key: "diagnostic.refresh".to_string(),
                label: "刷新".to_string(),
                action_type: "refresh".to_string(),
                target: None,
            }],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "copy-paste-dedup".to_string(),
            title: "回写去重状态".to_string(),
            status: dedup_status.to_string(),
            summary: dedup_summary,
            details: vec![
                format!(
                    "总请求数 {}",
                    dedup_state["metrics"]["totalRequests"]
                        .as_u64()
                        .unwrap_or(0)
                ),
                format!(
                    "命中总数 {}",
                    dedup_state["metrics"]["hitCount"].as_u64().unwrap_or(0)
                ),
                format!(
                    "时间窗口 {} ms",
                    dedup_state["windowMs"].as_u64().unwrap_or(0)
                ),
            ],
            actions: vec![
                DiagnosticAction {
                    key: "diagnostic.refresh".to_string(),
                    label: "刷新".to_string(),
                    action_type: "refresh".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "copy-paste-dedup.reset-metrics".to_string(),
                    label: "清零计数".to_string(),
                    action_type: "reset_metrics".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "copy-paste-dedup.open-settings".to_string(),
                    label: "调整设置".to_string(),
                    action_type: "open_settings".to_string(),
                    target: Some("selection".to_string()),
                },
            ],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "writeback-flow".to_string(),
            title: "回写链路状态".to_string(),
            status: writeback_status.to_string(),
            summary: writeback_summary,
            details: match last_writeback.as_ref() {
                Some(item) => vec![
                    format!("来源 {}", item.source),
                    format!("阶段 {}", item.stage),
                    format!(
                        "目标窗口 {}",
                        if item.target_window_title.is_empty() {
                            "未知".to_string()
                        } else {
                            item.target_window_title.clone()
                        }
                    ),
                    format!("目标进程 PID {}", item.target_window_pid),
                ],
                None => vec![
                    "尚无最近回写结果".to_string(),
                    "可通过文字历史、图片历史或结果窗触发一次回写".to_string(),
                ],
            },
            actions: vec![
                DiagnosticAction {
                    key: "diagnostic.refresh".to_string(),
                    label: "刷新".to_string(),
                    action_type: "refresh".to_string(),
                    target: None,
                },
                DiagnosticAction {
                    key: "writeback-flow.open-settings".to_string(),
                    label: "查看划词设置".to_string(),
                    action_type: "open_settings".to_string(),
                    target: Some("selection".to_string()),
                },
            ],
            last_checked_at: checked_at,
        },
        DiagnosticItem {
            key: "longshot".to_string(),
            title: "长截图可用性状态".to_string(),
            status: longshot_status.to_string(),
            summary: longshot.summary,
            details: longshot_details,
            actions: longshot_actions,
            last_checked_at: checked_at,
        },
    ])
}

#[tauri::command]
pub async fn preview_backup_export(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<BackupExportPreviewResponse, String> {
    let started_at = std::time::Instant::now();
    let prepared = match build_prepared_backup_data(state.inner()).await {
        Ok(value) => {
            record_perf_metric(
                "backup.preview_export",
                "备份导出预览耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            value
        }
        Err(error) => {
            record_perf_metric(
                "backup.preview_export",
                "备份导出预览耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            );
            return Err(error);
        }
    };
    Ok(BackupExportPreviewResponse {
        success: true,
        message: "已生成导出预览".to_string(),
        data: BackupExportPreviewData {
            includes: prepared.includes,
            stats: prepared.stats,
            estimated_bytes: prepared.estimated_bytes,
            warnings: prepared.warnings,
        },
    })
}

#[tauri::command]
pub async fn export_backup_to_path(
    request: BackupExportRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<BackupExportResultResponse, String> {
    let started_at = std::time::Instant::now();
    let _backup_job_guard = BACKUP_JOB_MUTEX
        .get_or_init(|| tauri::async_runtime::Mutex::new(()))
        .lock()
        .await;
    let target = PathBuf::from(request.target_path);
    let result = export_backup_internal(&target, state.inner()).await;
    if let Err(err) = &result {
        let _ = update_backup_run_state(&target.to_string_lossy(), "failed");
        record_perf_metric(
            "backup.export",
            "备份导出耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(err.clone()),
        );
        return Err(err.clone());
    }
    record_perf_metric(
        "backup.export",
        "备份导出耗时",
        started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    Ok(BackupExportResultResponse {
        success: true,
        message: "备份导出成功".to_string(),
        data: result?,
    })
}

#[tauri::command]
pub async fn preview_backup_package(
    request: BackupPackagePreviewRequest,
) -> Result<BackupPackagePreviewResponse, String> {
    let started_at = std::time::Instant::now();
    let manifest = match read_manifest_from_package(Path::new(&request.package_path)) {
        Ok(value) => {
            record_perf_metric(
                "backup.preview_package",
                "备份包预览耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            value
        }
        Err(error) => {
            record_perf_metric(
                "backup.preview_package",
                "备份包预览耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            );
            return Err(error);
        }
    };
    Ok(BackupPackagePreviewResponse {
        success: true,
        message: "已读取备份包".to_string(),
        data: BackupPackagePreviewData {
            includes: manifest.includes.clone(),
            stats: manifest.stats.clone(),
            warnings: backup_preview_warnings_from_manifest(&manifest),
            restore_options: BackupRestoreOptions {
                can_restore_settings: manifest.includes.settings,
                can_restore_text_history: manifest.includes.text_history,
                can_restore_image_history: manifest.includes.image_history,
            },
            manifest,
        },
    })
}

#[tauri::command]
pub async fn restore_backup_package(
    request: BackupRestoreRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<BackupRestoreResultResponse, String> {
    let started_at = std::time::Instant::now();
    let _backup_job_guard = BACKUP_JOB_MUTEX
        .get_or_init(|| tauri::async_runtime::Mutex::new(()))
        .lock()
        .await;
    let result = match execute_restore_backup_package(state.inner().clone(), request).await {
        Ok(value) => {
            record_perf_metric(
                "backup.restore",
                "备份恢复耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            value
        }
        Err(error) => {
            record_perf_metric(
                "backup.restore",
                "备份恢复耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            );
            return Err(error);
        }
    };
    cleanup_dir(&result.extracted_dir);
    if let Some(rollback_dir) = &result.rollback_dir {
        cleanup_dir(rollback_dir);
    }
    Ok(BackupRestoreResultResponse {
        success: true,
        message: "备份恢复完成".to_string(),
        data: result.data,
    })
}

#[tauri::command]
pub async fn get_backup_settings() -> Result<BackupSettingsData, String> {
    current_backup_settings()
}

#[tauri::command]
pub async fn save_backup_settings(
    request: SaveBackupSettingsRequest,
) -> Result<BackupSettingsData, String> {
    let mut settings = load_settings()?;
    settings.backup_enabled = request.enabled;
    settings.backup_frequency = if request.frequency.trim().is_empty() {
        "weekly".to_string()
    } else {
        request.frequency.trim().to_string()
    };
    settings.backup_target_dir = request.target_dir.trim().to_string();
    settings.backup_max_count = request.max_backup_count.clamp(1, 50);
    save_settings(&settings)?;
    current_backup_settings()
}

#[tauri::command]
pub async fn list_backup_history() -> Result<Vec<BackupHistoryItem>, String> {
    let settings = current_backup_settings()?;
    if settings.target_dir.trim().is_empty() {
        return Ok(Vec::new());
    }
    list_backup_history_items(Path::new(&settings.target_dir))
}

#[tauri::command]
pub async fn delete_backup_history_item(
    request: DeleteBackupHistoryItemRequest,
) -> Result<(), String> {
    let settings = current_backup_settings()?;
    if settings.target_dir.trim().is_empty() {
        return Err("未配置备份目录".to_string());
    }
    let path = PathBuf::from(request.file_path);
    if !path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.ends_with(".fytbk.zip"))
        .unwrap_or(false)
    {
        return Err("仅允许删除 .fytbk.zip 备份文件".to_string());
    }
    let target_dir = PathBuf::from(settings.target_dir);
    let canonical_target_dir = target_dir
        .canonicalize()
        .map_err(|e| format!("读取备份目录失败: {}", e))?;
    if !path.exists() {
        return Ok(());
    }
    let canonical_path = path
        .canonicalize()
        .map_err(|e| format!("读取备份文件路径失败: {}", e))?;
    if !canonical_path.starts_with(&canonical_target_dir) {
        return Err("禁止删除备份目录之外的文件".to_string());
    }
    fs::remove_file(&canonical_path).map_err(|e| format!("删除备份文件失败: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn run_manual_backup(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<BackupExportResultResponse, String> {
    let started_at = std::time::Instant::now();
    let _backup_job_guard = BACKUP_JOB_MUTEX
        .get_or_init(|| tauri::async_runtime::Mutex::new(()))
        .lock()
        .await;
    let settings = current_backup_settings()?;
    if settings.target_dir.trim().is_empty() {
        return Err("请先配置自动备份目录".to_string());
    }
    let target_path = Path::new(&settings.target_dir).join(default_backup_file_name());
    let response = match export_backup_internal(&target_path, state.inner()).await {
        Ok(value) => {
            record_perf_metric(
                "backup.manual_export",
                "手动备份耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            value
        }
        Err(error) => {
            record_perf_metric(
                "backup.manual_export",
                "手动备份耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            );
            return Err(error);
        }
    };

    let history_items = list_backup_history_items(Path::new(&settings.target_dir))?;
    if history_items.len() > settings.max_backup_count {
        for item in history_items.iter().skip(settings.max_backup_count) {
            let _ = fs::remove_file(&item.file_path);
        }
    }

    Ok(BackupExportResultResponse {
        success: true,
        message: "手动备份完成".to_string(),
        data: response,
    })
}

#[tauri::command]
pub async fn get_manual_longshot_availability() -> Result<ManualLongshotAvailability, String> {
    #[cfg(not(feature = "longshot-opencv"))]
    {
        return Ok(ManualLongshotAvailability {
            status: "unavailable_missing_dependency".to_string(),
            phase: "idle".to_string(),
            summary: "当前构建未启用长截图依赖".to_string(),
            details: vec![
                "需要启用 longshot-opencv feature".to_string(),
                "默认构建未携带 OpenCV 长截图能力".to_string(),
                "该问题属于构建能力缺失，无法通过当前运行时自动修复".to_string(),
            ],
            session_id: None,
            recent_failure_kind: None,
            recent_failure_message: None,
            recent_failure_at: None,
        });
    }

    #[cfg(feature = "longshot-opencv")]
    {
        let recent_failure =
            crate::features::screenshot::longshot::get_last_manual_longshot_failure();
        let session_id = crate::features::screenshot::longshot::active_manual_longshot_session_id();
        if let Some(session_id) = session_id {
            let status =
                crate::features::screenshot::longshot::get_manual_longshot_status(session_id)
                    .map_err(|e| format!("读取长截图状态失败: {}", e))?;
            return Ok(ManualLongshotAvailability {
                status: "busy".to_string(),
                phase: status.phase.clone(),
                summary: status.user_message,
                details: vec![
                    format!("当前阶段: {}", status.phase),
                    "请先完成或取消当前长截图会话".to_string(),
                ],
                session_id: Some(session_id),
                recent_failure_kind: recent_failure
                    .as_ref()
                    .map(|item| item.failure_kind.clone()),
                recent_failure_message: recent_failure.as_ref().map(|item| item.message.clone()),
                recent_failure_at: recent_failure.as_ref().map(|item| item.occurred_at),
            });
        }
        match crate::features::recording::ffmpeg_runner::resolve_ffmpeg_path() {
            Ok(path) => Ok(ManualLongshotAvailability {
                status: "available".to_string(),
                phase: "idle".to_string(),
                summary: "长截图当前可用".to_string(),
                details: vec![
                    format!("已检测到 FFmpeg: {}", path.display()),
                    "当前构建已启用 longshot-opencv feature".to_string(),
                ],
                session_id: None,
                recent_failure_kind: recent_failure
                    .as_ref()
                    .map(|item| item.failure_kind.clone()),
                recent_failure_message: recent_failure.as_ref().map(|item| item.message.clone()),
                recent_failure_at: recent_failure.as_ref().map(|item| item.occurred_at),
            }),
            Err(err) => Ok(ManualLongshotAvailability {
                status: "unavailable_missing_dependency".to_string(),
                phase: "idle".to_string(),
                summary: "长截图依赖未就绪".to_string(),
                details: vec![
                    err,
                    "请先确保 FFmpeg 可执行文件可用，再重新检查".to_string(),
                    "若当前构建未携带 longshot-opencv feature，也需要切换到支持长截图的构建"
                        .to_string(),
                ],
                session_id: None,
                recent_failure_kind: recent_failure
                    .as_ref()
                    .map(|item| item.failure_kind.clone()),
                recent_failure_message: recent_failure.as_ref().map(|item| item.message.clone()),
                recent_failure_at: recent_failure.as_ref().map(|item| item.occurred_at),
            }),
        }
    }
}

#[tauri::command]
pub async fn get_diagnostic_items(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<Vec<DiagnosticItem>, String> {
    build_diagnostic_items_inner(state.inner()).await
}

#[tauri::command]
pub async fn get_diagnostic_overview(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<DiagnosticOverview, String> {
    let items = build_diagnostic_items_inner(state.inner()).await?;
    let error_count = items.iter().filter(|item| item.status == "error").count();
    let warning_count = items.iter().filter(|item| item.status == "warning").count();
    let overall_status = if error_count > 0 {
        "error"
    } else if warning_count > 0 {
        "warning"
    } else {
        "healthy"
    };
    Ok(DiagnosticOverview {
        overall_status: overall_status.to_string(),
        error_count,
        warning_count,
        checked_at: now_unix_ms() as i64,
    })
}

#[tauri::command]
pub async fn run_diagnostic_action(
    request: DiagnosticActionRequest,
) -> Result<DiagnosticActionResult, String> {
    match request.action_key.as_str() {
        "diagnostic.refresh" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "诊断状态已刷新".to_string(),
            needs_refresh: true,
            should_restart: false,
            navigate_to: None,
            external_url: None,
        }),
        "image-storage.open-settings" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "请检查剪贴板设置中的图片容量限制".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("clipboard".to_string()),
            external_url: None,
        }),
        "recording-degrade.open-settings" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "请检查录屏设置与依赖状态".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("recording".to_string()),
            external_url: None,
        }),
        "copy-paste-dedup.open-settings" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "请检查划词与回写设置".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("selection".to_string()),
            external_url: None,
        }),
        "longshot.open-settings" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "请检查截图设置与长截图能力".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("screenshot".to_string()),
            external_url: None,
        }),
        "longshot.show-help" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "长截图依赖修复建议：1) 先下载并配置 FFmpeg；2) 确认 ffmpeg 可在命令行直接执行；3) 若仍不可用，检查当前构建是否启用 longshot-opencv feature；4) 回到诊断页重新检查。".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("diagnostic".to_string()),
            external_url: None,
        }),
        "longshot.download-ffmpeg" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "已准备 FFmpeg 下载页".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: None,
            external_url: Some("https://ffmpeg.org/download.html".to_string()),
        }),
        "longshot.show-build-help" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "当前长截图除 FFmpeg 外，还要求构建启用 longshot-opencv feature。若诊断仍提示构建未启用，只能切换到支持长截图的构建产物。".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("diagnostic".to_string()),
            external_url: None,
        }),
        "longshot.show-runtime-help" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "长截图运行失败建议：重新开始一次长截图；若仍失败，优先检查滚动区域大小、依赖环境与最近失败原因。".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: Some("diagnostic".to_string()),
            external_url: None,
        }),
        "dependencies.download-vc-runtime" => Ok(DiagnosticActionResult {
            success: true,
            action_key: request.action_key,
            message: "已准备 VC Runtime 下载链接".to_string(),
            needs_refresh: false,
            should_restart: false,
            navigate_to: None,
            external_url: Some("https://aka.ms/vs/17/release/vc_redist.x64.exe".to_string()),
        }),
        "copy-paste-dedup.reset-metrics" => {
            COPY_PASTE_DEDUP_TOTAL_REQUESTS.store(0, Ordering::Relaxed);
            COPY_PASTE_DEDUP_HIT_COUNT.store(0, Ordering::Relaxed);
            COPY_PASTE_DEDUP_REQUEST_ID_HIT_COUNT.store(0, Ordering::Relaxed);
            COPY_PASTE_DEDUP_TEXT_HASH_HIT_COUNT.store(0, Ordering::Relaxed);
            COPY_PASTE_DEDUP_LOG_COUNT.store(0, Ordering::Relaxed);
            if let Some(lock) = COPY_PASTE_DEDUP_WINDOW_STATS.get() {
                let mut stats = lock.lock().unwrap();
                stats.window_start_ms = now_unix_ms();
                stats.requests = 0;
                stats.hits = 0;
                stats.last_hit_at_ms = 0;
            }
            Ok(DiagnosticActionResult {
                success: true,
                action_key: request.action_key,
                message: "回写去重计数已清零".to_string(),
                needs_refresh: true,
                should_restart: false,
                navigate_to: None,
                external_url: None,
            })
        }
        "perf-metrics.reset" => {
            reset_perf_metrics();
            Ok(DiagnosticActionResult {
                success: true,
                action_key: request.action_key,
                message: "性能采样已清零".to_string(),
                needs_refresh: true,
                should_restart: false,
                navigate_to: None,
                external_url: None,
            })
        }
        _ => Err(format!("不支持的诊断动作: {}", request.action_key)),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct VcRuntimeDownloadProgress {
    phase: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    progress_percent: Option<u8>,
    message: String,
}

fn normalize_sha256_hex(raw: &str) -> Option<String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(value)
    } else {
        None
    }
}

fn split_download_url_and_sha256(raw: &str) -> Result<(String, Option<String>), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("下载地址不能为空".to_string());
    }
    if let Some((url, fragment)) = trimmed.split_once("#sha256=") {
        let expected = normalize_sha256_hex(fragment)
            .ok_or_else(|| "下载地址中的 sha256 参数格式无效（应为64位十六进制）".to_string())?;
        return Ok((url.trim().to_string(), Some(expected)));
    }
    Ok((trimmed.to_string(), None))
}

fn compute_file_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("读取下载文件失败: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("读取下载文件失败: {}", e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        hex.push_str(format!("{:02x}", b).as_str());
    }
    Ok(hex)
}

fn verify_downloaded_exe_integrity(
    path: &Path,
    expected_sha256: Option<&str>,
) -> Result<(), String> {
    let mut header = [0u8; 2];
    let mut file = fs::File::open(path).map_err(|e| format!("读取下载文件失败: {}", e))?;
    file.read_exact(&mut header)
        .map_err(|e| format!("读取下载文件头失败: {}", e))?;
    if header != [b'M', b'Z'] {
        return Err("下载文件不是有效的 Windows 可执行文件".to_string());
    }
    if let Some(expected) = expected_sha256 {
        let actual = compute_file_sha256(path)?;
        if actual != expected {
            return Err(format!(
                "下载文件 SHA-256 校验失败，expected={}, actual={}",
                expected, actual
            ));
        }
    }
    Ok(())
}

fn validate_vc_runtime_installer_path(installer_path: &str) -> Result<PathBuf, String> {
    let raw = installer_path.trim();
    if raw.is_empty() {
        return Err("安装包路径不能为空".to_string());
    }
    let path = PathBuf::from(raw);
    if !path.exists() || !path.is_file() {
        return Err("安装包文件不存在，请重新下载".to_string());
    }
    let canonical = fs::canonicalize(&path).map_err(|e| format!("解析安装包路径失败: {}", e))?;
    let file_name = canonical
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if file_name != "vc_redist.x64.exe" {
        return Err("安装包文件名不合法，拒绝执行".to_string());
    }
    let allowed_root = fs::canonicalize(std::env::temp_dir().join("fuyun_tools"))
        .map_err(|e| format!("解析安装目录失败: {}", e))?;
    if !canonical.starts_with(&allowed_root) {
        return Err("安装包路径不在受信任目录，拒绝执行".to_string());
    }
    Ok(canonical)
}

#[tauri::command]
pub async fn download_vc_runtime_installer(
    download_url: Option<String>,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    #[cfg(windows)]
    {
        let default_url = "https://aka.ms/vs/17/release/vc_redist.x64.exe".to_string();
        let raw_url = download_url
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .unwrap_or(default_url);
        let (url, expected_sha256) = split_download_url_and_sha256(&raw_url)?;
        let parsed = reqwest::Url::parse(&url).map_err(|e| format!("下载地址无效: {}", e))?;
        if parsed.scheme() != "https" {
            return Err("下载地址必须使用 HTTPS".to_string());
        }
        let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
        if expected_sha256.is_none() && host != "aka.ms" {
            return Err("未提供 sha256 时，仅允许从 aka.ms 下载 VC Runtime".to_string());
        }
        let target_dir = std::env::temp_dir().join("fuyun_tools");
        fs::create_dir_all(&target_dir).map_err(|e| format!("创建目录失败: {}", e))?;
        let installer_path = target_dir.join("vc_redist.x64.exe");
        let tmp_path = target_dir.join("vc_redist.x64.exe.tmp");
        if tmp_path.exists() {
            let _ = fs::remove_file(&tmp_path);
        }

        let _ = app.emit(
            "vc-runtime-download-progress",
            VcRuntimeDownloadProgress {
                phase: "start".to_string(),
                downloaded_bytes: 0,
                total_bytes: None,
                progress_percent: Some(0),
                message: "开始下载 VC Runtime 安装包".to_string(),
            },
        );

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("下载请求失败: {}", e))?;
        if !response.status().is_success() {
            return Err(format!(
                "下载 VC Runtime 失败，HTTP 状态: {}",
                response.status()
            ));
        }
        let total_bytes = response.content_length();
        let mut downloaded_bytes: u64 = 0;
        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| format!("创建临时文件失败: {}", e))?;

        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res.map_err(|e| format!("下载数据流失败: {}", e))?;
            use tokio::io::AsyncWriteExt;
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("写入临时文件失败: {}", e))?;
            downloaded_bytes = downloaded_bytes.saturating_add(chunk.len() as u64);
            let progress_percent = total_bytes.and_then(|total| {
                if total == 0 {
                    None
                } else {
                    Some(((downloaded_bytes.saturating_mul(100)) / total).min(100) as u8)
                }
            });
            let _ = app.emit(
                "vc-runtime-download-progress",
                VcRuntimeDownloadProgress {
                    phase: "downloading".to_string(),
                    downloaded_bytes,
                    total_bytes,
                    progress_percent,
                    message: "正在下载 VC Runtime 安装包".to_string(),
                },
            );
        }
        file.flush()
            .await
            .map_err(|e| format!("刷新下载文件失败: {}", e))?;
        let metadata = fs::metadata(&tmp_path).map_err(|e| format!("读取下载文件失败: {}", e))?;
        if metadata.len() == 0 {
            let _ = fs::remove_file(&tmp_path);
            return Err("下载结果为空文件，请重试".to_string());
        }
        verify_downloaded_exe_integrity(&tmp_path, expected_sha256.as_deref()).inspect_err(
            |_| {
                let _ = fs::remove_file(&tmp_path);
            },
        )?;
        fs::rename(&tmp_path, &installer_path)
            .or_else(|_| {
                if installer_path.exists() {
                    let _ = fs::remove_file(&installer_path);
                }
                fs::rename(&tmp_path, &installer_path)
            })
            .map_err(|e| format!("写入安装包失败: {}", e))?;

        let _ = app.emit(
            "vc-runtime-download-progress",
            VcRuntimeDownloadProgress {
                phase: "completed".to_string(),
                downloaded_bytes,
                total_bytes,
                progress_percent: Some(100),
                message: "VC Runtime 安装包下载完成".to_string(),
            },
        );

        return Ok(serde_json::json!({
            "installerPath": installer_path.to_string_lossy().to_string(),
            "downloadUrl": url
        }));
    }
    #[cfg(not(windows))]
    {
        Err("当前平台不支持 VC Runtime 下载".to_string())
    }
}

#[tauri::command]
pub async fn open_vc_runtime_installer(installer_path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        let path = validate_vc_runtime_installer_path(&installer_path)?;
        std::process::Command::new(&path)
            .spawn()
            .map_err(|e| format!("启动安装程序失败: {}", e))?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        Err("当前平台不支持该操作".to_string())
    }
}

#[tauri::command]
pub async fn install_vc_runtime_and_wait(
    installer_path: String,
) -> Result<serde_json::Value, String> {
    #[cfg(windows)]
    {
        let path = validate_vc_runtime_installer_path(&installer_path)?;
        let status = tauri::async_runtime::spawn_blocking(move || {
            std::process::Command::new(&path)
                .arg("/install")
                .arg("/passive")
                .arg("/norestart")
                .status()
        })
        .await
        .map_err(|e| format!("启动安装程序失败: {}", e))?
        .map_err(|e| format!("执行安装程序失败: {}", e))?;
        let exit_code = status.code().unwrap_or(-1);
        let success = matches!(exit_code, 0 | 1638 | 3010);
        let cancelled = exit_code == 1602;
        let reboot_required = exit_code == 3010;
        return Ok(serde_json::json!({
            "success": success,
            "cancelled": cancelled,
            "rebootRequired": reboot_required,
            "exitCode": exit_code
        }));
    }
    #[cfg(not(windows))]
    {
        Err("当前平台不支持该操作".to_string())
    }
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_vc_runtime_debug_state() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "forceMissing": VC_RUNTIME_FORCE_MISSING.load(Ordering::Relaxed)
    }))
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn set_vc_runtime_debug_config(
    force_missing: Option<bool>,
) -> Result<serde_json::Value, String> {
    if let Some(enabled) = force_missing {
        VC_RUNTIME_FORCE_MISSING.store(enabled, Ordering::Relaxed);
    }
    Ok(serde_json::json!({
        "forceMissing": VC_RUNTIME_FORCE_MISSING.load(Ordering::Relaxed)
    }))
}

#[tauri::command]
pub async fn copy_image_clipboard_item_to_directory(
    item_id: String,
    target_directory: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let manager_arc = get_image_clipboard_manager_arc(&state_arc);
        let manager = lock_arc_mutex(&manager_arc);
        let source_path = manager.get_preview_image_path_by_id(&item_id)?;
        drop(manager);

        let source = PathBuf::from(&source_path);
        if !source.exists() {
            return Err("源图片文件不存在".to_string());
        }
        let file_name = source
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .ok_or_else(|| "无法解析源文件名".to_string())?
            .to_string();

        let target_dir = PathBuf::from(target_directory.trim());
        if target_dir.as_os_str().is_empty() {
            return Err("目标目录不能为空".to_string());
        }
        fs::create_dir_all(&target_dir).map_err(|e| format!("创建目标目录失败: {}", e))?;

        let stem = Path::new(&file_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image");
        let ext = Path::new(&file_name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("png");

        let mut target_path = target_dir.join(&file_name);
        if target_path.exists() {
            for idx in 1..10000u32 {
                let candidate = target_dir.join(format!("{} ({idx}).{}", stem, ext));
                if !candidate.exists() {
                    target_path = candidate;
                    break;
                }
            }
        }

        fs::copy(&source, &target_path).map_err(|e| format!("复制图片失败: {}", e))?;
        Ok(serde_json::json!({
            "success": true,
            "sourcePath": source.to_string_lossy(),
            "savedPath": target_path.to_string_lossy(),
        }))
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "复制图片任务执行失败",
            e.to_string(),
        )
    })?
}

/// 开始截图（全屏）
#[tauri::command]
pub async fn start_screenshot(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    use crate::features::screenshot::capture;
    if !is_screenshot_feature_enabled(state.inner()) {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "截图功能已停用",
            "screenshot feature disabled",
        ));
    }

    log::info!("开始全屏截图");

    match capture::capture_full_screen() {
        Ok((rgba, width, height, origin_x, origin_y)) => {
            let session_id = NEXT_SCREENSHOT_SESSION_ID.fetch_add(1, Ordering::SeqCst);
            let image_path = write_screenshot_boot_image(&rgba, width, height, session_id)
                .map_err(|e| format!("写入截图源图失败: {}", e))?;
            let png_base64 = capture::rgba_to_base64_png(&rgba, width, height)
                .map_err(|e| format!("转换PNG失败: {}", e))?;

            Ok(serde_json::json!({
                "success": true,
                "width": width,
                "height": height,
                "origin_x": origin_x,
                "origin_y": origin_y,
                "png_base64": png_base64,
                "image_path": image_path
            }))
        }
        Err(e) => {
            log::error!("截图失败: {}", e);
            Ok(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

#[tauri::command]
pub async fn start_manual_longshot(
    request: crate::features::screenshot::longshot::StartManualLongshotRequest,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    if !is_screenshot_feature_enabled(state.inner()) {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "截图功能已停用",
            "screenshot feature disabled",
        ));
    }
    // 在真正启动采样前先隐藏截图窗，避免首帧录入UI边框
    let _ = hide_overlay_window_by_label(&app, "screenshot");
    let _ = hide_overlay_window_by_label(&app, "longshot_border");
    tauri::async_runtime::spawn_blocking(|| {
        std::thread::sleep(std::time::Duration::from_millis(90))
    })
    .await
    .map_err(|e| format!("等待截图窗口隐藏失败: {}", e))?;
    crate::features::screenshot::longshot::start_manual_longshot(app, request)
}

#[tauri::command]
pub async fn pause_manual_longshot(
    request: ManualLongshotSessionRequest,
    app: AppHandle,
) -> Result<(), String> {
    crate::features::screenshot::longshot::pause_manual_longshot(request.session_id, app)
}

#[tauri::command]
pub async fn resume_manual_longshot(
    request: ManualLongshotSessionRequest,
    app: AppHandle,
) -> Result<(), String> {
    crate::features::screenshot::longshot::resume_manual_longshot(request.session_id, app)
}

#[tauri::command]
pub async fn cancel_manual_longshot(
    request: ManualLongshotSessionRequest,
    app: AppHandle,
) -> Result<(), String> {
    let session_id = request.session_id;
    tauri::async_runtime::spawn_blocking(move || {
        crate::features::screenshot::longshot::cancel_manual_longshot(session_id, app)
    })
    .await
    .map_err(|e| format!("取消长截图任务执行失败: {}", e))?
}

#[tauri::command]
pub async fn finish_manual_longshot(
    request: ManualLongshotSessionRequest,
    app: AppHandle,
) -> Result<crate::features::screenshot::longshot::ManualLongshotFinishResult, String> {
    let session_id = request.session_id;
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::features::screenshot::longshot::finish_manual_longshot(session_id, app)
    })
    .await
    .map_err(|e| format!("完成长截图任务执行失败: {}", e))??;
    if !result.image_path.is_empty() {
        replace_screenshot_boot_image_path(Some(PathBuf::from(&result.image_path)));
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_manual_longshot_status(
    request: ManualLongshotSessionRequest,
) -> Result<crate::features::screenshot::longshot::ManualLongshotStatus, String> {
    crate::features::screenshot::longshot::get_manual_longshot_status(request.session_id)
}

#[tauri::command]
pub async fn recognize_image_ocr(png_base64: String) -> Result<serde_json::Value, String> {
    let started_at = std::time::Instant::now();
    match crate::services::native_ocr::recognize_png_base64(&png_base64).await {
        Ok(result) => {
            record_perf_metric(
                "ocr.recognize",
                "OCR识别耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            Ok(serde_json::json!({
                "success": true,
                "paragraphs": result.paragraphs
            }))
        }
        Err(e) => {
            record_perf_metric(
                "ocr.recognize",
                "OCR识别耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(e.clone()),
            );
            Ok(serde_json::json!({
                "success": false,
                "error": e
            }))
        }
    }
}

/// 捕获指定区域
#[tauri::command]
pub async fn capture_region(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    use crate::features::screenshot::capture;
    if !is_screenshot_feature_enabled(state.inner()) {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "截图功能已停用",
            "screenshot feature disabled",
        ));
    }

    log::info!("捕获区域: ({}, {}) {}x{}", x, y, width, height);

    if width < 1 || height < 1 {
        return Ok(serde_json::json!({
            "success": false,
            "error": "区域尺寸无效"
        }));
    }

    match capture::capture_screen_region(x, y, width, height) {
        Ok((rgba, w, h)) => {
            let png_base64 = capture::rgba_to_base64_png(&rgba, w, h)
                .map_err(|e| format!("转换PNG失败: {}", e))?;

            Ok(serde_json::json!({
                "success": true,
                "width": w,
                "height": h,
                "png_base64": png_base64
            }))
        }
        Err(e) => {
            log::error!("区域截图失败: {}", e);
            Ok(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

/// 保存截图到文件
#[tauri::command]
pub async fn choose_screenshot_save_path(app: AppHandle) -> Result<serde_json::Value, String> {
    let filename = format!("screenshot_{}.png", now_unix_ms());
    let (tx, rx) = mpsc::channel::<Result<Option<PathBuf>, String>>();
    let screenshot_window = app.get_webview_window("screenshot");

    if let Some(window) = screenshot_window.as_ref() {
        let _ = window.set_always_on_top(false);
        let _ = window.set_ignore_cursor_events(true);
    }

    app.dialog()
        .file()
        .add_filter("PNG图片", &["png"])
        .set_file_name(&filename)
        .save_file(move |path| {
            let result = match path {
                Some(file_path) => file_path
                    .as_path()
                    .map(|p| Some(p.to_path_buf()))
                    .ok_or_else(|| "无法获取保存路径".to_string()),
                None => Ok(None),
            };
            let _ = tx.send(result);
        });

    let selected_path_result = tauri::async_runtime::spawn_blocking(move || rx.recv()).await;

    if let Some(window) = screenshot_window.as_ref() {
        let _ = window.set_always_on_top(true);
        let _ = window.set_ignore_cursor_events(false);
        let _ = focus_overlay_window_by_label(&app, "screenshot");
    }

    let selected_path = selected_path_result
        .map_err(|e| format!("等待保存对话框结果失败: {}", e))?
        .map_err(|e| format!("接收保存对话框结果失败: {}", e))??;

    let Some(path_buf) = selected_path else {
        return Ok(serde_json::json!({
            "success": false,
            "cancelled": true,
            "message": "用户取消保存"
        }));
    };

    Ok(serde_json::json!({
        "success": true,
        "cancelled": false,
        "path": path_buf.to_string_lossy().to_string()
    }))
}

#[tauri::command]
pub async fn save_screenshot_to_path(
    png_base64: String,
    output_path: String,
) -> Result<serde_json::Value, String> {
    use base64::Engine;

    if output_path.trim().is_empty() {
        return Err("保存路径为空".to_string());
    }

    let target_path = PathBuf::from(&output_path);
    timed_sync("screenshot.save_file", "截图保存耗时", || {
        let png_data = base64::engine::general_purpose::STANDARD
            .decode(&png_base64)
            .map_err(|e| format!("Base64解码失败: {}", e))?;

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建保存目录失败: {}", e))?;
        }

        fs::write(&target_path, &png_data).map_err(|e| format!("写入文件失败: {}", e))?;
        Ok::<(), String>(())
    })?;

    Ok(serde_json::json!({
        "success": true,
        "path": target_path.to_string_lossy().to_string()
    }))
}

#[tauri::command]
pub async fn export_screenshot_to_path(
    request: ScreenshotExportRequest,
) -> Result<serde_json::Value, String> {
    let output_path = request.output_path.clone();
    tauri::async_runtime::spawn_blocking(move || export_screenshot_image(&request))
        .await
        .map_err(|e| format!("执行截图导出任务失败: {}", e))?
        .map(|_| {
            serde_json::json!({
                "success": true,
                "path": output_path
            })
        })
}

#[tauri::command]
pub async fn render_screenshot_to_png_data(
    request: ScreenshotExportRequest,
) -> Result<serde_json::Value, String> {
    let (rgba, width, height) = tauri::async_runtime::spawn_blocking(move || {
        let canvas = render_screenshot_image(&request)?;
        let width = canvas.width();
        let height = canvas.height();
        Ok::<(Vec<u8>, u32, u32), String>((canvas.into_raw(), width, height))
    })
    .await
    .map_err(|e| format!("执行截图渲染任务失败: {}", e))??;
    let png_base64 = crate::features::screenshot::capture::rgba_to_base64_png(&rgba, width, height)
        .map_err(|e| format!("转换PNG失败: {}", e))?;
    Ok(serde_json::json!({
        "success": true,
        "pngBase64": png_base64,
        "width": width,
        "height": height
    }))
}

#[tauri::command]
pub async fn copy_screenshot_to_clipboard(
    request: ScreenshotExportRequest,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    let (rgba, width, height) = tauri::async_runtime::spawn_blocking(move || {
        let canvas = render_screenshot_image(&request)?;
        let width = canvas.width();
        let height = canvas.height();
        Ok::<(Vec<u8>, u32, u32), String>((canvas.into_raw(), width, height))
    })
    .await
    .map_err(|e| format!("执行截图渲染任务失败: {}", e))??;
    let image = tauri::image::Image::new_owned(rgba, width, height);
    ImageClipboardManager::write_clipboard_image(&app, &image)?;
    Ok(serde_json::json!({
        "success": true,
        "width": width,
        "height": height
    }))
}

#[tauri::command]
pub async fn save_screenshot(
    png_base64: String,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    use base64::Engine;
    use std::time::{SystemTime, UNIX_EPOCH};
    let started_at = std::time::Instant::now();

    log::info!("保存截图到文件");

    // 解码Base64
    let png_data = base64::engine::general_purpose::STANDARD
        .decode(&png_base64)
        .map_err(|e| format!("Base64解码失败: {}", e))?;

    // 生成文件名
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let filename = format!("screenshot_{}.png", timestamp);

    let (tx, rx) = mpsc::channel::<Result<Option<PathBuf>, String>>();

    // 获取保存路径（用户选择）
    app.dialog()
        .file()
        .add_filter("PNG图片", &["png"])
        .set_file_name(&filename)
        .save_file(move |path| {
            let result = match path {
                Some(file_path) => file_path
                    .as_path()
                    .map(|p| Some(p.to_path_buf()))
                    .ok_or_else(|| "无法获取保存路径".to_string()),
                None => Ok(None),
            };
            let _ = tx.send(result);
        });

    let selected_path = tauri::async_runtime::spawn_blocking(move || rx.recv())
        .await
        .map_err(|e| format!("等待保存对话框结果失败: {}", e))?
        .map_err(|e| format!("接收保存对话框结果失败: {}", e))??;

    let Some(path_buf) = selected_path else {
        log::info!("用户取消保存");
        record_perf_metric(
            "screenshot.save_dialog",
            "截图保存对话框总耗时",
            started_at.elapsed().as_millis() as u64,
            true,
            None,
        );
        return Ok(serde_json::json!({
            "success": false,
            "cancelled": true,
            "message": "用户取消保存"
        }));
    };

    fs::write(&path_buf, &png_data).map_err(|e| format!("写入文件失败: {}", e))?;
    record_perf_metric(
        "screenshot.save_dialog",
        "截图保存对话框总耗时",
        started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    log::info!("截图已保存到: {}", path_buf.display());

    Ok(serde_json::json!({
        "success": true,
        "cancelled": false,
        "path": path_buf.to_string_lossy().to_string()
    }))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinScreenshotRequest {
    png_base64: String,
    x: Option<f64>,
    y: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
}

#[tauri::command]
pub async fn pin_screenshot_on_screen(
    request: PinScreenshotRequest,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    let label = format!(
        "pinned_image_{}",
        NEXT_PINNED_IMAGE_WINDOW_ID.fetch_add(1, Ordering::Relaxed)
    );
    let x = request.x.unwrap_or(100.0).max(0.0);
    let y = request.y.unwrap_or(100.0).max(0.0);
    let width = request.width.unwrap_or(360.0).max(1.0);
    let height = request.height.unwrap_or(240.0).max(1.0);
    let payload = serde_json::json!({
        "label": label,
        "png_base64": request.png_base64,
        "width": width,
        "height": height
    });
    let payload_init_script = format!("window.__PINNED_IMAGE_PAYLOAD__ = {};", payload);
    let window = tauri::WebviewWindowBuilder::new(
        &app,
        label.clone(),
        tauri::WebviewUrl::App("pinned_image.html".into()),
    )
    .title("固定截图")
    .visible(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(true)
    .initialization_script(&payload_init_script)
    .build()
    .map_err(|e| format!("创建固定图片窗口失败: {}", e))?;
    bind_overlay_window_events(&window, app.clone(), label.clone());

    let window_clone = window.clone();
    let _ = window_clone.set_resizable(true);
    let _ = window_clone.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
    let _ = window_clone.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
    let _ = show_overlay_window_by_label(&app, &label, false);
    let script = format!(
        "window.__PINNED_IMAGE_PAYLOAD__ = {}; window.dispatchEvent(new CustomEvent('pinned-image-data', {{ detail: {} }}));",
        payload, payload
    );
    let _ = window_clone.eval(script);

    Ok(serde_json::json!({ "success": true, "label": label }))
}

#[tauri::command]
pub async fn close_pinned_image_window(label: String, app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.close();
    }
    Ok(())
}

#[tauri::command]
pub async fn get_pinned_image_window_position(
    label: String,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    if let Some(window) = app.get_webview_window(&label) {
        if let Ok(pos) = window.outer_position() {
            return Ok(serde_json::json!({
                "success": true,
                "x": pos.x,
                "y": pos.y
            }));
        }
    }
    Ok(serde_json::json!({
        "success": false
    }))
}

#[tauri::command]
pub async fn move_pinned_image_window(
    label: String,
    x: i32,
    y: i32,
    app: AppHandle,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
    }
    Ok(())
}

/// 获取屏幕尺寸
#[tauri::command]
pub async fn get_screen_size() -> Result<serde_json::Value, String> {
    use crate::features::screenshot::capture;

    match capture::get_screen_size() {
        Ok((width, height)) => Ok(serde_json::json!({
            "success": true,
            "width": width,
            "height": height
        })),
        Err(e) => Ok(serde_json::json!({
            "success": false,
            "error": e.to_string()
        })),
    }
}

#[tauri::command]
pub async fn set_screenshot_clipboard_link_once(linked: bool) -> Result<(), String> {
    use crate::features::screenshot::capture;
    if let Ok(settings) = load_settings() {
        if !settings.screenshot_enabled {
            return Ok(());
        }
    }
    capture::set_allow_image_clipboard_once(linked);
    Ok(())
}

fn set_screenshot_window_passthrough_internal(
    app: &AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let Some(window) = app.get_webview_window("screenshot") else {
        return Ok(());
    };
    window
        .set_ignore_cursor_events(enabled)
        .map_err(|e| format!("设置截图窗口输入穿透失败: {}", e))?;
    if !enabled {
        let _ = focus_overlay_window_by_label(&app, "screenshot");
    }
    Ok(())
}

fn set_screenshot_window_visibility_internal(app: &AppHandle, visible: bool) -> Result<(), String> {
    if app.get_webview_window("screenshot").is_none() {
        return Ok(());
    }
    if visible {
        show_overlay_window_by_label(app, "screenshot", true)?;
    } else {
        hide_overlay_window_by_label(app, "screenshot")?;
    }
    Ok(())
}

fn ensure_longshot_toolbar_window(app: &AppHandle) -> Result<(tauri::WebviewWindow, bool), String> {
    let label = "longshot_toolbar";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok((existing, false));
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("longshot_toolbar.html".into()),
    )
    .title("长截图工具栏")
    .visible(false)
    .resizable(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .inner_size(320.0, 180.0)
    .build()
    .map_err(|e| format!("创建长截图工具栏窗口失败: {}", e))?;
    let _ = window.set_content_protected(true);
    bind_overlay_window_events(&window, app.clone(), label);
    Ok((window, true))
}

fn ensure_longshot_border_window(app: &AppHandle) -> Result<(tauri::WebviewWindow, bool), String> {
    let label = "longshot_border";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok((existing, false));
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("longshot_border.html".into()),
    )
    .title("长截图边框")
    .visible(false)
    .resizable(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .build()
    .map_err(|e| format!("创建长截图边框窗口失败: {}", e))?;
    let _ = window.set_content_protected(true);
    bind_overlay_window_events(&window, app.clone(), label);
    let _ = window.set_ignore_cursor_events(true);
    Ok((window, true))
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LongshotToolbarAnchor {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn place_longshot_toolbar_near_anchor(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    anchor: Option<LongshotToolbarAnchor>,
) {
    let Some(anchor) = anchor else {
        let _ = window.move_window(tauri_plugin_positioner::Position::TopRight);
        return;
    };
    let (toolbar_w, toolbar_h) = (260i32, 430i32);
    let anchor_w = anchor.width as i32;
    let anchor_h = anchor.height as i32;
    let margin = 12i32;
    let default_x = anchor.x + anchor_w + margin;
    let default_y = anchor.y + (anchor_h / 2) - (toolbar_h / 2);
    let Some(screen_window) = app.get_webview_window("screenshot") else {
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            default_x, default_y,
        )));
        return;
    };
    let Ok(Some(monitor)) = screen_window.current_monitor() else {
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            default_x, default_y,
        )));
        return;
    };
    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let min_x = mon_pos.x + 8;
    let max_x = mon_pos.x + mon_size.width as i32 - toolbar_w - 8;
    let min_y = mon_pos.y + 8;
    let max_y = mon_pos.y + mon_size.height as i32 - toolbar_h - 8;
    let anchor_left = anchor.x;
    let anchor_top = anchor.y;
    let anchor_right = anchor.x + anchor_w;
    let anchor_bottom = anchor.y + anchor_h;

    let clamp_candidate =
        |x: i32, y: i32| -> (i32, i32) { (x.clamp(min_x, max_x), y.clamp(min_y, max_y)) };
    let intersects_anchor = |x: i32, y: i32| -> bool {
        let right = x + toolbar_w;
        let bottom = y + toolbar_h;
        x < anchor_right && right > anchor_left && y < anchor_bottom && bottom > anchor_top
    };

    let mut candidates = vec![
        clamp_candidate(anchor_right + margin, default_y),
        clamp_candidate(anchor_left - toolbar_w - margin, default_y),
        clamp_candidate(
            anchor_left + (anchor_w - toolbar_w) / 2,
            anchor_bottom + margin,
        ),
        clamp_candidate(
            anchor_left + (anchor_w - toolbar_w) / 2,
            anchor_top - toolbar_h - margin,
        ),
        (max_x, min_y),
    ];
    candidates.dedup();

    let chosen = candidates
        .iter()
        .copied()
        .find(|(x, y)| !intersects_anchor(*x, *y))
        .unwrap_or((max_x, min_y));

    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        chosen.0, chosen.1,
    )));
}

#[tauri::command]
pub async fn set_screenshot_input_passthrough(enabled: bool, app: AppHandle) -> Result<(), String> {
    set_screenshot_window_passthrough_internal(&app, enabled)
}

#[tauri::command]
pub async fn set_screenshot_window_visible(visible: bool, app: AppHandle) -> Result<(), String> {
    set_screenshot_window_visibility_internal(&app, visible)
}

#[tauri::command]
pub async fn show_longshot_toolbar(
    app: AppHandle,
    anchor: Option<LongshotToolbarAnchor>,
) -> Result<(), String> {
    let (window, _created) = ensure_longshot_toolbar_window(&app)?;
    let _ = window.set_content_protected(true);
    let _ = window.emit(
        "manual-longshot-toolbar-reset",
        serde_json::json!({
            "ts": now_unix_ms()
        }),
    );
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
        width: 260.0,
        height: 430.0,
    }));
    place_longshot_toolbar_near_anchor(&app, &window, anchor);
    show_overlay_window_by_label(&app, "longshot_toolbar", true)?;
    Ok(())
}

#[tauri::command]
pub async fn show_longshot_border(
    app: AppHandle,
    anchor: LongshotToolbarAnchor,
) -> Result<(), String> {
    let (window, _created) = ensure_longshot_border_window(&app)?;
    let _ = window.set_content_protected(true);
    // 边框窗外扩，确保边框不进入实际采集区域
    const BORDER_OUTSET: i32 = 2;
    let width = (anchor.width as i32 + BORDER_OUTSET * 2).max(2) as u32;
    let height = (anchor.height as i32 + BORDER_OUTSET * 2).max(2) as u32;
    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        anchor.x - BORDER_OUTSET,
        anchor.y - BORDER_OUTSET,
    )));
    show_overlay_window_by_label(&app, "longshot_border", false)?;
    Ok(())
}

#[tauri::command]
pub async fn hide_longshot_border(app: AppHandle) -> Result<(), String> {
    let _ = hide_overlay_window_by_label(&app, "longshot_border");
    Ok(())
}

#[tauri::command]
pub async fn snap_longshot_toolbar_window(app: AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("longshot_toolbar") else {
        return Ok(());
    };
    let Ok(pos) = window.outer_position() else {
        return Ok(());
    };
    let Ok(size) = window.outer_size() else {
        return Ok(());
    };
    let Ok(Some(monitor)) = window.current_monitor() else {
        return Ok(());
    };
    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let left = mon_pos.x + 8;
    let right = mon_pos.x + mon_size.width as i32 - size.width as i32 - 8;
    let top = mon_pos.y + 8;
    let threshold = 28;

    let mut next_x = pos.x;
    let mut next_y = pos.y;
    if (pos.x - left).abs() <= threshold {
        next_x = left;
    } else if (pos.x - right).abs() <= threshold {
        next_x = right;
    }
    if (pos.y - top).abs() <= threshold {
        next_y = top;
    }
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        next_x, next_y,
    )));
    Ok(())
}

#[tauri::command]
pub async fn hide_longshot_toolbar(app: AppHandle) -> Result<(), String> {
    let _ = hide_overlay_window_by_label(&app, "longshot_toolbar");
    Ok(())
}

#[tauri::command]
pub async fn longshot_toolbar_action(action: String, app: AppHandle) -> Result<(), String> {
    let Some(session_id) =
        crate::features::screenshot::longshot::active_manual_longshot_session_id()
    else {
        return Ok(());
    };
    match action.as_str() {
        "pause" => {
            crate::features::screenshot::longshot::pause_manual_longshot(session_id, app.clone())?;
        }
        "resume" => {
            crate::features::screenshot::longshot::resume_manual_longshot(session_id, app.clone())?;
        }
        "finish" => {
            let app_for_finish = app.clone();
            let result = tauri::async_runtime::spawn_blocking(move || {
                crate::features::screenshot::longshot::finish_manual_longshot(
                    session_id,
                    app_for_finish,
                )
            })
            .await
            .map_err(|e| format!("完成长截图任务执行失败: {}", e))??;
            if !result.image_path.is_empty() {
                replace_screenshot_boot_image_path(Some(PathBuf::from(&result.image_path)));
            }
            let _ = app.emit(
                "manual-longshot-shortcut-finished",
                serde_json::json!({
                    "sessionId": result.session_id,
                    "pngBase64": result.png_base64,
                    "imagePath": result.image_path,
                    "width": result.width,
                    "height": result.height,
                    "frameCount": result.frame_count,
                    "droppedFrames": result.dropped_frames,
                }),
            );
            let _ = hide_longshot_border(app.clone()).await;
            let _ = hide_longshot_toolbar(app.clone()).await;
            let _ = set_screenshot_window_visibility_internal(&app, true);
            return Ok(());
        }
        "cancel" => {
            let app_for_cancel = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                crate::features::screenshot::longshot::cancel_manual_longshot(
                    session_id,
                    app_for_cancel,
                )
            })
            .await
            .map_err(|e| format!("取消长截图任务执行失败: {}", e))??;
            let _ = app.emit(
                "manual-longshot-shortcut-canceled",
                serde_json::json!({
                    "sessionId": session_id
                }),
            );
            let _ = hide_longshot_border(app.clone()).await;
            let _ = hide_longshot_toolbar(app.clone()).await;
            let _ = set_screenshot_window_visibility_internal(&app, true);
            return Ok(());
        }
        _ => return Err("不支持的长截图操作".to_string()),
    }
    Ok(())
}

pub async fn finish_manual_longshot_from_shortcut(app: AppHandle) -> Result<(), String> {
    let Some(session_id) =
        crate::features::screenshot::longshot::active_manual_longshot_session_id()
    else {
        return Ok(());
    };
    let app_for_finish = app.clone();
    match tauri::async_runtime::spawn_blocking(move || {
        crate::features::screenshot::longshot::finish_manual_longshot(session_id, app_for_finish)
    })
    .await
    .map_err(|e| format!("完成长截图任务执行失败: {}", e))?
    {
        Ok(result) => {
            if !result.image_path.is_empty() {
                replace_screenshot_boot_image_path(Some(PathBuf::from(&result.image_path)));
            }
            let _ = app.emit(
                "manual-longshot-shortcut-finished",
                serde_json::json!({
                    "sessionId": result.session_id,
                    "pngBase64": result.png_base64,
                    "imagePath": result.image_path,
                    "width": result.width,
                    "height": result.height,
                    "frameCount": result.frame_count,
                    "droppedFrames": result.dropped_frames,
                }),
            );
            let _ = hide_longshot_border(app.clone()).await;
            let _ = hide_longshot_toolbar(app.clone()).await;
            let _ = set_screenshot_window_visibility_internal(&app, true);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

pub async fn cancel_manual_longshot_from_shortcut(app: AppHandle) -> Result<(), String> {
    let Some(session_id) =
        crate::features::screenshot::longshot::active_manual_longshot_session_id()
    else {
        return Ok(());
    };
    let app_for_cancel = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::features::screenshot::longshot::cancel_manual_longshot(session_id, app_for_cancel)
    })
    .await
    .map_err(|e| format!("取消长截图任务执行失败: {}", e))??;
    let _ = app.emit(
        "manual-longshot-shortcut-canceled",
        serde_json::json!({
            "sessionId": session_id
        }),
    );
    let _ = hide_longshot_border(app.clone()).await;
    let _ = hide_longshot_toolbar(app.clone()).await;
    let _ = set_screenshot_window_visibility_internal(&app, true);
    Ok(())
}

pub async fn toggle_manual_longshot_pause_from_shortcut(app: AppHandle) -> Result<(), String> {
    let Some(session_id) =
        crate::features::screenshot::longshot::active_manual_longshot_session_id()
    else {
        return Ok(());
    };
    let status = crate::features::screenshot::longshot::get_manual_longshot_status(session_id)?;
    if status.state == "running" {
        crate::features::screenshot::longshot::pause_manual_longshot(session_id, app.clone())?;
        let _ = app.emit(
            "manual-longshot-shortcut-paused",
            serde_json::json!({
                "sessionId": session_id
            }),
        );
        return Ok(());
    }
    if status.state == "paused" {
        crate::features::screenshot::longshot::resume_manual_longshot(session_id, app.clone())?;
        let _ = app.emit(
            "manual-longshot-shortcut-resumed",
            serde_json::json!({
                "sessionId": session_id
            }),
        );
    }
    Ok(())
}

/// 打开截图编辑窗口
#[tauri::command]
pub async fn open_screenshot_editor(app: AppHandle, mode: Option<String>) -> Result<(), String> {
    let selection_mode = mode
        .as_ref()
        .map(|m| m.to_lowercase())
        .unwrap_or_else(|| "screenshot".to_string());
    if let Ok(settings) = load_settings() {
        if !settings.screenshot_enabled && selection_mode != "recording_region" {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "截图功能已停用",
                "screenshot feature disabled",
            ));
        }
    }
    log::info!("打开截图编辑窗口");
    let started_at = std::time::Instant::now();

    use crate::features::screenshot::capture;
    if !capture::try_begin_screenshot() {
        log::info!("截图任务已在进行中，忽略重复触发");
        return Ok(());
    }
    let (rgba, width, height, origin_x, origin_y) = match capture::capture_full_screen() {
        Ok(data) => data,
        Err(e) => {
            capture::set_screenshot_in_progress(false);
            record_perf_metric(
                "screenshot.open_prepare",
                "截图打开准备耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(e.to_string()),
            );
            return Err(format!("截图失败: {}", e));
        }
    };

    let session_id = NEXT_SCREENSHOT_SESSION_ID.fetch_add(1, Ordering::SeqCst);
    let image_path =
        write_screenshot_boot_image(&rgba, width, height, session_id).map_err(|e| {
            capture::set_screenshot_in_progress(false);
            record_perf_metric(
                "screenshot.open_prepare",
                "截图打开准备耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(e.clone()),
            );
            e
        })?;

    let selection_mode = selection_mode;
    if let Some(window) = app.get_webview_window("screenshot") {
        if SCREENSHOT_LIFECYCLE_BOUND_FOR_BOOT_WINDOW
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            bind_screenshot_window_lifecycle(&window, &app);
        }
        let payload = serde_json::json!({
            "image_path": image_path,
            "width": width,
            "height": height,
            "origin_x": origin_x,
            "origin_y": origin_y,
            "session_id": session_id
        });
        let script = format!(
            "if (!window.__SCREENSHOT_BOOT_READY__) {{ throw new Error('screenshot boot not ready'); }}\
window.__SCREENSHOT_BOOT__ = window.__SCREENSHOT_BOOT__ || {{ pendingData: null, pendingStartSessionId: 0 }};\
window.__SCREENSHOT_BOOT__.pendingData = {payload};\
window.__SCREENSHOT_BOOT__.pendingStartSessionId = {session_id};\
window.__SCREENSHOT_BOOT__.pendingMode = '{selection_mode}';\
window.dispatchEvent(new CustomEvent('screenshot-data', {{ detail: {payload} }}));\
window.dispatchEvent(new CustomEvent('start-region-select', {{ detail: {{ session_id: {session_id}, mode: '{selection_mode}' }} }}));"
        );

        let app_for_window = app.clone();
        thread::spawn(move || {
            let _ = window.set_always_on_top(true);
            let _ = window.set_ignore_cursor_events(false);
            let _ = window.set_fullscreen(true);
            let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                x: origin_x,
                y: origin_y,
            }));
            let mut injected = false;
            for _attempt in 0..20 {
                if window.eval(&script).is_ok() {
                    injected = true;
                    break;
                }
                thread::sleep(Duration::from_millis(25));
            }
            if injected {
                record_perf_metric(
                    "screenshot.open_prepare",
                    "截图打开准备耗时",
                    started_at.elapsed().as_millis() as u64,
                    true,
                    None,
                );
                let _ = show_overlay_window_by_label(&app_for_window, "screenshot", true);
            } else {
                record_perf_metric(
                    "screenshot.open_prepare",
                    "截图打开准备耗时",
                    started_at.elapsed().as_millis() as u64,
                    false,
                    Some("截图窗口脚本注入失败".to_string()),
                );
                let _ = hide_overlay_window_by_label(&app_for_window, "screenshot");
                cleanup_all_screenshot_boot_images();
                capture::set_screenshot_in_progress(false);
            }
        });
    } else {
        let payload = serde_json::json!({
            "image_path": image_path,
            "width": width,
            "height": height,
            "origin_x": origin_x,
            "origin_y": origin_y,
            "session_id": session_id
        });
        let boot_script = format!(
            "window.__SCREENSHOT_BOOT__ = window.__SCREENSHOT_BOOT__ || {{ pendingData: null, pendingStartSessionId: 0 }};\
window.__SCREENSHOT_BOOT__.pendingData = {};\
window.__SCREENSHOT_BOOT__.pendingStartSessionId = {};\
window.__SCREENSHOT_BOOT__.pendingMode = '{}';",
            payload, session_id, selection_mode
        );
        let window = tauri::WebviewWindowBuilder::new(
            &app,
            "screenshot",
            tauri::WebviewUrl::App("screenshot.html".into()),
        )
        .title("截图选择")
        .visible(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .inner_size(width as f64, height as f64)
        .position(origin_x as f64, origin_y as f64)
        .fullscreen(true)
        .on_page_load(move |window, _| {
            let _ = window.eval(&boot_script);
            let app_handle = window.app_handle();
            let _ = show_overlay_window_by_label(&app_handle, "screenshot", true);
        })
        .build()
        .map_err(|e| {
            cleanup_all_screenshot_boot_images();
            capture::set_screenshot_in_progress(false);
            record_perf_metric(
                "screenshot.open_prepare",
                "截图打开准备耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(e.to_string()),
            );
            format!("创建截图窗口失败: {}", e)
        })?;
        bind_screenshot_window_lifecycle(&window, &app);
        let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
            x: origin_x,
            y: origin_y,
        }));
        record_perf_metric(
            "screenshot.open_prepare",
            "截图打开准备耗时",
            started_at.elapsed().as_millis() as u64,
            true,
            None,
        );
    }

    Ok(())
}

/// 获取窗口列表
#[tauri::command]
pub async fn get_window_list() -> Result<serde_json::Value, String> {
    use crate::features::screenshot::window_detect;

    match window_detect::get_window_list() {
        Ok(windows) => Ok(serde_json::json!({
            "success": true,
            "windows": windows
        })),
        Err(e) => {
            log::error!("获取窗口列表失败: {}", e);
            Ok(serde_json::json!({
                "success": false,
                "error": e.to_string(),
                "windows": []
            }))
        }
    }
}

/// 关闭截图窗口并释放焦点
#[tauri::command]
pub async fn close_screenshot_window(app: AppHandle) -> Result<(), String> {
    log::info!("关闭截图窗口");
    if let Some(window) = app.get_webview_window("screenshot") {
        // 解除置顶和鼠标拦截，防止在Windows上残留透明幽灵窗口导致桌面无法点击
        let _ = window.set_always_on_top(false);
        let _ = window.set_ignore_cursor_events(true);
        let _ = window.eval(
            "window.dispatchEvent(new CustomEvent('screenshot-reset'));\
window.__SCREENSHOT_BOOT__ = window.__SCREENSHOT_BOOT__ || { pendingData: null, pendingStartSessionId: 0 };\
window.__SCREENSHOT_BOOT__.pendingData = null;\
window.__SCREENSHOT_BOOT__.pendingStartSessionId = 0;\
window.__SCREENSHOT_BOOT__.pendingMode = null;",
        );
        let _ = hide_overlay_window_by_label(&app, "screenshot");
    }
    cleanup_all_screenshot_boot_images();
    features::screenshot::capture::set_screenshot_in_progress(false);

    Ok(())
}
