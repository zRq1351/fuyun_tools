//! Windows专用划词功能实现
//! 使用全局鼠标钩子和Windows UI Automation获取选中文本

#[cfg(windows)]
mod windows_impl {
    use std::ffi::c_int;
    use std::ptr;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use tauri::{AppHandle, Emitter};
    use winapi::shared::minwindef::*;
    use winapi::shared::windef::*;
    use winapi::shared::windowsx::*;
    use winapi::um::libloaderapi::*;
    use winapi::um::winuser::*;

    // 全局变量用于存储鼠标钩子和应用句柄
    static mut MOUSE_HOOK: HHOOK = ptr::null_mut();
    static mut APP_HANDLE: Option<Arc<Mutex<AppHandle>>> = None;
    static mut IS_SELECTING: bool = false;
    static mut SELECTION_START_POS: (i32, i32) = (0, 0);

    /// Windows鼠标钩子回调函数
    extern "system" fn mouse_proc(n_code: c_int, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
        if n_code >= 0 {
            match w_param as u32 {
                WM_LBUTTONDOWN => {
                    // 鼠标左键按下，开始划词
                    unsafe {
                        IS_SELECTING = true;
                        let pt = GET_X_LPARAM(l_param) as i32;
                        let pt_y = GET_Y_LPARAM(l_param) as i32;
                        SELECTION_START_POS = (pt, pt_y);
                    }
                }
                WM_LBUTTONUP => {
                    // 鼠标左键释放，结束划词并尝试获取选中文本
                    if unsafe { IS_SELECTING } {
                        unsafe {
                            IS_SELECTING = false;
                            // 在新线程中延迟获取文本，因为需要等待目标应用完成选择
                            thread::spawn(|| {
                                thread::sleep(Duration::from_millis(50)); // 短暂延迟
                                if let Ok(selected_text) = get_selected_text_via_uia() {
                                    if is_valid_text(&selected_text) {
                                        // 如果找到了应用句柄，发送选中文本到前端
                                        if let Some(ref app_handle_mutex) = APP_HANDLE {
                                            if let Ok(app_handle) = app_handle_mutex.lock() {
                                                // 发送选中文本到前端
                                                log::debug!("Selected Text: {}", selected_text);
                                                let _ = app_handle
                                                    .emit("selected-text", selected_text.clone());
                                                // 显示划词工具栏 - 调用实际的实现函数
                                                crate::show_selection_toolbar_impl(
                                                    app_handle.clone(),
                                                    selected_text,
                                                );
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        unsafe { CallNextHookEx(MOUSE_HOOK, n_code, w_param, l_param) }
    }

    /// 检查selected_text是否为有效文本（排除网址，邮箱，电话号码等）
    fn is_valid_text(selected_text: &str) -> bool {
        let clean_text = selected_text.trim();
        
        // 忽略空文本或太短的文本
        if clean_text.is_empty() || clean_text.len() < 2 {
            return false;
        }
        
        // 忽略网址
        if selected_text.contains("http://")
            || selected_text.contains("https://")
            || selected_text.starts_with("www.") {
            return false;
        }
        
        // 忽略邮箱地址
        if selected_text.contains('@') && selected_text.contains('.') {
            // 简单检查邮箱格式
            let parts: Vec<&str> = selected_text.split('@').collect();
            if parts.len() == 2 {
                let local_part = parts[0];
                let domain_part = parts[1];
                if !local_part.is_empty() && domain_part.contains('.') {
                    return false;
                }
            }
        }
        
        // 检查是否为数字或特殊字符过多
        let special_chars = clean_text.chars().filter(|c| !c.is_alphanumeric() && !c.is_whitespace()).count();
        if special_chars > clean_text.len() / 2 {
            return false;
        }
        
        true
    }
    /// 通过UI Automation获取当前选中的文本
    fn get_selected_text_via_uia() -> Result<String, Box<dyn std::error::Error>> {
        use windows::{Win32::System::Com::*, Win32::UI::Accessibility::*};

        unsafe {
            // 初始化COM
            let hr = CoInitialize(None);
            let should_uninitialize = hr.is_ok() || hr == windows::Win32::Foundation::S_FALSE;
            
            // 创建UI Automation接口
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;

            // 获取当前焦点元素
            let focused_element = match automation.GetFocusedElement() {
                Ok(element) => element,
                Err(_) => {
                    if should_uninitialize {
                        CoUninitialize();
                    }
                    return Ok(String::new());
                }
            };

            // 尝试获取文本模式
            match focused_element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
                Ok(text_pattern) => {
                    // 获取选择范围
                    if let Ok(selection) = text_pattern.GetSelection() {
                        if let Ok(count) = selection.Length() {
                            if count > 0 {
                                // 获取第一个选择范围
                                if let Ok(range) = selection.GetElement(0) {
                                    if let Ok(text_bstr) = range.GetText(-1) { // -1表示获取全部文本
                                        let text_str = if !text_bstr.is_empty() {
                                            text_bstr.to_string()
                                        } else {
                                            String::new()
                                        };

                                        if should_uninitialize {
                                            CoUninitialize();
                                        }
                                        return Ok(text_str);
                                    }
                                }
                            }
                        }
                    }

                    // 如果没有选择范围，尝试获取文档范围
                    if let Ok(doc_range) = text_pattern.DocumentRange() {
                        if let Ok(text_bstr) = doc_range.GetText(-1) {
                            let text_str = if !text_bstr.is_empty() {
                                text_bstr.to_string()
                            } else {
                                String::new()
                            };

                            if should_uninitialize {
                                CoUninitialize();
                            }
                            return Ok(text_str);
                        }
                    }
                }
                Err(_) => {
                    // 如果文本模式不可用，尝试Value模式
                    if let Ok(value_pattern) = focused_element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
                        if let Ok(value) = value_pattern.CurrentValue() {
                            let text_str = if !value.is_empty() {
                                value.to_string()
                            } else {
                                String::new()
                            };
                            
                            if should_uninitialize {
                                CoUninitialize();
                            }
                            return Ok(text_str);
                        }
                    }
                }
            }

            if should_uninitialize {
                CoUninitialize();
            }
        }

        Ok(String::new())
    }
    /// 启动Windows划词监听器
    pub fn start_windows_text_selection_listener(app_handle: AppHandle) {
        #[cfg(windows)]
        unsafe {
            // 存储应用句柄到全局变量
            let app_handle_mutex = Arc::new(Mutex::new(app_handle));
            APP_HANDLE = Some(app_handle_mutex);

            // 设置鼠标钩子
            MOUSE_HOOK = SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(mouse_proc),
                GetModuleHandleW(ptr::null()),
                0,
            );

            if MOUSE_HOOK.is_null() {
                eprintln!("Failed to install mouse hook");
            }
        }
    }

    /// 停止Windows划词监听器
    pub fn stop_windows_text_selection_listener() {
        #[cfg(windows)]
        unsafe {
            if !MOUSE_HOOK.is_null() {
                UnhookWindowsHookEx(MOUSE_HOOK);
                MOUSE_HOOK = ptr::null_mut();
            }

            // 清空全局应用句柄
            APP_HANDLE = None;
        }
    }
}

#[cfg(windows)]
pub use windows_impl::*;

#[cfg(not(windows))]
pub fn start_windows_text_selection_listener(_: tauri::AppHandle) {
    // 非Windows平台不实现此功能
}

#[cfg(not(windows))]
pub fn stop_windows_text_selection_listener() {
    // 非Windows平台不实现此功能
}
