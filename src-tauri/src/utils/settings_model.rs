use crate::core::config::{
    ProviderConfig, DEFAULT_IMAGE_TOGGLE_SHORTCUT, DEFAULT_RECORDING_SHORTCUT,
    DEFAULT_SCREENSHOT_SHORTCUT, DEFAULT_TOGGLE_SHORTCUT,
};
use crate::utils::system_utils::get_default_app_version;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;

const LEGACY_ENCRYPTION_KEY: &[u8] = b"fuyun_tools_encryption_key_2025!";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CustomPrompt {
    pub name: String,
    pub prompt: String,
    #[serde(default = "default_custom_prompt_icon")]
    pub icon: String,
    #[serde(default = "default_custom_prompt_color")]
    pub color: String,
    #[serde(default = "default_custom_prompt_bg_color")]
    pub bg_color: String,
    #[serde(default = "default_custom_prompt_enabled")]
    pub enabled: bool,
}

fn default_custom_prompt_icon() -> String {
    "Star".to_string()
}

fn default_custom_prompt_color() -> String {
    "#909399".to_string()
}

fn default_custom_prompt_bg_color() -> String {
    "rgba(255, 255, 255, 0.1)".to_string()
}

fn default_custom_prompt_enabled() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppSettingsData {
    pub version: String,
    pub max_items: usize,
    #[serde(default = "default_text_max_items")]
    pub text_max_items: usize,
    #[serde(default = "default_image_max_items")]
    pub image_max_items: usize,
    #[serde(default = "default_image_disk_limit_mb")]
    pub image_disk_limit_mb: u64,
    pub hot_key: String,
    #[serde(default = "default_text_clipboard_enabled")]
    pub text_clipboard_enabled: bool,
    #[serde(default = "default_image_hot_key")]
    pub image_hot_key: String,
    #[serde(default = "default_image_clipboard_enabled")]
    pub image_clipboard_enabled: bool,
    #[serde(default = "default_screenshot_hot_key")]
    pub screenshot_hot_key: String,
    #[serde(default = "default_screenshot_enabled")]
    pub screenshot_enabled: bool,
    #[serde(default = "default_recording_hot_key")]
    pub recording_hot_key: String,
    #[serde(default = "default_recording_mic_toggle_hot_key")]
    pub recording_mic_toggle_hot_key: String,
    #[serde(default = "default_recording_enabled")]
    pub recording_enabled: bool,
    #[serde(default = "default_recording_default_fps")]
    pub recording_default_fps: u32,
    #[serde(default = "default_recording_default_video_bitrate_kbps")]
    pub recording_default_video_bitrate_kbps: u32,
    #[serde(default = "default_recording_default_audio_bitrate_kbps")]
    pub recording_default_audio_bitrate_kbps: u32,
    #[serde(default = "default_recording_capture_cursor")]
    pub recording_capture_cursor: bool,
    #[serde(default = "default_recording_capture_system_audio")]
    pub recording_capture_system_audio: bool,
    #[serde(default = "default_recording_capture_microphone")]
    pub recording_capture_microphone: bool,
    #[serde(default)]
    pub recording_microphone_device_id: String,
    #[serde(default = "default_recording_output_dir")]
    pub recording_output_dir: String,
    #[serde(default = "default_recording_auto_open_folder")]
    pub recording_auto_open_folder: bool,
    #[serde(default = "default_recording_toolbar_content_protected")]
    pub recording_toolbar_content_protected: bool,
    #[serde(default = "default_recording_max_duration_minutes")]
    pub recording_max_duration_minutes: u32,
    #[serde(default = "default_recording_file_name_template")]
    pub recording_file_name_template: String,
    #[serde(default = "default_recording_ffmpeg_download_url")]
    pub recording_ffmpeg_download_url: String,
    #[serde(default = "default_recording_window_audio_sync_advance_ms")]
    pub recording_window_audio_sync_advance_ms: u32,
    #[serde(default = "default_recording_wgc_force_default_border")]
    pub recording_wgc_force_default_border: bool,
    #[serde(default = "default_recording_wgc_force_default_dirty_region")]
    pub recording_wgc_force_default_dirty_region: bool,
    #[serde(default = "default_dev_force_ffmpeg_window_capture")]
    pub dev_force_ffmpeg_window_capture: bool,
    #[serde(default)]
    pub ai_provider: String,
    #[serde(default)]
    pub provider_configs: HashMap<String, ProviderConfig>,
    #[serde(default = "default_selection_enabled")]
    pub selection_enabled: bool,
    #[serde(default = "default_selection_modifier_key")]
    pub selection_modifier_key: String,
    #[serde(default = "default_selection_custom_prompts")]
    pub selection_custom_prompts: Vec<CustomPrompt>,
    #[serde(default = "default_selection_web_search_enabled")]
    pub selection_web_search_enabled: bool,
    #[serde(default = "default_selection_web_search_engine")]
    pub selection_web_search_engine: String,
    #[serde(default = "default_grouped_items_protected_from_limit")]
    pub grouped_items_protected_from_limit: bool,
    #[serde(default = "default_clipboard_bottom_offset")]
    pub clipboard_bottom_offset: i32,
    #[serde(default = "default_translation_prompt_template")]
    pub translation_prompt_template: String,
    #[serde(default = "default_explanation_prompt_template")]
    pub explanation_prompt_template: String,
    #[serde(default = "default_image_fill_verify_mode")]
    pub image_fill_verify_mode: String,
    #[serde(default = "default_ocr_engine")]
    pub ocr_engine: String,
    #[serde(default = "default_backup_enabled")]
    pub backup_enabled: bool,
    #[serde(default = "default_backup_frequency")]
    pub backup_frequency: String,
    #[serde(default)]
    pub backup_target_dir: String,
    #[serde(default = "default_backup_max_count")]
    pub backup_max_count: usize,
    #[serde(default)]
    pub backup_last_run_at: i64,
    #[serde(default = "default_backup_last_run_status")]
    pub backup_last_run_status: String,
}

