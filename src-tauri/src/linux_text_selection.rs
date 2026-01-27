//! Linux 专用划词功能实现
//! 使用 X11 或 Wayland 事件监听划词结束事件

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use tauri::AppHandle;

    // 全局变量存储应用状态
    static mut IS_SELECTING: bool = false;
    static mut PREVIOUS_SELECTED_TEXT: String = String::new();

    /// 获取当前选中的文本
    fn get_selected_text() -> Result<String, Box<dyn std::error::Error>> {
        // 在 Linux 中，文本选择通常使用 PRIMARY 剪贴板
        // 这允许我们获取当前选中的文本而不影响用户的常规剪贴板
        Ok(get_primary_selection()?)
    }

    /// 获取 PRIMARY 剪贴板内容（即当前选中的文本）
    fn get_primary_selection() -> Result<String, Box<dyn std::error::Error>> {
        use std::process::Command;
        
        // 尝试使用 xclip 获取 PRIMARY 选择
        let output = Command::new("xclip").args(&["-o", "-selection", "primary"]).output();
        
        if let Ok(output) = output {
            let text = String::from_utf8(output.stdout)?;
            return Ok(text.trim_end_matches('\n').to_string());
        }
        
        // 如果 xclip 不可用，尝试使用 xsel
        let output = Command::new("xsel").args(&["--output", "--primary"]).output();
        
        if let Ok(output) = output {
            let text = String::from_utf8(output.stdout)?;
            return Ok(text.trim_end_matches('\n').to_string());
        }
        
        // 如果以上都不行，返回空字符串
        Ok(String::new())
    }

    /// 获取 CLIPBOARD 剪贴板内容
    fn get_clipboard_content() -> Result<String, Box<dyn std::error::Error>> {
        use std::process::Command;
        
        let output = Command::new("xclip").args(&["-o", "-selection", "clipboard"]).output();
        
        if let Ok(output) = output {
            let text = String::from_utf8(output.stdout)?;
            return Ok(text.trim_end_matches('\n').to_string());
        }
        
        // 如果 xclip 不可用，尝试使用 xsel
        let output = Command::new("xsel").args(&["--output", "--clipboard"]).output();
        
        if let Ok(output) = output {
            let text = String::from_utf8(output.stdout)?;
            return Ok(text.trim_end_matches('\n').to_string());
        }
        
        // 如果以上都不行，返回空字符串
        Ok(String::new())
    }

    /// 检查系统是否安装了必要的工具
    fn check_dependencies() -> bool {
        use std::process::Command;
        
        // 检查 xclip 是否可用
        if Command::new("xclip").arg("--version").output().is_ok() {
            return true;
        }
        
        // 检查 xsel 是否可用
        if Command::new("xsel").arg("--version").output().is_ok() {
            return true;
        }
        
        false
    }

    /// 启动Linux划词监听器
    pub fn start_linux_text_selection_listener(app_handle: AppHandle) {
        if !check_dependencies() {
            eprintln!("Warning: Neither xclip nor xsel found. Install one to enable text selection detection.");
            return;
        }

        thread::spawn(move || {
            let app_handle = Arc::new(Mutex::new(app_handle));
            let mut last_primary_content = String::new();

            loop {
                thread::sleep(Duration::from_millis(100)); // 每100ms检查一次

                // 获取当前 PRIMARY 选择内容
                if let Ok(current_content) = get_primary_selection() {
                    // 检测到 PRIMARY 选择内容变化，这通常表示文本被选中
                    if !current_content.is_empty() && current_content != last_primary_content {
                        // 检查内容是否为合理的选择文本
                        if is_reasonable_selection(&current_content) {
                            let app_handle_clone = app_handle.lock().unwrap().clone();
                            let selected_text = current_content.clone();
                            
                            // 发送选中文本到前端
                            let _ = app_handle_clone.emit("selected-text", selected_text.clone());
                            // 显示划词工具栏
                            show_selection_toolbar(&app_handle_clone, selected_text);
                        }
                    }
                    last_primary_content = current_content;
                }
            }
        });
    }

    /// 检查是否为合理的文本选择
    fn is_reasonable_selection(content: &str) -> bool {
        let content = content.trim();
        
        if content.is_empty() || content.len() < 2 || content.len() > 500 {
            return false;
        }
        
        // 排除URL
        if content.contains("://") || content.starts_with("www.") {
            return false;
        }
        
        // 排除邮箱地址（限制只有一个@符号，超过则可能是其他内容）
        if content.matches('@').count() > 1 {
            return false;
        }
        
        // 排除时间格式（简单的检测）
        if content.len() >= 5 && content.chars().nth(2) == Some(':') && content.chars().nth(1).unwrap().is_ascii_digit() {
            return false;
        }
        
        true
    }

    /// 显示划词工具栏
    fn show_selection_toolbar(app_handle: &AppHandle, selected_text: String) {
        // 发送命令到前端显示划词工具栏
        let _ = app_handle.emit("show-selection-toolbar", selected_text);
    }

    /// 停止Linux划词监听器
    pub fn stop_linux_text_selection_listener() {
        // 在 Linux 上不需要特殊清理操作
    }
}

#[cfg(target_os = "linux")]
pub use linux_impl::*;

#[cfg(not(target_os = "linux"))]
pub fn start_linux_text_selection_listener(_: tauri::AppHandle) {
    // 非Linux平台不实现此功能
}

#[cfg(not(target_os = "linux"))]
pub fn stop_linux_text_selection_listener() {
    // 非Linux平台不实现此功能
}