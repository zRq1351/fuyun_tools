use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppSettingsData {
    pub max_items: usize,
}

impl Default for AppSettingsData {
    fn default() -> Self {
        Self { max_items: 50 }
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

    let settings: AppSettingsData =
        serde_json::from_str(&contents).map_err(|e| format!("解析设置文件失败: {}", e))?;
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
    std::path::PathBuf::from("logs")
}