impl Default for AppSettingsData {
    fn default() -> Self {
        Self {
            version: get_default_app_version(),
            max_items: 50,
            text_max_items: default_text_max_items(),
            image_max_items: default_image_max_items(),
            image_disk_limit_mb: default_image_disk_limit_mb(),
            hot_key: DEFAULT_TOGGLE_SHORTCUT.to_string(),
            text_clipboard_enabled: default_text_clipboard_enabled(),
            image_hot_key: default_image_hot_key(),
            image_clipboard_enabled: default_image_clipboard_enabled(),
            screenshot_hot_key: default_screenshot_hot_key(),
            screenshot_enabled: default_screenshot_enabled(),
            recording_hot_key: default_recording_hot_key(),
            recording_mic_toggle_hot_key: default_recording_mic_toggle_hot_key(),
            recording_enabled: default_recording_enabled(),
            recording_default_fps: default_recording_default_fps(),
            recording_default_video_bitrate_kbps: default_recording_default_video_bitrate_kbps(),
            recording_default_audio_bitrate_kbps: default_recording_default_audio_bitrate_kbps(),
            recording_capture_cursor: default_recording_capture_cursor(),
            recording_capture_system_audio: default_recording_capture_system_audio(),
            recording_capture_microphone: default_recording_capture_microphone(),
            recording_microphone_device_id: String::new(),
            recording_output_dir: default_recording_output_dir(),
            recording_auto_open_folder: default_recording_auto_open_folder(),
            recording_toolbar_content_protected: default_recording_toolbar_content_protected(),
            recording_max_duration_minutes: default_recording_max_duration_minutes(),
            recording_file_name_template: default_recording_file_name_template(),
            recording_ffmpeg_download_url: default_recording_ffmpeg_download_url(),
            recording_window_audio_sync_advance_ms: default_recording_window_audio_sync_advance_ms(
            ),
            recording_wgc_force_default_border: default_recording_wgc_force_default_border(),
            recording_wgc_force_default_dirty_region:
                default_recording_wgc_force_default_dirty_region(),
            dev_force_ffmpeg_window_capture: default_dev_force_ffmpeg_window_capture(),
            ai_provider: "deepseek".to_string(),
            provider_configs: HashMap::new(),
            selection_enabled: true,
            selection_modifier_key: default_selection_modifier_key(),
            selection_custom_prompts: default_selection_custom_prompts(),
            selection_web_search_enabled: default_selection_web_search_enabled(),
            selection_web_search_engine: default_selection_web_search_engine(),
            grouped_items_protected_from_limit: default_grouped_items_protected_from_limit(),
            clipboard_bottom_offset: default_clipboard_bottom_offset(),
            translation_prompt_template: default_translation_prompt_template(),
            explanation_prompt_template: default_explanation_prompt_template(),
            image_fill_verify_mode: default_image_fill_verify_mode(),
            ocr_engine: default_ocr_engine(),
            backup_enabled: default_backup_enabled(),
            backup_frequency: default_backup_frequency(),
            backup_target_dir: String::new(),
            backup_max_count: default_backup_max_count(),
            backup_last_run_at: 0,
            backup_last_run_status: default_backup_last_run_status(),
        }
    }
}

