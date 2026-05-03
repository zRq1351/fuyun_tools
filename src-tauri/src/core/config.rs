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
pub const DEFAULT_RECORDING_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "Alt+R"
} else {
    "Alt+R"
};
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

pub const C_KEY: Key = if cfg!(target_os = "macos") {
    Key::Unicode('c')
} else {
    Key::Insert
};

/// AI服务提供商枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AIProvider {
    #[serde(rename = "deepseek")]
    DeepSeek,
    #[serde(rename = "qwen")]
    Qwen,
    #[serde(rename = "xiaomimimo")]
    XiaoMiMimo,
}

impl Default for AIProvider {
    /// 默认AI提供商
    fn default() -> Self {
        AIProvider::DeepSeek
    }
}

impl std::fmt::Display for AIProvider {
    /// 格式化显示
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AIProvider::DeepSeek => "deepseek",
            AIProvider::Qwen => "qwen",
            AIProvider::XiaoMiMimo => "xiaomimimo",
        };
        write!(f, "{}", s)
    }
}

impl AIProvider {
    /// 获取提供商的默认配置
    pub fn get_default_config(&self) -> (String, String) {
        match self {
            AIProvider::DeepSeek => (
                "https://api.deepseek.com/v1".to_string(),
                "deepseek-chat".to_string(),
            ),
            AIProvider::Qwen => (
                "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
                "qwen-plus".to_string(),
            ),
            AIProvider::XiaoMiMimo => (
                "https://api.xiaomimimo.com/v1".to_string(),
                "mimo-v2-flash".to_string(),
            ),
        }
    }
}

/// 单个AI提供商的配置
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ProviderConfig {
    pub api_url: String,
    pub model_name: String,
    #[serde(default)]
    pub encrypted_api_key: String,
}
