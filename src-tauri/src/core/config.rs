//! 应用程序配置常量

use enigo::Key;
use serde::{Deserialize, Serialize};
/// 剪贴板窗口与任务栏之间的额外安全边距（像素）
pub const CLIPBOARD_WINDOW_BOTTOM_EXTRA_MARGIN: i32 = 8;
/// 默认切换快捷键（根据操作系统自动适配）
pub const DEFAULT_TOGGLE_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "Cmd+Shift+z"
} else {
    "Ctrl+Shift+z"
};
pub const DEFAULT_IMAGE_TOGGLE_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "Cmd+Shift+x"
} else {
    "Ctrl+Shift+x"
};
pub const DEFAULT_SCREENSHOT_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "Cmd+Shift+s"
} else {
    "Ctrl+Shift+s"
};
pub const DEFAULT_RECORDING_SHORTCUT: &str = "Alt+R";
pub const DEFAULT_LAUNCHER_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "Cmd+K"
} else {
    "Ctrl+K"
};
pub const DEFAULT_DOC_MANAGER_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "Cmd+Shift+D"
} else {
    "Ctrl+Shift+D"
};
/// Ctrl+C操作中的控制键（根据操作系统自动适配）
pub const CTRL_KEY: Key = if cfg!(target_os = "macos") {
    Key::Meta
} else {
    Key::Control
};

/// AI服务提供商配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderConfig {
    pub api_url: String,
    pub model_name: String,
}
