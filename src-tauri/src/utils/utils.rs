use crate::core::config::ProviderConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

const ENCRYPTION_KEY: &[u8] = b"fuyun_tools_encryption_key_2025!"; // 32字节密钥

/// 获取应用默认版本号
pub fn get_default_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppSettingsData {
    pub version: String,
    pub max_items: usize,
    #[serde(default)]
    pub ai_provider: String,  // 改为String类型以支持自定义提供商
    /// 每个AI提供商的独立配置
    #[serde(default)]
    pub provider_configs: HashMap<String, ProviderConfig>,
}

impl Default for AppSettingsData {
    fn default() -> Self {
        Self {
            version: get_default_app_version(),
            max_items: 50,
            ai_provider: "deepseek".to_string(),  // 默认使用字符串
            provider_configs: HashMap::new(),
        }
    }
}

impl AppSettingsData {
    /// 为指定提供商加密API密钥
    pub fn encrypt_provider_api_key(&mut self, provider_key: &str, api_key: &str) -> Result<(), String> {
        if let Some(config) = self.provider_configs.get_mut(provider_key) {
            if api_key.is_empty() {
                config.encrypted_api_key.clear();
                return Ok(());
            }

            let encrypted: Vec<u8> = api_key
                .bytes()
                .enumerate()
                .map(|(i, b)| b ^ ENCRYPTION_KEY[i % ENCRYPTION_KEY.len()])
                .collect();

            use base64::engine::general_purpose::STANDARD;
            use base64::Engine as _;
            config.encrypted_api_key = STANDARD.encode(encrypted);
        }
        Ok(())
    }

    /// 为指定提供商解密API密钥
    pub fn decrypt_provider_api_key(&self, provider_key: &str) -> Result<String, String> {
        if let Some(config) = self.provider_configs.get(provider_key) {
            if config.encrypted_api_key.is_empty() {
                return Ok(String::new());
            }

            use base64::engine::general_purpose::STANDARD;
            use base64::Engine as _;
            let encrypted = STANDARD
                .decode(&config.encrypted_api_key)
                .map_err(|e| format!("解密失败: {}", e))?;

            let decrypted: Vec<u8> = encrypted
                .iter()
                .enumerate()
                .map(|(i, &b)| b ^ ENCRYPTION_KEY[i % ENCRYPTION_KEY.len()])
                .collect();

            String::from_utf8(decrypted).map_err(|e| format!("UTF-8解码失败: {}", e))
        } else {
            Ok(String::new())
        }
    }

    /// 保存当前提供商的配置
    pub fn save_current_provider_config(&mut self, api_key: &str) -> Result<(), String> {
        let provider_key = self.ai_provider.clone();  // 克隆避免借用冲突

        // 加密该提供商的API密钥
        self.encrypt_provider_api_key(&provider_key, api_key)?;

        Ok(())
    }

    /// 加载指定提供商的配置到当前设置
    pub fn load_provider_config_to_current(
        &mut self,
        provider_name: &str,  // 改为接受字符串参数
    ) -> Result<ProviderConfig, String> {
        let provider_key = provider_name.to_string();

        // 先获取配置的副本
        let config_copy = if let Some(config) = self.provider_configs.get(&provider_key) {
            config.clone()
        } else {
            // 对于内置提供商，获取默认配置
            let (default_url, default_model) = match provider_name {
                "deepseek" => (
                    "https://api.deepseek.com/v1".to_string(),
                    "deepseek-chat".to_string(),
                ),
                "qwen" => (
                    "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
                    "qwen-plus".to_string(),
                ),
                "xiaomimimo" => (
                    "https://api.xiaomimimo.com/v1".to_string(),
                    "mimo-v2-flash".to_string(),
                ),
                _ => {
                    // 自定义提供商使用空默认值
                    (String::new(), String::new())
                }
            };
            ProviderConfig {
                api_url: default_url,
                model_name: default_model,
                encrypted_api_key: String::new(),
            }
        };

        // 解密该提供商的API密钥
        let _ = self.decrypt_provider_api_key(&provider_key);

        // 更新当前提供商
        self.ai_provider = provider_name.to_string();

        // 如果是已存在的配置，需要重新获取解密后的版本
        if self.provider_configs.contains_key(&provider_key) {
            if let Some(decrypted_config) = self.provider_configs.get(&provider_key) {
                Ok(decrypted_config.clone())
            } else {
                Ok(config_copy)
            }
        } else {
            Ok(config_copy)
        }
    }

    /// 获取当前提供商的配置信息
    pub fn get_current_provider_config(&self) -> Option<&ProviderConfig> {
        self.provider_configs.get(&self.ai_provider)
    }

    /// 验证设置有效性
    pub fn validate(&self) -> Result<(), String> {
        if self.max_items == 0 || self.max_items > 1000 {
            return Err("max_items必须在1-1000之间".to_string());
        }

        Ok(())
    }