fn default_selection_enabled() -> bool {
    false
}

fn default_selection_modifier_key() -> String {
    "".to_string()
}

fn default_selection_custom_prompts() -> Vec<CustomPrompt> {
    Vec::new()
}

fn default_selection_web_search_enabled() -> bool {
    true
}

fn default_selection_web_search_engine() -> String {
    "bing".to_string()
}

fn default_text_clipboard_enabled() -> bool {
    false
}

fn default_image_clipboard_enabled() -> bool {
    false
}

fn default_screenshot_enabled() -> bool {
    false
}

fn default_recording_enabled() -> bool {
    false
}

fn default_text_max_items() -> usize {
    50
}

fn default_image_max_items() -> usize {
    50
}

fn default_image_disk_limit_mb() -> u64 {
    2048
}

fn default_image_hot_key() -> String {
    DEFAULT_IMAGE_TOGGLE_SHORTCUT.to_string()
}

fn default_screenshot_hot_key() -> String {
    DEFAULT_SCREENSHOT_SHORTCUT.to_string()
}

fn default_recording_hot_key() -> String {
    DEFAULT_RECORDING_SHORTCUT.to_string()
}

fn default_recording_mic_toggle_hot_key() -> String {
    "Ctrl+Space".to_string()
}

fn default_recording_default_fps() -> u32 {
    30
}

fn default_recording_default_video_bitrate_kbps() -> u32 {
    6000
}

fn default_recording_default_audio_bitrate_kbps() -> u32 {
    160
}

fn default_recording_capture_cursor() -> bool {
    true
}

fn default_recording_capture_system_audio() -> bool {
    true
}

fn default_recording_capture_microphone() -> bool {
    false
}

fn default_recording_output_dir() -> String {
    String::new()
}

fn default_recording_auto_open_folder() -> bool {
    false
}

fn default_recording_toolbar_content_protected() -> bool {
    false
}

fn default_recording_max_duration_minutes() -> u32 {
    180
}

fn default_recording_file_name_template() -> String {
    "{timestamp}".to_string()
}

fn default_recording_ffmpeg_download_url() -> String {
    "https://gitee.com/zrq1351/fuyun_tools/releases/download/v0.5.6/ffmpeg.exe".to_string()
}

fn default_recording_window_audio_sync_advance_ms() -> u32 {
    80
}

fn default_recording_wgc_force_default_border() -> bool {
    false
}

fn default_recording_wgc_force_default_dirty_region() -> bool {
    false
}

