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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_shortcuts_non_empty() {
        assert!(!DEFAULT_TOGGLE_SHORTCUT.is_empty());
        assert!(!DEFAULT_IMAGE_TOGGLE_SHORTCUT.is_empty());
        assert!(!DEFAULT_SCREENSHOT_SHORTCUT.is_empty());
        assert!(!DEFAULT_RECORDING_SHORTCUT.is_empty());
        assert!(!DEFAULT_LAUNCHER_SHORTCUT.is_empty());
        assert!(!DEFAULT_DOC_MANAGER_SHORTCUT.is_empty());
    }

    #[test]
    fn test_recording_shortcut_is_alt_r() {
        assert_eq!(DEFAULT_RECORDING_SHORTCUT, "Alt+R");
    }

    #[test]
    fn test_provider_config_serde_roundtrip() {
        let cfg = ProviderConfig {
            api_url: "https://api.example.com".to_string(),
            model_name: "model-x".to_string(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn test_provider_config_partial_eq() {
        let a = ProviderConfig {
            api_url: "u".to_string(),
            model_name: "m".to_string(),
        };
        let b = a.clone();
        assert_eq!(a, b);
        assert_ne!(
            a,
            ProviderConfig {
                api_url: "u2".to_string(),
                model_name: "m".to_string(),
            }
        );
    }

    #[test]
    fn test_ctrl_key_defined() {
        // 只要常量存在且可用即可
        let _ = CTRL_KEY;
    }
}
