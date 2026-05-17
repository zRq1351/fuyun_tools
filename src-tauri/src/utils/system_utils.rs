use crate::core::error_codes::AppErrorKind;
use crate::utils::settings_model::{initialize_builtin_providers, AppSettingsData};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static SAVE_SETTINGS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn get_default_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub fn get_settings_file_path() -> PathBuf {
    let mut settings_dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    settings_dir.pop();
    settings_dir.push("settings.json");
    settings_dir
}

pub fn get_logs_dir_path() -> PathBuf {
    let mut logs_dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    logs_dir.pop();
    logs_dir.push("logs");
    logs_dir
}

fn get_backup_file_path(path: &Path) -> PathBuf {
    let mut backup_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "data".to_string());
    backup_name.push_str(".bak");
    path.with_file_name(backup_name)
}

pub fn atomic_write_with_backup(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppErrorKind::IoError.to_frontend_json_with_details(format!("{}", e)))?;
    }

    let mut tmp_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "data".to_string());
    tmp_name.push_str(".tmp");
    let tmp_path = path.with_file_name(tmp_name);
    let backup_path = get_backup_file_path(path);

    fs::write(&tmp_path, bytes)
        .map_err(|e| AppErrorKind::IoError.to_frontend_json_with_details(format!("{}", e)))?;

    if path.exists() {
        if backup_path.exists() {
            let _ = fs::remove_file(&backup_path);
        }
        fs::copy(path, &backup_path)
            .map_err(|e| AppErrorKind::IoError.to_frontend_json_with_details(format!("{}", e)))?;
    }

    match fs::rename(&tmp_path, path) {
        Ok(_) => {
            let _ = fs::remove_file(&backup_path);
            Ok(())
        }
        Err(rename_error) => {
            let _ = fs::remove_file(&tmp_path);
            if backup_path.exists() {
                let _ = fs::copy(&backup_path, path);
            }
            Err(AppErrorKind::IoError.to_frontend_json_with_details(format!("{}", rename_error)))
        }
    }
}

pub fn read_text_with_backup(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(primary_error) => {
            let backup_path = get_backup_file_path(path);
            if !backup_path.exists() {
                return Err(AppErrorKind::IoError.to_frontend_json_with_details(format!("{}", primary_error)));
            }

            let backup_content = fs::read_to_string(&backup_path).map_err(|e| {
                AppErrorKind::IoError.to_frontend_json_with_details(format!(
                    "主文件错误: {}，备份错误: {}",
                    primary_error, e
                ))
            })?;
            let _ = atomic_write_with_backup(path, backup_content.as_bytes());
            Ok(backup_content)
        }
    }
}

pub fn save_settings(settings: &AppSettingsData) -> Result<(), String> {
    let _lock = SAVE_SETTINGS_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let settings_path = get_settings_file_path();
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| AppErrorKind::JsonError.to_frontend_json_with_details(format!("{}", e)))?;
    atomic_write_with_backup(&settings_path, json.as_bytes())?;
    Ok(())
}

pub fn load_settings() -> Result<AppSettingsData, String> {
    let settings_path = get_settings_file_path();
    if !settings_path.exists() {
        log::info!("首次运行，创建默认设置文件");
        let mut default_settings = AppSettingsData::default();
        initialize_builtin_providers(&mut default_settings);
        let json = serde_json::to_string_pretty(&default_settings)
            .map_err(|e| AppErrorKind::JsonError.to_frontend_json_with_details(format!("{}", e)))?;
        atomic_write_with_backup(&settings_path, json.as_bytes())?;
        return Ok(default_settings);
    }

    let contents = read_text_with_backup(&settings_path)?;
    let mut settings: AppSettingsData = serde_json::from_str(&contents)
        .map_err(|e| AppErrorKind::JsonError.to_frontend_json_with_details(format!("{}", e)))?;
    let keys_migrated = settings.migrate_legacy_api_keys();
    let old_version = settings.version.clone();
    settings.migrate_from_old();

    // 反序列化后再序列化，如果两者内容不同，说明有缺失的默认字段被补全
    let new_contents = serde_json::to_string_pretty(&settings)
        .map_err(|e| AppErrorKind::JsonError.to_frontend_json_with_details(format!("{}", e)))?;
    let fields_added = contents != new_contents;

    // 只有在版本变化、密钥迁移或字段添加时才保存，避免频繁IO
    if old_version != settings.version || keys_migrated || fields_added {
        log::info!("配置已更新或补全缺失字段，保存到文件");
        save_settings(&settings)?;
    }
    
    let _provider_key = settings.ai_provider.to_string();
    Ok(settings)
}