fn default_dev_force_ffmpeg_window_capture() -> bool {
    false
}

fn default_grouped_items_protected_from_limit() -> bool {
    true
}

fn default_clipboard_bottom_offset() -> i32 {
    180
}

fn default_image_fill_verify_mode() -> String {
    "fast".to_string()
}

fn default_ocr_engine() -> String {
    "ocr-rs".to_string()
}

fn default_backup_enabled() -> bool {
    false
}

fn default_backup_frequency() -> String {
    "weekly".to_string()
}

fn default_backup_max_count() -> usize {
    5
}

fn default_backup_last_run_status() -> String {
    "idle".to_string()
}

pub fn default_translation_prompt_template() -> String {
    "你是专业翻译助手。任务：将用户文本翻译为{target_language}。\n要求：\n1) 自动识别源语言（如已提供{source_language}且不是“自动识别”，按其处理）。\n2) 忠实原意，不遗漏、不杜撰。\n3) 保留专有名词、代码、变量、URL、邮箱、数字与单位。\n4) 保持原文段落与换行结构。\n5) 只输出译文，不要任何说明。\n\n待翻译文本：\n{text}".to_string()
}

pub fn default_explanation_prompt_template() -> String {
    "你是清晰易懂的讲解助手。请使用{target_language}解释下列内容。\n要求：\n1) 先给一句话总结，再分点说明关键点。\n2) 面向普通用户，术语给简短释义。\n3) 保持准确，不编造；不确定时直接说明。\n4) 控制在180字以内。\n5) 仅输出解释内容。\n\n待解释文本：\n{text}".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct MigrationVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl MigrationVersion {
    const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

fn parse_migration_version(raw: &str) -> Option<MigrationVersion> {
    let trimmed = raw.trim().trim_start_matches('v');
    if trimmed.is_empty() {
        return None;
    }
    if !trimmed.contains('.') {
        if let Ok(legacy) = trimmed.parse::<u32>() {
            return Some(MigrationVersion::new(0, legacy, 0));
        }
    }
    let core = trimmed
        .split_once('-')
        .map(|(left, _)| left)
        .unwrap_or(trimmed)
        .split_once('+')
        .map(|(left, _)| left)
        .unwrap_or(trimmed);
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u32>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u32>().ok()?;
    Some(MigrationVersion::new(major, minor, patch))
}

impl AppSettingsData {
    pub fn set_provider_api_key(
        &mut self,
        provider_key: &str,
        api_key: &str,
    ) -> Result<(), String> {
        if let Some(config) = self.provider_configs.get_mut(provider_key) {
            config.encrypted_api_key.clear();
        }
        let service_name = "fuyun_tools";
        let user_name = format!("api_key_{}", provider_key);
        if api_key.is_empty() {
            if let Ok(entry) = Entry::new(service_name, &user_name) {
                let _ = entry.delete_credential();
            }
            log::info!("API key cleared for provider: {}", provider_key);
            return Ok(());
        }
        match Entry::new(service_name, &user_name) {
            Ok(entry) => {
                let mut last_error = String::new();
                for i in 0..3 {
                    match entry.set_password(api_key) {
                        Ok(_) => {
                            log::info!(
                                "API key saved for provider: {} (attempt {})",
                                provider_key,
                                i + 1
                            );
                            return Ok(());
                        }
                        Err(e) => {
                            let _ = entry.delete_credential();
                            log::warn!("Failed to save API key (attempt {}): {}", i + 1, e);
                            last_error = e.to_string();
                            thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
                Err(format!("保存API密钥失败(重试3次后): {}", last_error))
            }
            Err(e) => Err(format!("创建密钥入口失败: {}", e)),
        }
    }

    pub fn get_provider_api_key(&self, provider_key: &str) -> Result<String, String> {
        let service_name = "fuyun_tools";
        let user_name = format!("api_key_{}", provider_key);
        let entry =
            Entry::new(service_name, &user_name).map_err(|e| format!("创建密钥入口失败: {}", e))?;
        let mut last_error = String::new();
        for i in 0..3 {
            match entry.get_password() {
                Ok(password) => {
                    log::info!(
                        "Successfully retrieved API key for provider: {} (attempt {})",
                        provider_key,
                        i + 1
                    );
                    return Ok(password);
                }
                Err(keyring::Error::NoEntry) => {
                    log::info!("No API key found in keyring for provider: {}", provider_key);
                    return Ok(String::new());
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    if error_msg.contains("Element not found") || error_msg.contains("找不到元素")
                    {
                        return Ok(String::new());
                    }
                    log::warn!(
                        "Failed to retrieve API key for provider {} (attempt {}): {}",
                        provider_key,
                        i + 1,
                        e
                    );
                    last_error = error_msg;
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
        log::error!(
            "Failed to retrieve API key after retries for provider {}: {}",
            provider_key,
            last_error
        );
        Err(format!("获取API密钥失败: {}", last_error))
    }

    pub fn migrate_legacy_api_keys(&mut self) -> bool {
        let mut migrated = false;
        let provider_keys: Vec<String> = self.provider_configs.keys().cloned().collect();
        for provider_key in provider_keys {
            if let Some(config) = self.provider_configs.get_mut(&provider_key) {
                if !config.encrypted_api_key.is_empty() {
                    log::info!("发现旧版加密密钥，正在迁移提供商: {}", provider_key);
                    use base64::engine::general_purpose::STANDARD;
                    use base64::Engine as _;
                    let decrypted_result = STANDARD
                        .decode(&config.encrypted_api_key)
                        .ok()
                        .and_then(|encrypted| {
                            let decrypted: Vec<u8> = encrypted
                                .iter()
                                .enumerate()
                                .map(|(i, &b)| {
                                    b ^ LEGACY_ENCRYPTION_KEY[i % LEGACY_ENCRYPTION_KEY.len()]
                                })
                                .collect();
                            String::from_utf8(decrypted).ok()
                        });
                    if let Some(api_key) = decrypted_result {
                        if let Ok(entry) =
                            Entry::new("fuyun_tools", &format!("api_key_{}", provider_key))
                        {
                            if let Err(e) = entry.set_password(&api_key) {
                                log::error!("迁移密钥失败: {}", e);
                            } else {
                                log::info!("密钥迁移成功");
                                migrated = true;
                                config.encrypted_api_key.clear();
                            }
                        }
                    }
                }
            }
        }
        migrated
    }

    pub fn save_current_provider_config(&mut self, api_key: &str) -> Result<(), String> {
        let provider_key = self.ai_provider.clone();
        self.set_provider_api_key(&provider_key, api_key)?;
        Ok(())
    }

    pub fn load_provider_config_to_current(
        &mut self,
        provider_name: &str,
    ) -> Result<ProviderConfig, String> {
        let provider_key = provider_name.to_string();
        let config_copy = if let Some(config) = self.provider_configs.get(&provider_key) {
            config.clone()
        } else {
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
                _ => (String::new(), String::new()),
            };
            ProviderConfig {
                api_url: default_url,
                model_name: default_model,
                encrypted_api_key: String::new(),
            }
        };
        let _ = self.get_provider_api_key(&provider_key);
        self.ai_provider = provider_name.to_string();
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

    pub fn get_current_provider_config(&self) -> Option<&ProviderConfig> {
        self.provider_configs.get(&self.ai_provider)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.max_items == 0 || self.max_items > 1000 {
            return Err("max_items必须在1-1000之间".to_string());
        }
        if self.text_max_items == 0 || self.text_max_items > 1000 {
            return Err("text_max_items必须在1-1000之间".to_string());
        }
        if self.image_max_items == 0 || self.image_max_items > 1000 {
            return Err("image_max_items必须在1-1000之间".to_string());
        }

        if self.image_disk_limit_mb < 100 || self.image_disk_limit_mb > 102400 {
            return Err("image_disk_limit_mb必须在100-102400之间".to_string());
        }

        if self.image_fill_verify_mode != "strict" && self.image_fill_verify_mode != "fast" {
            return Err("image_fill_verify_mode必须是strict或fast".to_string());
        }

        if !self.hot_key.is_empty() && !self.hot_key.contains('+') {
            return Err("快捷键格式无效，必须包含修饰键（如Ctrl+Alt+C）".to_string());
        }
        if !self.image_hot_key.is_empty() && !self.image_hot_key.contains('+') {
            return Err("图片快捷键格式无效，必须包含修饰键（如Ctrl+Alt+X）".to_string());
        }
        if !self.screenshot_hot_key.is_empty() && !self.screenshot_hot_key.contains('+') {
            return Err("截图快捷键格式无效，必须包含修饰键（如Ctrl+Alt+A）".to_string());
        }
        if !self.recording_hot_key.is_empty() && !self.recording_hot_key.contains('+') {
            return Err("录屏快捷键格式无效，必须包含修饰键（如Ctrl+Alt+R）".to_string());
        }
        if !self.recording_mic_toggle_hot_key.is_empty()
            && !self.recording_mic_toggle_hot_key.contains('+')
        {
            return Err("麦克风切换快捷键格式无效，必须包含修饰键（如Ctrl+Space）".to_string());
        }

        if self.recording_default_fps == 0 || self.recording_default_fps > 120 {
            return Err("recording_default_fps必须在1-120之间".to_string());
        }
        if self.recording_default_video_bitrate_kbps < 500
            || self.recording_default_video_bitrate_kbps > 50000
        {
            return Err("recording_default_video_bitrate_kbps必须在500-50000之间".to_string());
        }
        if self.recording_default_audio_bitrate_kbps < 32
            || self.recording_default_audio_bitrate_kbps > 512
        {
            return Err("recording_default_audio_bitrate_kbps必须在32-512之间".to_string());
        }
        if self.recording_max_duration_minutes == 0 || self.recording_max_duration_minutes > 1440 {
            return Err("recording_max_duration_minutes必须在1-1440之间".to_string());
        }
        if self.recording_file_name_template.trim().is_empty() {
            return Err("recording_file_name_template不能为空".to_string());
        }
        if self.recording_ffmpeg_download_url.trim().is_empty() {
            return Err("recording_ffmpeg_download_url不能为空".to_string());
        }
        if !self.recording_ffmpeg_download_url.starts_with("https://") {
            return Err("recording_ffmpeg_download_url必须以https://开头".to_string());
        }
        if self.recording_window_audio_sync_advance_ms > 500 {
            return Err("recording_window_audio_sync_advance_ms必须在0-500之间".to_string());
        }

        for (provider_name, config) in &self.provider_configs {
            if !config.api_url.is_empty() && !config.api_url.starts_with("https://") {
                return Err(format!(
                    "提供商 {} 的API URL格式无效，必须以https://开头",
                    provider_name
                ));
            }
        }

        if self.clipboard_bottom_offset < 0 || self.clipboard_bottom_offset > 400 {
            return Err("clipboard_bottom_offset必须在0-400之间".to_string());
        }

        if self.translation_prompt_template.trim().is_empty() {
            return Err("翻译提示模板不能为空".to_string());
        }
        if self.explanation_prompt_template.trim().is_empty() {
            return Err("解释提示模板不能为空".to_string());
        }

        if !self.translation_prompt_template.contains("{text}")
            || !self
                .translation_prompt_template
                .contains("{target_language}")
        {
            return Err("翻译提示模板必须包含{text}和{target_language}占位符".to_string());
        }
        if !self.explanation_prompt_template.contains("{text}")
            || !self
                .explanation_prompt_template
                .contains("{target_language}")
        {
            return Err("解释提示模板必须包含{text}和{target_language}占位符".to_string());
        }

        Ok(())
    }

    pub fn get_masked_api_key(&self) -> String {
        match self.get_provider_api_key(&self.ai_provider) {
            Ok(api_key) => {
                if api_key.is_empty() {
                    return String::new();
                }
                let chars: Vec<char> = api_key.chars().collect();
                let len = chars.len();
                if len <= 16 {
                    return "*".repeat(len.min(30));
                }
                let prefix_len = 8.min(len);
                let suffix_len = 8.min(len.saturating_sub(prefix_len));
                let prefix: String = chars.iter().take(prefix_len).collect();
                let suffix: String = chars
                    .iter()
                    .skip(len.saturating_sub(suffix_len))
                    .take(suffix_len)
                    .collect();
                format!("{}{}{}", prefix, "*".repeat(30), suffix)
            }
            Err(_) => String::new(),
        }
    }

    pub fn migrate_from_old(&mut self) {
        let current_version = get_default_app_version();
        if self.version == current_version {
            log::debug!("当前已是最新版本: {}，无需迁移", self.version);
            return;
        }
        match (
            parse_migration_version(&self.version),
            parse_migration_version(&current_version),
        ) {
            (Some(old_ver), Some(new_ver)) => {
                if old_ver < new_ver {
                    log::debug!("执行版本 {} 到 {} 的迁移", self.version, current_version);
                    self.perform_version_migration(old_ver, new_ver);
                }
            }
            _ => {
                log::debug!("无法解析版本号格式，执行通用迁移");
                self.perform_generic_migration();
            }
        }
        self.version = current_version;
        log::debug!("版本迁移完成，当前版本: {}", self.version);
    }

    fn perform_version_migration(
        &mut self,
        old_version: MigrationVersion,
        new_version: MigrationVersion,
    ) {
        log::info!("执行版本迁移: {:?} -> {:?}", old_version, new_version);
        if old_version < MigrationVersion::new(0, 2, 0)
            && new_version >= MigrationVersion::new(0, 2, 0)
        {
            log::info!("迁移至版本 2: 确保基础配置完整性");
            self.ensure_basic_config_integrity();
        }
        if old_version < MigrationVersion::new(0, 3, 0)
            && new_version >= MigrationVersion::new(0, 3, 0)
        {
            log::info!("迁移至版本 3: 初始化AI提供商配置");
            self.initialize_ai_provider_configs_if_needed();
        }
    }

    fn perform_generic_migration(&mut self) {
        log::info!("执行通用配置迁移");
        self.ensure_basic_config_integrity();
        self.initialize_ai_provider_configs_if_needed();
    }

    fn ensure_basic_config_integrity(&mut self) {
        log::info!("开始确保基础配置完整性");
        log::debug!("迁移前 max_items: {}", self.max_items);
        if self.max_items < 10 || self.max_items > 1000 {
            let old_value = self.max_items;
            self.max_items = 50;
            log::info!("修复 max_items 从 {} 为默认值: 50", old_value);
        }
        if self.text_max_items == default_text_max_items()
            && self.image_max_items == default_image_max_items()
            && self.max_items >= 10
            && self.max_items <= 1000
        {
            self.text_max_items = self.max_items;
            self.image_max_items = self.max_items;
        }
        if self.text_max_items < 10 || self.text_max_items > 1000 {
            self.text_max_items = default_text_max_items();
        }
        if self.image_max_items < 10 || self.image_max_items > 1000 {
            self.image_max_items = default_image_max_items();
        }
        if self.image_disk_limit_mb < 100 || self.image_disk_limit_mb > 102400 {
            self.image_disk_limit_mb = default_image_disk_limit_mb();
        }
        self.max_items = self.text_max_items;
        if self.hot_key.is_empty() {
            self.hot_key = DEFAULT_TOGGLE_SHORTCUT.to_string();
            log::info!("修复 hot_key 为默认值: {}", DEFAULT_TOGGLE_SHORTCUT);
        }
        if !self.text_clipboard_enabled {
            log::info!("文字剪贴板功能保持禁用");
        }
        if self.image_hot_key.is_empty() {
            self.image_hot_key = default_image_hot_key();
        }
        if !self.image_clipboard_enabled {
            log::info!("图片剪贴板功能保持禁用");
        }
        if self.screenshot_hot_key.is_empty() {
            self.screenshot_hot_key = default_screenshot_hot_key();
        }
        if !self.screenshot_enabled {
            log::info!("截图功能保持禁用");
        }
        if self.recording_hot_key.is_empty() {
            self.recording_hot_key = default_recording_hot_key();
        }
        if self.recording_default_fps == 0 || self.recording_default_fps > 120 {
            self.recording_default_fps = default_recording_default_fps();
        }
        if self.recording_default_video_bitrate_kbps < 500
            || self.recording_default_video_bitrate_kbps > 50000
        {
            self.recording_default_video_bitrate_kbps =
                default_recording_default_video_bitrate_kbps();
        }
        if self.recording_default_audio_bitrate_kbps < 32
            || self.recording_default_audio_bitrate_kbps > 512
        {
            self.recording_default_audio_bitrate_kbps =
                default_recording_default_audio_bitrate_kbps();
        }
        if self.recording_max_duration_minutes == 0 || self.recording_max_duration_minutes > 1440 {
            self.recording_max_duration_minutes = default_recording_max_duration_minutes();
        }
        if self.recording_file_name_template.trim().is_empty() {
            self.recording_file_name_template = default_recording_file_name_template();
        }
        if self.recording_ffmpeg_download_url.trim().is_empty() {
            self.recording_ffmpeg_download_url = default_recording_ffmpeg_download_url();
        }
        if self.recording_window_audio_sync_advance_ms > 500 {
            self.recording_window_audio_sync_advance_ms =
                default_recording_window_audio_sync_advance_ms();
        }
        if self.clipboard_bottom_offset < 0 || self.clipboard_bottom_offset > 400 {
            self.clipboard_bottom_offset = default_clipboard_bottom_offset();
        }
        if self.translation_prompt_template.trim().is_empty() {
            self.translation_prompt_template = default_translation_prompt_template();
        }
        if self.explanation_prompt_template.trim().is_empty() {
            self.explanation_prompt_template = default_explanation_prompt_template();
        }
        if self.image_fill_verify_mode != "strict" && self.image_fill_verify_mode != "fast" {
            self.image_fill_verify_mode = default_image_fill_verify_mode();
        }
        log::debug!(
            "迁移后 max_items: {}, text_max_items: {}, image_max_items: {}, image_disk_limit_mb: {}",
            self.max_items,
            self.text_max_items,
            self.image_max_items,
            self.image_disk_limit_mb
        );
    }

    fn initialize_ai_provider_configs_if_needed(&mut self) {
        if self.provider_configs.is_empty() {
            initialize_builtin_providers(self);
            log::info!("初始化内置AI提供商配置");
        }
        if !self.provider_configs.contains_key(&self.ai_provider) {
            let (default_url, default_model) = self.get_provider_default_config(&self.ai_provider);
            let config = ProviderConfig {
                api_url: default_url,
                model_name: default_model,
                encrypted_api_key: String::new(),
            };
            self.provider_configs
                .insert(self.ai_provider.clone(), config);
            log::info!("为提供商 {} 创建默认配置", self.ai_provider);
        }
    }

    fn get_provider_default_config(&self, provider_name: &str) -> (String, String) {
        match provider_name {
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
            _ => (String::new(), String::new()),
        }
    }
}

pub fn initialize_builtin_providers(settings: &mut AppSettingsData) {
    use crate::core::config::{AIProvider, ProviderConfig};
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
