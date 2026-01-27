//! macOS 专用划词功能实现
//! 使用 Accessibility API 监听划词结束事件

#[cfg(target_os = "macos")]
mod macos_impl {
    use core_foundation::base::{CFRelease, CFType, TCFType};
    use core_foundation::dict::CFDictionary;
    use core_foundation::string::CFString;
    use core_graphics::event::{CGEvent, CGEventTapLocation};
    use core_graphics::event_source::CGEventSource;
    use core_graphics::event_types::EventType;
    use core_graphics::hot_key::{CGHotKey, CGHotKeyCallback};
    use objc::runtime::{Class, Object};
    use objc::{class, msg_send, sel, sel_impl};
    use std::ffi::c_void;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use tauri::AppHandle;

    // 全局变量存储应用状态
    static mut IS_SELECTING: bool = false;
    static mut PREVIOUS_SELECTED_TEXT: String = String::new();

    /// 获取当前选中的文本
    fn get_selected_text() -> Result<String, Box<dyn std::error::Error>> {
        // 使用 AppleScript 获取当前选中文本
        use std::process::Command;

        // 尝试通过 AppleScript 获取选中文本
        let script = r#"
            try
                tell application "System Events"
                    set frontApp to name of first application process whose frontmost is true
                    set frontAppId to unix id of first application process whose frontmost is true
                end tell
                
                if frontApp is "Finder" then
                    -- Finder 中获取选中文件名
                    tell application "Finder"
                        if selection contains items then
                            set selectedItems to selection
                            set selectedText to ""
                            repeat with anItem in selectedItems
                                set itemName to name of anItem
                                if selectedText is "" then
                                    set selectedText to itemName
                                else
                                    set selectedText to selectedText & "\n" & itemName
                                end if
                            end repeat
                            return selectedText
                        end if
                    end tell
                else
                    -- 其他应用中尝试获取剪贴板临时内容
                    set originalClipboard to the clipboard
                    try
                        keystroke "c" using {command down}
                        delay 0.05
                        set selectedText to the clipboard
                        set the clipboard to originalClipboard
                        return selectedText
                    on error
                        set the clipboard to originalClipboard
                        error "Could not retrieve selected text"
                    end try
                end if
            on error
                error "Could not retrieve selected text"
            end try
        "#;

        // 由于直接执行AppleScript可能不安全，我们采用另一种方式
        // 模拟复制快捷键并获取剪贴板内容
        Ok(simulate_copy_and_get_text()?)
    }

    /// 模拟Cmd+C并获取剪贴板内容
    fn simulate_copy_and_get_text() -> Result<String, Box<dyn std::error::Error>> {
        use std::process::Command;
        use std::thread;
        use std::time::Duration;

        // 保存原始剪贴板内容
        let original_content = get_clipboard_text().unwrap_or_default();

        // 使用 AppleScript 模拟 Cmd+C
        let apple_script = r#"tell application "System Events" to keystroke "c" using {command down}"#;
        Command::new("osascript").arg("-e").arg(apple_script).output()?;

        // 等待剪贴板更新
        thread::sleep(Duration::from_millis(50));

        // 获取新的剪贴板内容
        let new_content = get_clipboard_text().unwrap_or_default();

        // 恢复原始剪贴板内容
        if !original_content.is_empty() {
            set_clipboard_text(&original_content).ok();
        }

        Ok(new_content)
    }

    /// 获取剪贴板文本
    fn get_clipboard_text() -> Result<String, Box<dyn std::error::Error>> {
        use std::process::Command;
        
        let output = Command::new("pbpaste").output()?;
        let text = String::from_utf8(output.stdout)?;
        Ok(text.trim_end_matches('\n').to_string())
    }

    /// 设置剪贴板文本
    fn set_clipboard_text(text: &str) -> Result<(), Box<dyn std::error::Error>> {
        use std::process::Command;
        use std::io::Write;

        let mut child = Command::new("pbcopy").stdin(std::process::Stdio::piped()).spawn()?;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(text.as_bytes())?;
        drop(stdin);
        child.wait()?;

        Ok(())
    }

    /// 启动macOS划词监听器
    pub fn start_macos_text_selection_listener(app_handle: AppHandle) {
        thread::spawn(move || {
            let app_handle = Arc::new(Mutex::new(app_handle));
            let mut last_clipboard_content = String::new();

            loop {
                thread::sleep(Duration::from_millis(100)); // 每100ms检查一次

                // 获取当前剪贴板内容
                if let Ok(current_content) = get_clipboard_text() {
                    // 检测到剪贴板内容变化，可能是划词复制
                    if !current_content.is_empty() && current_content != last_clipboard_content {
                        // 检查内容是否为合理的选择文本（不是URL、邮件等）
                        if is_reasonable_selection(&current_content) {
                            // 延迟一小段时间，确保是划词操作而不是用户主动复制
                            thread::sleep(Duration::from_millis(50));
                            
                            // 再次检查剪贴板内容是否一致
                            if let Ok(verify_content) = get_clipboard_text() {
                                if verify_content == current_content {
                                    let app_handle_clone = app_handle.lock().unwrap().clone();
                                    let selected_text = current_content.clone();
                                    
                                    // 发送选中文本到前端
                                    let _ = app_handle_clone.emit("selected-text", selected_text.clone());
                                    // 显示划词工具栏
                                    show_selection_toolbar(&app_handle_clone, selected_text);
                                }
                            }
                        }
                    }
                    last_clipboard_content = current_content;
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

    /// 停止macOS划词监听器
    pub fn stop_macos_text_selection_listener() {
        // 在 macOS 上不需要特殊清理操作
    }
}

#[cfg(target_os = "macos")]
pub use macos_impl::*;

#[cfg(not(target_os = "macos"))]
pub fn start_macos_text_selection_listener(_: tauri::AppHandle) {
    // 非macOS平台不实现此功能
}

#[cfg(not(target_os = "macos"))]
pub fn stop_macos_text_selection_listener() {
    // 非macOS平台不实现此功能
}