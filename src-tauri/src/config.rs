//! 应用程序配置常量

use enigo::Key;
use std::time::Duration;

// 剪贴板监听配置
pub const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(100);
// 快捷键配置
pub const DEFAULT_TOGGLE_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "Cmd+Shift+k"
} else {
    "Ctrl+Shift+k"
};
pub const DEFAULT_HIDE_SHORTCUT: &str = "Escape";
// 记录数配置项
pub const MAX_ITEMS_OPTIONS: &[usize] = &[10, 20, 50, 100];

// ctrl+c中的ctrl键
pub const CTRL_KEY: Key = if cfg!(target_os = "macos") {
    Key::Meta
} else {
    Key::Control
};