    /// 获取部分隐藏的API密钥（用于前端显示）
    pub fn get_masked_api_key(&self) -> String {
        // 解密当前提供商的API密钥
        match self.decrypt_provider_api_key(&self.ai_provider) {
            Ok(api_key) => {
                if api_key.is_empty() {
                    return String::new();
                }

                let len = api_key.len();

                if len <= 16 {
                    // 如果密钥长度小于等于16，全部显示为*
                    return "*".repeat(len.min(30));
                }

                // 前8个字符 + 30个* + 后8个字符
                let prefix = &api_key[..8.min(len)];
                let suffix = &api_key[len - 8.min(len - 8)..];

                format!("{}{}{}", prefix, "*".repeat(30), suffix)
            }
            Err(_) => String::new(),
        }
    }

    /// 迁移旧版本设置
    pub fn migrate_from_old(&mut self) {
        if let Ok(old_version) = self.version.parse::<u32>() {
            if old_version == 0 {
                self.version = get_default_app_version();
            }
        } else if self.version != get_default_app_version() {
            self.version = get_default_app_version();
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ClipboardHistoryData {
    pub items: Vec<String>,
}
/// 获取设置文件路径
pub fn get_settings_file_path() -> PathBuf {
    let mut settings_dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    settings_dir.pop();
    settings_dir.push("settings.json");
    settings_dir
}

/// 获取历史记录文件路径
pub fn get_history_file_path() -> PathBuf {
    let mut history_dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    history_dir.pop();
    history_dir.push("history.json");
    history_dir
}

/// 保存设置到文件
pub fn save_settings(settings: &AppSettingsData) -> Result<(), String> {
    let settings_path = get_settings_file_path();
    let json =
        serde_json::to_string_pretty(settings).map_err(|e| format!("序列化设置失败: {}", e))?;
    std::fs::write(&settings_path, json).map_err(|e| format!("写入设置文件失败: {}", e))?;
    Ok(())
}

/// 从文件加载设置
pub fn load_settings() -> Result<AppSettingsData, String> {
    let settings_path = get_settings_file_path();

    if !settings_path.exists() {
        // 首次运行，初始化默认设置和内置提供商配置
        let mut default_settings = AppSettingsData::default();

        // 初始化内置提供商的默认配置
        initialize_builtin_providers(&mut default_settings);

        let json = serde_json::to_string_pretty(&default_settings)
            .map_err(|e| format!("序列化默认设置失败: {}", e))?;
        std::fs::write(&settings_path, json).map_err(|e| format!("创建设置文件失败: {}", e))?;
        return Ok(default_settings);
    }
    let contents =
        std::fs::read_to_string(&settings_path).map_err(|e| format!("读取设置文件失败: {}", e))?;

    let mut settings: AppSettingsData =
        serde_json::from_str(&contents).map_err(|e| format!("解析设置文件失败: {}", e))?;

    settings.migrate_from_old();

    // 解密当前提供商的API密钥
    let _provider_key = settings.ai_provider.to_string();
    // 解密操作已经在 decrypt_provider_api_key 方法中处理

    Ok(settings)
}

/// 保存剪切板历史记录到文件
pub fn save_history(history: &[String]) -> Result<(), String> {
    let history_path = get_history_file_path();

    let history_data = ClipboardHistoryData {
        items: history.to_vec(),
    };

    let json = serde_json::to_string_pretty(&history_data)
        .map_err(|e| format!("序列化历史记录失败: {}", e))?;

    std::fs::write(&history_path, json).map_err(|e| format!("写入历史记录文件失败: {}", e))?;

    Ok(())
}

/// 带重试机制的保存历史记录函数
pub fn save_history_with_retry(history: &[String], max_retries: u32) -> Result<(), String> {
    let mut attempts = 0;
    loop {
        match save_history(history) {
            Ok(()) => return Ok(()),
            Err(e) if attempts >= max_retries => return Err(e),
            Err(_) => {
                attempts += 1;
                thread::sleep(Duration::from_millis((100 * attempts).into()));
            }
        }
    }
}

/// 从文件加载剪切板历史记录
pub fn load_history() -> Result<Vec<String>, String> {
    let history_path = get_history_file_path();

    if !history_path.exists() {
        return Ok(vec![]);
    }

    let contents = std::fs::read_to_string(&history_path)
        .map_err(|e| format!("读取历史记录文件失败: {}", e))?;

    let history_data: ClipboardHistoryData =
        serde_json::from_str(&contents).map_err(|e| format!("解析历史记录文件失败: {}", e))?;

    Ok(history_data.items)
}

/// 获取日志目录路径
pub fn get_logs_dir_path() -> PathBuf {
    PathBuf::from("logs")
}

/// 初始化内置提供商配置
fn initialize_builtin_providers(settings: &mut AppSettingsData) {
    use crate::core::config::{AIProvider, ProviderConfig};

    // 为每个内置提供商创建默认配置
    let builtin_providers = [
        AIProvider::DeepSeek,
        AIProvider::Qwen,
        AIProvider::XiaoMiMimo,
    ];

    for provider in builtin_providers {
        let provider_key = provider.to_string();
        let (default_url, default_model) = provider.get_default_config();

        let config = ProviderConfig {
            api_url: default_url,
            model_name: default_model,
            encrypted_api_key: String::new(),
        };

        settings.provider_configs.insert(provider_key, config);
    }

    log::info!("已初始化内置AI提供商配置");
}