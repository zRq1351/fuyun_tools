use serde::{Deserialize, Serialize};

/// 窗口信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// 获取所有可见窗口列表
pub fn get_window_list() -> Result<Vec<WindowInfo>, String> {
    #[cfg(target_os = "windows")]
    {
        get_windows_list_win32()
    }

    #[cfg(target_os = "macos")]
    {
        get_window_list_macos()
    }

    #[cfg(target_os = "linux")]
    {
        get_window_list_linux()
    }
}

#[cfg(target_os = "windows")]
fn get_windows_list_win32() -> Result<Vec<WindowInfo>, String> {
    use std::ptr::NonNull;
    use winapi::um::winuser::{
        EnumWindows, GetWindowLongW, GetWindowRect, GetWindowTextLengthW,
        GetWindowTextW, IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
    };
    use winapi::shared::windef::RECT;

    let mut windows = Vec::new();

    unsafe extern "system" fn enum_callback(hwnd: winapi::shared::windef::HWND, lparam: winapi::shared::minwindef::LPARAM) -> i32 {
        let Some(mut windows_ptr) = NonNull::new(lparam as *mut Vec<WindowInfo>) else {
            return 0;
        };
        let windows = windows_ptr.as_mut();

        // 检查窗口是否可见
        if IsWindowVisible(hwnd) == 0 {
            return 1; // 继续枚举
        }

        // 排除工具窗口
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        if (ex_style & WS_EX_TOOLWINDOW as i32) != 0 {
            return 1;
        }

        // 获取窗口标题
        let title_len = GetWindowTextLengthW(hwnd);
        if title_len == 0 {
            return 1;
        }

        let mut title_buf = vec![0u16; (title_len + 1) as usize];
        let copied = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_len + 1);
        if copied == 0 {
            return 1;
        }

        let title = String::from_utf16_lossy(&title_buf[..copied as usize]);
        if title == "固定截图" || title == "截图选择" || title == "fuyun_tools" {
            return 1;
        }

        // 获取窗口位置
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return 1;
        }

        let width = (rect.right - rect.left) as u32;
        let height = (rect.bottom - rect.top) as u32;

        // 排除太小的窗口
        if width < 50 || height < 50 {
            return 1;
        }

        windows.push(WindowInfo {
            title,
            x: rect.left,
            y: rect.top,
            width,
            height,
        });

        1 // 继续枚举
    }

    unsafe {
        EnumWindows(Some(enum_callback), &mut windows as *mut _ as _);
    }

    Ok(windows)
}

#[cfg(target_os = "macos")]
fn get_window_list_macos() -> Result<Vec<WindowInfo>, String> {
    // macOS 实现（需要使用Cocoa API）
    // 这里返回空列表，实际实现需要使用objc或cocoa crate
    Ok(Vec::new())
}

#[cfg(target_os = "linux")]
fn get_window_list_linux() -> Result<Vec<WindowInfo>, String> {
    // Linux实现（需要使用X11或Wayland API）
    // 这里返回空列表，实际实现需要使用x11 crate
    Ok(Vec::new())
}
