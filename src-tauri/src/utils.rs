use serde::{Deserialize, Serialize};
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
    pub ai_api_url: String,
    pub ai_model_name: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub ai_api_key: String,
    #[serde(default)]
    pub encrypted_api_key: String,
}

impl Default for AppSettingsData {
    fn default() -> Self {
        Self {
            version: get_default_app_version(),
            max_items: 50,
            ai_api_url: String::new(),
            ai_model_name: String::new(),
            ai_api_key: String::new(),
            encrypted_api_key: String::new(),
        }
    }
}

impl AppSettingsData {
    /// 加密API密钥
    pub fn encrypt_api_key(&mut self) -> Result<(), String> {
        if self.ai_api_key.is_empty() {
            self.encrypted_api_key.clear();
            return Ok(());
        }

        let encrypted: Vec<u8> = self
            .ai_api_key
            .bytes()
            .enumerate()
            .map(|(i, b)| b ^ ENCRYPTION_KEY[i % ENCRYPTION_KEY.len()])
            .collect();

        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;
        self.encrypted_api_key = STANDARD.encode(encrypted);
        self.ai_api_key.clear();
        Ok(())
    }

    /// 解密API密钥
    pub fn decrypt_api_key(&mut self) -> Result<(), String> {
        if self.encrypted_api_key.is_empty() {
            self.ai_api_key.clear();
            return Ok(());
        }

        // 使用新的base64 Engine API
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;
        let encrypted = STANDARD
            .decode(&self.encrypted_api_key)
            .map_err(|e| format!("解密失败: {}", e))?;

        let decrypted: Vec<u8> = encrypted
            .iter()
            .enumerate()
            .map(|(i, &b)| b ^ ENCRYPTION_KEY[i % ENCRYPTION_KEY.len()])
            .collect();

        self.ai_api_key =
            String::from_utf8(decrypted).map_err(|e| format!("UTF-8解码失败: {}", e))?;
        Ok(())
    }

    /// 验证设置有效性
    pub fn validate(&self) -> Result<(), String> {
        if self.max_items == 0 || self.max_items > 1000 {
            return Err("max_items必须在1-1000之间".to_string());
        }

        if !self.ai_api_url.is_empty() && !self.ai_api_url.starts_with("http") {
            return Err("AI API URL必须以http或https开头".to_string());
        }

        Ok(())
    }
    /// 获取部分隐藏的API密钥（用于前端显示）
    pub fn get_masked_api_key(&self) -> String {
        if self.ai_api_key.is_empty() {
            return String::new();
        }

        let key = &self.ai_api_key;
        let len = key.len();

        if len <= 16 {
            // 如果密钥长度小于等于16，全部显示为*
            return "*".repeat(len.min(30));
        }

        // 前8个字符 + 30个* + 后8个字符
        let prefix = &key[..8.min(len)];
        let suffix = &key[len - 8.min(len - 8)..];

        format!("{}{}{}", prefix, "*".repeat(30), suffix)
    }

    /// 迁移旧版本设置
    pub fn migrate_from_old(&mut self) {
        if let Ok(old_version) = self.version.parse::<u32>() {
            if old_version == 0 {
                self.version = get_default_app_version();
                if !self.ai_api_key.is_empty() && self.encrypted_api_key.is_empty() {
                    let _ = self.encrypt_api_key();
                }
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
        let json = serde_json::to_string_pretty(&AppSettingsData::default())
            .map_err(|e| format!("序列化默认设置失败: {}", e))?;
        std::fs::write(&settings_path, json).map_err(|e| format!("创建设置文件失败: {}", e))?;
        return Ok(AppSettingsData::default());
    }
    let contents =
        std::fs::read_to_string(&settings_path).map_err(|e| format!("读取设置文件失败: {}", e))?;

    let mut settings: AppSettingsData =
        serde_json::from_str(&contents).map_err(|e| format!("解析设置文件失败: {}", e))?;

    settings.migrate_from_old();

    // 解密API密钥以便前端使用
    settings.decrypt_api_key()?;

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
