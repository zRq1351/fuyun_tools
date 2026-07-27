use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use base64::Engine;
use crate::utils::system_utils::resolve_lnk_target;

const SHGFI_ICON: u32 = 0x000000100;
const SHGFI_LARGEICON: u32 = 0x000000000;
const SHGFI_USEFILEATTRIBUTES: u32 = 0x000000010;

#[repr(C)]
#[allow(non_snake_case)]
struct SHFILEINFOW {
    hIcon: *mut std::ffi::c_void,
    iIcon: i32,
    dwAttributes: u32,
    szDisplayName: [u16; 260],
    szTypeName: [u16; 80],
}

extern "system" {
    fn SHGetFileInfoW(
        pszPath: *const u16,
        dwFileAttributes: u32,
        psfi: *mut SHFILEINFOW,
        cbFileInfo: u32,
        uFlags: u32,
    ) -> usize;
}

/// 统一的 GDI 图标提取函数（scopeguard 保护，支持自适应尺寸）
///
/// `path_or_ext`: 文件路径或扩展名（如 ".pdf"）
/// `flags`: SHGetFileInfoW 的 uFlags (SHGFI_ICON | SHGFI_LARGEICON 等)
/// `size_hint`: 期望尺寸；传 None 则自动检测原生图标尺寸
fn extract_icon_from_shell(
    path_or_ext: &str,
    flags: u32,
    size_hint: Option<i32>,
) -> Option<String> {
    let wide: Vec<u16> = OsStr::new(path_or_ext)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        use winapi::um::winuser::{DestroyIcon, GetIconInfo, ICONINFO};
        use winapi::um::wingdi::*;

        let mut shell_info: SHFILEINFOW = std::mem::zeroed();
        let result = SHGetFileInfoW(
            wide.as_ptr(),
            0,
            &mut shell_info,
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | flags,
        );

        if result == 0 || shell_info.hIcon.is_null() {
            return None;
        }

        let hicon = shell_info.hIcon as winapi::shared::windef::HICON;

        let mut icon_info: ICONINFO = std::mem::zeroed();
        if GetIconInfo(hicon, &mut icon_info) == 0 {
            DestroyIcon(hicon);
            return None;
        }

        // 自动检测原生图标尺寸
        let size = if let Some(hint) = size_hint.filter(|&h| h > 0) {
            hint
        } else {
            let (color_w, mask_w) = detect_native_icon_size(&icon_info);
            color_w.max(mask_w).max(32)
        };

        let width = size;
        let height = size;

        let mut bitmap_info: BITMAPINFO = std::mem::zeroed();
        bitmap_info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bitmap_info.bmiHeader.biWidth = width;
        bitmap_info.bmiHeader.biHeight = -height;
        bitmap_info.bmiHeader.biPlanes = 1;
        bitmap_info.bmiHeader.biBitCount = 32;
        bitmap_info.bmiHeader.biCompression = BI_RGB;

        let hdc = CreateCompatibleDC(std::ptr::null_mut());
        scopeguard::defer! { cleanup(hicon, &icon_info, hdc); }

        let mut color_buffer: Vec<u8> = vec![0u8; (width * height * 4) as usize];
        let mut mask_buffer: Vec<u8> = vec![0u8; (width * height * 4) as usize];

        if !icon_info.hbmColor.is_null() {
            GetDIBits(hdc, icon_info.hbmColor, 0, height as u32,
                color_buffer.as_mut_ptr() as *mut _, &mut bitmap_info, DIB_RGB_COLORS);
        }
        if !icon_info.hbmMask.is_null() {
            GetDIBits(hdc, icon_info.hbmMask, 0, height as u32,
                mask_buffer.as_mut_ptr() as *mut _, &mut bitmap_info, DIB_RGB_COLORS);
        }

        let has_alpha = color_buffer.chunks_exact(4).any(|c| c[3] != 0);

        let mut final_buffer: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);
        for i in 0..(width * height) as usize {
            let ci = i * 4;
            let b = color_buffer[ci];
            let g = color_buffer[ci + 1];
            let r = color_buffer[ci + 2];
            let a = if has_alpha {
                color_buffer[ci + 3]
            } else {
                let mask_byte = mask_buffer[ci];
                if mask_byte == 0 { 255 } else { 0 }
            };
            final_buffer.extend_from_slice(&[r, g, b, a]);
        }

        let img = match image::RgbaImage::from_raw(width as u32, height as u32, final_buffer) {
            Some(img) => img,
            None => return None,
        };

        let mut png_data = Vec::new();
        {
            use image::ImageEncoder;
            let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
            if encoder
                .write_image(img.as_raw(), width as u32, height as u32, image::ColorType::Rgba8.into())
                .is_err()
            {
                return None;
            }
        }

        let base64_str = base64::engine::general_purpose::STANDARD.encode(&png_data);
        Some(format!("data:image/png;base64,{}", base64_str))
    }
}

/// 检测原生图标尺寸（从 hbmColor 和 hbmMask 位图中获取宽度）
unsafe fn detect_native_icon_size(icon_info: &winapi::um::winuser::ICONINFO) -> (i32, i32) {
    use winapi::um::wingdi::{GetObjectW, BITMAP};
    let get_w = |hbm: winapi::shared::windef::HBITMAP| -> i32 {
        if hbm.is_null() { return 0; }
        let mut bm: BITMAP = std::mem::zeroed();
        let size = std::mem::size_of::<BITMAP>() as i32;
        if GetObjectW(hbm as *mut _, size, &mut bm as *mut _ as *mut _) != 0 {
            bm.bmWidth
        } else { 0 }
    };
    (get_w(icon_info.hbmColor), get_w(icon_info.hbmMask))
}

/// 从文件路径提取图标（启动器使用）
/// 对于 .lnk 文件会先解析目标路径
pub fn extract_icon_base64(path: &str) -> Option<String> {
    let icon_path = if path.to_lowercase().ends_with(".lnk") {
        resolve_lnk_target(path).unwrap_or_else(|| path.to_string())
    } else {
        path.to_string()
    };
    extract_icon_from_shell(&icon_path, SHGFI_LARGEICON, Some(48))
}

/// 从文件扩展名提取系统关联图标（文档管理使用）
/// 使用 USEFILEATTRIBUTES 标志，无需实际文件存在
pub fn extract_icon_by_extension(ext: &str) -> Option<String> {
    let fake_path = if ext.starts_with('.') {
        ext.to_string()
    } else {
        format!(".{}", ext)
    };
    extract_icon_from_shell(&fake_path, SHGFI_USEFILEATTRIBUTES, None)
}

/// 批量从文件路径提取图标
pub fn batch_extract_icons(paths: &[String]) -> std::collections::HashMap<String, String> {
    let mut result = std::collections::HashMap::new();
    for path in paths {
        if let Some(icon) = extract_icon_base64(path) {
            result.insert(path.clone(), icon);
        }
    }
    result
}

unsafe fn cleanup(
    hicon: winapi::shared::windef::HICON,
    icon_info: &winapi::um::winuser::ICONINFO,
    hdc: winapi::shared::windef::HDC,
) {
    use winapi::um::winuser::DestroyIcon;
    use winapi::um::wingdi::{DeleteDC, DeleteObject};

    DeleteDC(hdc);
    if !icon_info.hbmColor.is_null() {
        DeleteObject(icon_info.hbmColor as *mut _);
    }
    if !icon_info.hbmMask.is_null() {
        DeleteObject(icon_info.hbmMask as *mut _);
    }
    DestroyIcon(hicon);
}
