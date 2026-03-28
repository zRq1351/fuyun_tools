use std::sync::atomic::{AtomicBool, Ordering};

/// 截图状态
static SCREENSHOT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static SCREENSHOT_ALLOW_IMAGE_CLIPBOARD_ONCE: AtomicBool = AtomicBool::new(false);

/// 检查是否正在截图
pub fn is_screenshot_in_progress() -> bool {
    SCREENSHOT_IN_PROGRESS.load(Ordering::SeqCst)
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
    set_screenshot_in_progress(true);

    let result = capture_screen_region_internal(x, y, width, height);

    set_screenshot_in_progress(false);
    result
}

/// 内部实现：捕获屏幕区域
fn capture_screen_region_internal(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(Vec<u8>, u32, u32), String> {
    // 获取所有屏幕
    let screens = screenshots::Screen::all()
        .map_err(|e| format!("获取屏幕列表失败: {}", e))?;

    if screens.is_empty() {
        return Err("未检测到屏幕".to_string());
    }

    // 默认使用主屏幕（第一个屏幕）
    let screen = screens.first()
        .ok_or_else(|| "无法获取主屏幕".to_string())?;

    // 捕获整个屏幕
    let image = screen.capture()
        .map_err(|e| format!("捕获屏幕失败: {}", e))?;

    // 获取图片尺寸
    let img_width = image.width();
    let img_height = image.height();

    // 边界检查和修正
    let x = x.max(0) as u32;
    let y = y.max(0) as u32;
    let width = width.min(img_width.saturating_sub(x));
    let height = height.min(img_height.saturating_sub(y));

    if width == 0 || height == 0 {
        return Err(format!("截图区域无效: {}x{}", width, height));
    }

    // 裁剪到指定区域 - 手动裁剪避免版本冲突
    let mut rgba_data = Vec::with_capacity((width * height * 4) as usize);
    for row in y..(y + height) {
        let start = ((row * img_width + x) * 4) as usize;
        let end = start + (width * 4) as usize;
        if end <= image.as_raw().len() {
            rgba_data.extend_from_slice(&image.as_raw()[start..end]);
        }
    }

    if rgba_data.len() != (width * height * 4) as usize {
        return Err("裁剪图片数据长度不匹配".to_string());
    }

    log::info!("截图成功: {}x{}, 数据大小: {} bytes", width, height, rgba_data.len());

    Ok((rgba_data, width, height))
}

/// 捕获全屏截图
///
/// # Returns
/// * `Result<(Vec<u8>, u32, u32)>` - (RGBA像素数据, 宽度, 高度)
pub fn capture_full_screen() -> Result<(Vec<u8>, u32, u32), String> {
    let screens = screenshots::Screen::all()
        .map_err(|e| format!("获取屏幕列表失败: {}", e))?;

    if screens.is_empty() {
        return Err("未检测到屏幕".to_string());
    }

    let screen = screens.first()
        .ok_or_else(|| "无法获取主屏幕".to_string())?;
    let image = screen.capture()
        .map_err(|e| format!("捕获全屏失败: {}", e))?;

    let width = image.width();
    let height = image.height();
    let rgba_data = image.to_vec();

    log::info!("全屏截图成功: {}x{}", width, height);

    Ok((rgba_data, width, height))
}

/// 获取屏幕尺寸
///
/// # Returns
/// * `Result<(u32, u32)>` - (宽度, 高度)
pub fn get_screen_size() -> Result<(u32, u32), String> {
    let screens = screenshots::Screen::all()
        .map_err(|e| format!("获取屏幕列表失败: {}", e))?;

    if screens.is_empty() {
        return Err("未检测到屏幕".to_string());
    }

    let screen = screens.first()
        .ok_or_else(|| "无法获取主屏幕".to_string())?;
    let display_info = &screen.display_info;

    Ok((display_info.width, display_info.height))
}

/// 将RGBA数据转换为PNG Base64字符串
pub fn rgba_to_base64_png(rgba: &[u8], width: u32, height: u32) -> Result<String, String> {
    use base64::Engine;
    use image::{ImageBuffer, ImageEncoder, Rgba};

    let _img_buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| "创建图片缓冲区失败".to_string())?;

    let mut png_data = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
    encoder.write_image(rgba, width, height, image::ColorType::Rgba8.into())
        .map_err(|e| format!("编码PNG失败: {}", e))?;

    let base64_str = base64::engine::general_purpose::STANDARD.encode(&png_data);

    Ok(base64_str)
}
