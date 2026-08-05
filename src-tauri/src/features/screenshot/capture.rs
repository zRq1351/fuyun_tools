use std::sync::atomic::{AtomicBool, Ordering};

/// 截图状态
static SCREENSHOT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static SCREENSHOT_ALLOW_IMAGE_CLIPBOARD_ONCE: AtomicBool = AtomicBool::new(false);

/// RAII守卫：获取时设置标志为true，Drop时自动重置为false
/// 防止panic导致标志永远卡在true状态
struct ScreenshotGuard;

impl ScreenshotGuard {
    fn try_acquire() -> Option<Self> {
        if SCREENSHOT_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            Some(Self)
        } else {
            None
        }
    }
}

impl Drop for ScreenshotGuard {
    fn drop(&mut self) {
        SCREENSHOT_IN_PROGRESS.store(false, Ordering::SeqCst);
        SCREENSHOT_ALLOW_IMAGE_CLIPBOARD_ONCE.store(false, Ordering::SeqCst);
    }
}

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
    let _guard = ScreenshotGuard::try_acquire()
        .ok_or_else(|| "截图功能正在进行中，无法并发启动".to_string())?;

    capture_screen_region_internal(x, y, width, height)
    // _guard 在此处自动drop，重置标志
}

/// 内部实现：捕获屏幕区域（支持跨屏：逐屏采集与目标区域重叠的部分并拼接）
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

    if req_right <= req_left || req_bottom <= req_top {
        return Err("截图区域无效".to_string());
    }

    let out_width = (req_right - req_left) as usize;
    let out_height = (req_bottom - req_top) as usize;
    let mut rgba_data = vec![0_u8; out_width.saturating_mul(out_height).saturating_mul(4)];

    for screen in &screens {
        let sx = i64::from(screen.display_info.x);
        let sy = i64::from(screen.display_info.y);
        let sw = i64::from(screen.display_info.width);
        let sh = i64::from(screen.display_info.height);

        // 该屏幕与目标区域在虚拟桌面坐标下的交集
        let inter_left = req_left.max(sx);
        let inter_top = req_top.max(sy);
        let inter_right = req_right.min(sx.saturating_add(sw));
        let inter_bottom = req_bottom.min(sy.saturating_add(sh));
        let inter_w = inter_right - inter_left;
        let inter_h = inter_bottom - inter_top;
        if inter_w <= 0 || inter_h <= 0 {
            continue;
        }

        let image = screen
            .capture()
            .map_err(|e| format!("捕获屏幕失败: {}", e))?;
        let img_width = image.width() as usize;
        let img_height = image.height() as usize;
        let src = image.as_raw();

        let loc_x = (inter_left - sx) as usize;
        let loc_y = (inter_top - sy) as usize;
        let src_row_bytes = inter_w as usize * 4;
        let dest_offset_x = (inter_left - req_left) as usize;
        let dest_offset_y = (inter_top - req_top) as usize;

        for row in 0..inter_h as usize {
            if loc_y + row >= img_height {
                continue;
            }
            let src_start = ((loc_y + row) * img_width + loc_x) * 4;
            let src_end = src_start + src_row_bytes;
            if src_end > src.len() {
                continue;
            }
            let dest_start = ((dest_offset_y + row) * out_width + dest_offset_x) * 4;
            let dest_end = dest_start + src_row_bytes;
            if dest_end > rgba_data.len() {
                continue;
            }
            rgba_data[dest_start..dest_end].copy_from_slice(&src[src_start..src_end]);
        }
    }

    log::info!(
        "截图成功: {}x{}, 数据大小: {} bytes",
        out_width,
        out_height,
        rgba_data.len()
    );

    Ok((rgba_data, out_width as u32, out_height as u32))
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

    if screens.len() == 1 {
        let screen = &screens[0];
        let image = screen
            .capture()
            .map_err(|e| format!("捕获单屏幕失败: {}", e))?;
        return Ok((image.into_raw(), width, height, origin_x, origin_y));
    }

    let mut rgba_data = vec![0_u8; (width as usize) * (height as usize) * 4];
    let dest_stride = width as usize * 4;

    for screen in &screens {
        let image = screen
            .capture()
            .map_err(|e| format!("捕获全屏失败: {}", e))?;
        let screen_width = image.width() as usize;
        let screen_height = image.height() as usize;
        let offset_x = (screen.display_info.x - origin_x).max(0) as usize;
        let offset_y = (screen.display_info.y - origin_y).max(0) as usize;
        let src = image.as_raw();
        let src_row_bytes = screen_width * 4;

        for row in 0..screen_height {
            let src_start = row * src_row_bytes;
            let src_end = src_start + src_row_bytes;
            let dest_row = offset_y + row;
            let dest_start = dest_row * dest_stride + offset_x * 4;
            let dest_end = dest_start + src_row_bytes;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 截图标志为全局静态，测试需串行避免互相干扰
    static FLAG_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock_flags() -> std::sync::MutexGuard<'static, ()> {
        FLAG_MUTEX.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn test_try_begin_screenshot_guards_concurrency() {
        let _g = lock_flags();
        set_screenshot_in_progress(false);
        assert!(!is_screenshot_in_progress());
        assert!(try_begin_screenshot());
        assert!(is_screenshot_in_progress());
        // 第二次获取应失败（已在进行中）
        assert!(!try_begin_screenshot());
        set_screenshot_in_progress(false);
        assert!(!is_screenshot_in_progress());
    }

    #[test]
    fn test_set_screenshot_in_progress_resets_clipboard_flag() {
        let _g = lock_flags();
        set_allow_image_clipboard_once(true);
        set_screenshot_in_progress(true);
        assert!(is_screenshot_in_progress());
        set_screenshot_in_progress(false);
        assert!(!is_screenshot_in_progress());
        // 结束后 allow-flag 应被清除
        assert!(!take_allow_image_clipboard_once());
    }

    #[test]
    fn test_take_allow_image_clipboard_once() {
        let _g = lock_flags();
        set_allow_image_clipboard_once(true);
        assert!(take_allow_image_clipboard_once());
        // 第二次应为 false（一次性标志已被取走）
        assert!(!take_allow_image_clipboard_once());
    }

    #[test]
    fn test_rgba_to_png_bytes_valid() {
        let rgba = vec![255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
        let png = rgba_to_png_bytes(&rgba, 2, 2).unwrap();
        // PNG 签名: 89 50 4E 47
        assert_eq!(&png[0..4], b"\x89PNG");
    }

    #[test]
    fn test_rgba_to_png_bytes_invalid_size() {
        // 2x2 需要 16 字节，给 8 字节应报错
        let rgba = vec![0u8; 8];
        assert!(rgba_to_png_bytes(&rgba, 2, 2).is_err());
    }

    #[test]
    fn test_rgba_to_base64_png_roundtrip() {
        let rgba = vec![0u8; 4 * 4 * 4]; // 4x4 透明黑
        let b64 = rgba_to_base64_png(&rgba, 4, 4).unwrap();
        assert!(b64.starts_with("iVBORw0KGgo")); // PNG base64 标准前缀
        // base64 解码后可得到 PNG 签名
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .unwrap();
        assert_eq!(&decoded[0..4], b"\x89PNG");
    }
}
