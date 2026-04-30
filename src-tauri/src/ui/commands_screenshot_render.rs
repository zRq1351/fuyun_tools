use image::{imageops, Rgba, RgbaImage};
use imageproc::drawing::{
    draw_filled_circle_mut, draw_hollow_ellipse_mut, draw_hollow_rect_mut, draw_text_mut,
};
use imageproc::rect::Rect;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScreenshotExportSelection {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScreenshotExportTextItem {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) text: String,
    pub(super) color: String,
    pub(super) font_size: f32,
    pub(super) font_family: Option<String>,
    pub(super) bold: Option<bool>,
    pub(super) stroke: Option<bool>,
    pub(super) stroke_color: Option<String>,
    pub(super) shadow: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScreenshotExportShapeItem {
    #[serde(rename = "type")]
    pub(super) shape_type: String,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) x1: Option<f32>,
    pub(super) y1: Option<f32>,
    pub(super) x2: Option<f32>,
    pub(super) y2: Option<f32>,
    pub(super) color: String,
    pub(super) line_width: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScreenshotExportRasterPoint {
    pub(super) x: f32,
    pub(super) y: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScreenshotExportRasterCommand {
    #[serde(rename = "type")]
    pub(super) raster_type: String,
    pub(super) color: String,
    pub(super) line_width: f32,
    pub(super) points: Vec<ScreenshotExportRasterPoint>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotExportRequest {
    pub(super) source_image_path: String,
    pub(super) output_path: String,
    pub(super) is_longshot: bool,
    pub(super) device_pixel_ratio: Option<f32>,
    pub(super) viewport_width: Option<f32>,
    pub(super) viewport_height: Option<f32>,
    pub(super) selection: Option<ScreenshotExportSelection>,
    pub(super) text_items: Vec<ScreenshotExportTextItem>,
    pub(super) shape_items: Vec<ScreenshotExportShapeItem>,
    pub(super) overlay_commands: Vec<ScreenshotExportRasterCommand>,
}

static SCREENSHOT_EXPORT_FONT_BYTES: OnceLock<StdMutex<HashMap<String, Arc<Vec<u8>>>>> =
    OnceLock::new();
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

pub(super) fn render_screenshot_image(request: &ScreenshotExportRequest) -> Result<RgbaImage, String> {
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

pub(super) fn export_screenshot_image(request: &ScreenshotExportRequest) -> Result<(), String> {
    if request.output_path.trim().is_empty() {
        return Err("缺少导出目标路径".to_string());
    }
    let canvas = render_screenshot_image(request)?;
    canvas
        .save(&request.output_path)
        .map_err(|e| format!("写入导出图片失败: {}", e))?;
    Ok(())
}
