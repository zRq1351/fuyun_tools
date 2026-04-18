use std::sync::atomic::{AtomicBool, Ordering};

/// 截图状态
static SCREENSHOT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static SCREENSHOT_ALLOW_IMAGE_CLIPBOARD_ONCE: AtomicBool = AtomicBool::new(false);

/// 检查是否正在截图
pub fn is_screenshot_in_progress() -> bool {
    SCREENSHOT_IN_PROGRESS.load(Ordering::SeqCst)
}

pub fn try_begin_screenshot() -> bool {
    SCREENSHOT_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

/// 设置截图状态
pub fn set_screenshot_in_progress(in_progress: bool) {
    SCREENSHOT_IN_PROGRESS.store(in_progress, Ordering::SeqCst);
    if !in_progress {
        SCREENSHOT_ALLOW_IMAGE_CLIPBOARD_ONCE.store(false, Ordering::SeqCst);
    }
}

pub fn set_allow_image_clipboard_once(allow: bool) {
    SCREENSHOT_ALLOW_IMAGE_CLIPBOARD_ONCE.store(allow, Ordering::SeqCst);
}

pub fn take_allow_image_clipboard_once() -> bool {
    SCREENSHOT_ALLOW_IMAGE_CLIPBOARD_ONCE.swap(false, Ordering::SeqCst)
}

fn resolve_virtual_screen_bounds(
    screens: &[screenshots::Screen],
) -> Result<(i32, i32, u32, u32), String> {
    if screens.is_empty() {
        return Err("未检测到屏幕".to_string());
    }
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_right = i32::MIN;
    let mut max_bottom = i32::MIN;
    for screen in screens {
        let info = &screen.display_info;
        min_x = min_x.min(info.x);
        min_y = min_y.min(info.y);
        max_right = max_right.max(info.x.saturating_add(info.width as i32));
        max_bottom = max_bottom.max(info.y.saturating_add(info.height as i32));
    }
    let width = max_right.saturating_sub(min_x).max(0) as u32;
    let height = max_bottom.saturating_sub(min_y).max(0) as u32;
    if width == 0 || height == 0 {
        return Err("虚拟桌面尺寸无效".to_string());
    }
    Ok((min_x, min_y, width, height))
}

/// 捕获指定区域的屏幕截图
///
/// # Arguments
/// * `x` - 区域左上角X坐标
/// * `y` - 区域左上角Y坐标
/// * `width` - 区域宽度
/// * `height` - 区域高度
///
/// # Returns
/// * `Result<(Vec<u8>, u32, u32)>` - (RGBA像素数据, 宽度, 高度)
pub fn capture_screen_region(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(Vec<u8>, u32, u32), String> {
    let owns_screenshot_state = SCREENSHOT_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok();

    if !owns_screenshot_state {
        return Err("截图功能正在进行中，无法并发启动".to_string());
    }

    let result = capture_screen_region_internal(x, y, width, height);

    if owns_screenshot_state {
        set_screenshot_in_progress(false);
    }
    result
}

/// 内部实现：捕获屏幕区域
fn capture_screen_region_internal(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(Vec<u8>, u32, u32), String> {
    let screens = screenshots::Screen::all().map_err(|e| format!("获取屏幕列表失败: {}", e))?;

    if screens.is_empty() {
        return Err("未检测到屏幕".to_string());
    }

    let req_left = i64::from(x);
    let req_top = i64::from(y);
    let req_right = req_left.saturating_add(i64::from(width));
    let req_bottom = req_top.saturating_add(i64::from(height));

    let mut best_screen_index: Option<usize> = None;
    let mut best_overlap_area: i64 = -1;
    let mut best_distance_sq: i128 = i128::MAX;
    let center_x = req_left.saturating_add(i64::from(width / 2));
    let center_y = req_top.saturating_add(i64::from(height / 2));

    for (index, screen) in screens.iter().enumerate() {
        let sx = i64::from(screen.display_info.x);
        let sy = i64::from(screen.display_info.y);
        let sw = i64::from(screen.display_info.width);
        let sh = i64::from(screen.display_info.height);
        let s_right = sx.saturating_add(sw);
        let s_bottom = sy.saturating_add(sh);

        let overlap_w = (req_right.min(s_right) - req_left.max(sx)).max(0);
        let overlap_h = (req_bottom.min(s_bottom) - req_top.max(sy)).max(0);
        let overlap_area = overlap_w.saturating_mul(overlap_h);

        let clamped_x = center_x.clamp(sx, s_right.saturating_sub(1));
        let clamped_y = center_y.clamp(sy, s_bottom.saturating_sub(1));
        let dx = i128::from(center_x.saturating_sub(clamped_x));
        let dy = i128::from(center_y.saturating_sub(clamped_y));
        let distance_sq = dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy));

        if overlap_area > best_overlap_area
            || (overlap_area == best_overlap_area && distance_sq < best_distance_sq)
        {
            best_overlap_area = overlap_area;
            best_distance_sq = distance_sq;
            best_screen_index = Some(index);
        }
    }

    let screen = screens
        .get(best_screen_index.unwrap_or(0))
        .ok_or_else(|| "无法获取目标屏幕".to_string())?;

    let image = screen
        .capture()
        .map_err(|e| format!("捕获屏幕失败: {}", e))?;

    let img_width = image.width();
    let img_height = image.height();
    let screen_x = i64::from(screen.display_info.x);
    let screen_y = i64::from(screen.display_info.y);

    let local_left = (req_left - screen_x).max(0);
    let local_top = (req_top - screen_y).max(0);
    let local_right = (req_right - screen_x).min(i64::from(img_width));
    let local_bottom = (req_bottom - screen_y).min(i64::from(img_height));

    let width = (local_right - local_left).max(0) as u32;
    let height = (local_bottom - local_top).max(0) as u32;
    let x = local_left as u32;
    let y = local_top as u32;

    if width == 0 || height == 0 {
        return Err(format!("截图区域无效: {}x{}", width, height));
    }

    let mut rgba_data = Vec::with_capacity((width * height * 4) as usize);
    for row in y..(y + height) {
        let start = ((row * img_width + x) * 4) as usize;
        let end = start + (width * 4) as usize;
        let src = image.as_raw();
        if end <= src.len() {
            rgba_data.extend_from_slice(&src[start..end]);
        } else if start < src.len() {
            // 部分越界，用黑色/透明补齐
            let available = src.len() - start;
            rgba_data.extend_from_slice(&src[start..src.len()]);
            let padding = (width * 4) as usize - available;
            rgba_data.extend(std::iter::repeat(0).take(padding));
        } else {
            // 完全越界
            rgba_data.extend(std::iter::repeat(0).take((width * 4) as usize));
        }
    }

    if rgba_data.len() != (width * height * 4) as usize {
        return Err("裁剪图片数据长度不匹配".to_string());
    }

    log::info!(
        "截图成功: {}x{}, 数据大小: {} bytes",
        width,
        height,
        rgba_data.len()
    );

    Ok((rgba_data, width, height))
}

/// 捕获全屏截图
///
/// # Returns
/// * `Result<(Vec<u8>, u32, u32, i32, i32)>` - (RGBA像素数据, 宽度, 高度, 屏幕原点X, 屏幕原点Y)
pub fn capture_full_screen() -> Result<(Vec<u8>, u32, u32, i32, i32), String> {
    let screens = screenshots::Screen::all().map_err(|e| format!("获取屏幕列表失败: {}", e))?;

    if screens.is_empty() {
        return Err("未检测到屏幕".to_string());
    }

    let (origin_x, origin_y, width, height) = resolve_virtual_screen_bounds(&screens)?;

    // 单屏幕优化：直接返回截图数据，避免全尺寸零数组分配与拷贝
    if screens.len() == 1 {
        let screen = &screens[0];
        let image = screen
            .capture()
            .map_err(|e| format!("捕获单屏幕失败: {}", e))?;
        return Ok((image.into_raw(), width, height, origin_x, origin_y));
    }

    let mut rgba_data = vec![0_u8; (width as usize) * (height as usize) * 4];

    for screen in &screens {
        let image = screen
            .capture()
            .map_err(|e| format!("捕获全屏失败: {}", e))?;
        let screen_width = image.width() as usize;
        let screen_height = image.height() as usize;
        let offset_x = (screen.display_info.x - origin_x).max(0) as usize;
        let offset_y = (screen.display_info.y - origin_y).max(0) as usize;
        let src = image.as_raw();
        for row in 0..screen_height {
            let src_start = row * screen_width * 4;
            let src_end = src_start + screen_width * 4;
            let dest_row = offset_y + row;
            let dest_start = (dest_row * width as usize + offset_x) * 4;
            let dest_end = dest_start + screen_width * 4;
            if src_end <= src.len() && dest_end <= rgba_data.len() {
                rgba_data[dest_start..dest_end].copy_from_slice(&src[src_start..src_end]);
            }
        }
    }

    log::info!(
        "全屏截图成功(虚拟桌面): {}x{}, origin=({}, {})",
        width,
        height,
        origin_x,
        origin_y
    );

    Ok((rgba_data, width, height, origin_x, origin_y))
}

/// 获取屏幕尺寸
///
/// # Returns
/// * `Result<(u32, u32)>` - (宽度, 高度)
pub fn get_screen_size() -> Result<(u32, u32), String> {
    let screens = screenshots::Screen::all().map_err(|e| format!("获取屏幕列表失败: {}", e))?;

    let (_, _, width, height) = resolve_virtual_screen_bounds(&screens)?;
    Ok((width, height))
}

/// 将RGBA数据转换为PNG Base64字符串
pub fn rgba_to_png_bytes(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    use image::{ImageBuffer, ImageEncoder, Rgba};

    let _img_buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| "创建图片缓冲区失败".to_string())?;

    let mut png_data = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
    encoder
        .write_image(rgba, width, height, image::ColorType::Rgba8.into())
        .map_err(|e| format!("编码PNG失败: {}", e))?;

    Ok(png_data)
}

/// 将RGBA数据转换为PNG Base64字符串
pub fn rgba_to_base64_png(rgba: &[u8], width: u32, height: u32) -> Result<String, String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::write::EncoderWriter;
    use image::ImageEncoder;

    let mut base64_output = Vec::new();
    {
        let mut encoder_writer = EncoderWriter::new(&mut base64_output, &STANDARD);
        let encoder = image::codecs::png::PngEncoder::new(&mut encoder_writer);
        encoder
            .write_image(rgba, width, height, image::ColorType::Rgba8.into())
            .map_err(|e| format!("编码PNG/Base64失败: {}", e))?;
    }
    String::from_utf8(base64_output).map_err(|e| format!("Base64转字符串失败: {}", e))
}
