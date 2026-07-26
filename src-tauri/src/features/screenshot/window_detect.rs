use serde::{Deserialize, Serialize};

/// 窗口信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub hwnd: String,
    pub title: String,
    pub process_name: String,
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
fn is_cloaked(hwnd: windows::Win32::Foundation::HWND) -> bool {
    use windows::core::BOOL;
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    let mut cloaked: BOOL = BOOL(0);
    let res = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as _,
            std::mem::size_of::<BOOL>() as u32,
        )
    };
    if res.is_ok() {
        cloaked.as_bool()
    } else {
        false
    }
}

#[cfg(target_os = "windows")]
fn is_system_or_invalid_process(process_name: &str) -> bool {
    let lower = process_name.to_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    matches!(
        stem,
        "textinputhost"
            | "applicationframehost"
            | "runtimebroker"
            | "svchost"
            | "shellhost"
            | "searchhost"
            | "taskhostw"
            | "dwm"
            | "systemsettings"
            | "widgetboard"
            | "widgetservice"
            | "startmenuexperiencehost"
            | "phoneexperiencehost"
            | "crossdeviceresume"
    )
}

#[cfg(target_os = "windows")]
fn get_window_process_name(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use std::path::Path;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    use windows::core::PWSTR;

    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == 0 {
        return None;
    }

    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => h,
        Err(_) => return None,
    };

    let mut buffer = vec![0u16; 1024];
    let mut size = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(handle, PROCESS_NAME_FORMAT(0), PWSTR(buffer.as_mut_ptr()), &mut size) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    if ok.is_err() || size == 0 {
        return None;
    }

    let full_path = String::from_utf16_lossy(&buffer[..size as usize]);
    let file_name = Path::new(&full_path)
        .file_name()
        .map(|x| x.to_string_lossy().to_string())?;
    let app_name = Path::new(&file_name)
        .file_stem()
        .map(|x| x.to_string_lossy().to_string())
        .unwrap_or(file_name);
    let trimmed = app_name.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(target_os = "windows")]
fn get_windows_list_win32() -> Result<Vec<WindowInfo>, String> {
    use std::ptr::NonNull;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowLongW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
        IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
    };

    let mut windows = Vec::new();

    unsafe extern "system" fn enum_callback(
        hwnd: HWND,
        lparam: LPARAM,
    ) -> BOOL {
        let Some(mut windows_ptr) = NonNull::new(lparam.0 as *mut Vec<WindowInfo>) else {
            return BOOL(0);
        };
        let windows = windows_ptr.as_mut();

        if IsWindowVisible(hwnd).as_bool() == false {
            return BOOL(1);
        }

        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        if (ex_style & WS_EX_TOOLWINDOW.0 as i32) != 0 {
            return BOOL(1);
        }

        let title_len = GetWindowTextLengthW(hwnd);
        if title_len == 0 {
            return BOOL(1);
        }

        let mut title_buf = vec![0u16; (title_len + 1) as usize];
        let copied = GetWindowTextW(hwnd, &mut title_buf);
        if copied == 0 {
            return BOOL(1);
        }

        let title = String::from_utf16_lossy(&title_buf[..copied as usize]);
        if title == "固定截图" || title == "截图选择" || title == "fuyun_tools" {
            return BOOL(1);
        }
        let process_name = get_window_process_name(hwnd).unwrap_or_default();
        if is_system_or_invalid_process(&process_name) {
            return BOOL(1);
        }

        if is_cloaked(hwnd) {
            return BOOL(1);
        }

        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return BOOL(1);
        }

        let width = (rect.right - rect.left) as u32;
        let height = (rect.bottom - rect.top) as u32;

        if width < 50 || height < 50 {
            return BOOL(1);
        }

        windows.push(WindowInfo {
            hwnd: format!("0x{:X}", hwnd.0 as usize),
            title,
            process_name,
            x: rect.left,
            y: rect.top,
            width,
            height,
        });

        BOOL(1)
    }

    unsafe {
        let _ = EnumWindows(Some(enum_callback), LPARAM(&mut windows as *mut _ as _));
    }

    Ok(windows)
}

#[cfg(target_os = "macos")]
fn get_window_list_macos() -> Result<Vec<WindowInfo>, String> {
    Ok(Vec::new())
}

#[cfg(target_os = "linux")]
fn get_window_list_linux() -> Result<Vec<WindowInfo>, String> {
    Ok(Vec::new())
}
