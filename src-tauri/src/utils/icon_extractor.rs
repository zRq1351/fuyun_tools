use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use base64::Engine;

const SHGFI_ICON: u32 = 0x000000100;
const SHGFI_LARGEICON: u32 = 0x000000000;

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

fn resolve_shortcut_target_batch(lnk_paths: &[String]) -> std::collections::HashMap<String, String> {
    let mut result = std::collections::HashMap::new();

    if lnk_paths.is_empty() {
        return result;
    }

    let mut current_index = 0;
    for chunk in lnk_paths.chunks(20) {
        let script_parts: Vec<String> = chunk.iter().map(|p| {
            format!(
                "try {{ $s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s.TargetPath }} catch {{ '' }}",
                p.replace("'", "''")
            )
        }).collect();

        let script = script_parts.join("; ");
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .creation_flags(0x08000000)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()
            .and_then(|child| child.wait_with_output().ok());

        if let Some(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let targets: Vec<&str> = stdout.lines().collect();
            for (i, target) in targets.iter().enumerate() {
                let target = target.trim();
                if !target.is_empty() && i < chunk.len() {
                    let path = &chunk[i];
                    if std::path::Path::new(target).exists() {
                        result.insert(path.clone(), target.to_string());
                    }
                }
            }
        }
        current_index += chunk.len();
        let _ = current_index;
    }

    result
}

fn get_icon_for_file(icon_path: &str) -> Option<String> {
    let wide_path: Vec<u16> = OsStr::new(icon_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        use winapi::um::winuser::{DestroyIcon, GetIconInfo, ICONINFO};
        use winapi::um::wingdi::*;

        let mut shell_info: SHFILEINFOW = std::mem::zeroed();
        let result = SHGetFileInfoW(
            wide_path.as_ptr(),
            0,
            &mut shell_info,
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
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

        let width = 48i32;
        let height = 48i32;

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

        GetDIBits(
            hdc,
            icon_info.hbmColor,
            0,
            height as u32,
            color_buffer.as_mut_ptr() as *mut _,
            &mut bitmap_info,
            DIB_RGB_COLORS,
        );

        GetDIBits(
            hdc,
            icon_info.hbmMask,
            0,
            height as u32,
            mask_buffer.as_mut_ptr() as *mut _,
            &mut bitmap_info,
            DIB_RGB_COLORS,
        );

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

pub fn extract_icon_base64(path: &str) -> Option<String> {
    let icon_path = if path.to_lowercase().ends_with(".lnk") {
        resolve_shortcut_target_batch(&[path.to_string()])
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.to_string())
    } else {
        path.to_string()
    };
    get_icon_for_file(&icon_path)
}

pub fn batch_extract_icons(paths: &[String]) -> std::collections::HashMap<String, String> {
    let mut result = std::collections::HashMap::new();

    let lnk_paths: Vec<String> = paths.iter()
        .filter(|p| p.to_lowercase().ends_with(".lnk"))
        .cloned()
        .collect();

    let non_lnk_paths: Vec<String> = paths.iter()
        .filter(|p| !p.to_lowercase().ends_with(".lnk"))
        .cloned()
        .collect();

    let resolved = resolve_shortcut_target_batch(&lnk_paths);

    for (lnk, target) in &resolved {
        if let Some(icon) = get_icon_for_file(target) {
            result.insert(lnk.clone(), icon);
        }
    }

    for path in &non_lnk_paths {
        if let Some(icon) = get_icon_for_file(path) {
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
