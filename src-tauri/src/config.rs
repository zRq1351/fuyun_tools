//! 应用程序配置常量

use enigo::Key;
use serde::{Deserialize, Serialize};
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
    fn default() -> Self {
        AIProvider::DeepSeek
    }
}

impl std::fmt::Display for AIProvider {
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
