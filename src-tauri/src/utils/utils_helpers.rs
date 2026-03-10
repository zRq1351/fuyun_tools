use crate::core::config::{ProviderConfig, DEFAULT_IMAGE_TOGGLE_SHORTCUT, DEFAULT_TOGGLE_SHORTCUT};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

const LEGACY_ENCRYPTION_KEY: &[u8] = b"fuyun_tools_encryption_key_2025!"; // 32字节旧版密钥，仅用于迁移

/// 获取应用默认版本号
pub fn get_default_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppSettingsData {
    pub version: String,
    pub max_items: usize,
    pub hot_key: String,
    #[serde(default = "default_image_hot_key")]
    pub image_hot_key: String,
    #[serde(default)]
    pub ai_provider: String,  // 改为String类型以支持自定义提供商
    /// 每个AI提供商的独立配置
    #[serde(default)]
    pub provider_configs: HashMap<String, ProviderConfig>,
    #[serde(default = "default_selection_enabled")]
    pub selection_enabled: bool,
    #[serde(default = "default_clipboard_bottom_offset")]
    pub clipboard_bottom_offset: i32,
}

impl Default for AppSettingsData {
    fn default() -> Self {
        Self {
            version: get_default_app_version(),
            max_items: 50,
            hot_key: DEFAULT_TOGGLE_SHORTCUT.to_string(),
            image_hot_key: default_image_hot_key(),
            ai_provider: "deepseek".to_string(),
            provider_configs: HashMap::new(),
            selection_enabled: true,
            clipboard_bottom_offset: default_clipboard_bottom_offset(),
        }
    }
}

fn default_selection_enabled() -> bool {
    true
}

fn default_image_hot_key() -> String {
    DEFAULT_IMAGE_TOGGLE_SHORTCUT.to_string()
}

fn default_clipboard_bottom_offset() -> i32 {
    8
}

impl AppSettingsData {
    /// 为指定提供商设置API密钥（存储到系统凭据管理器）
    pub fn set_provider_api_key(&mut self, provider_key: &str, api_key: &str) -> Result<(), String> {
        // 更新内存中的配置对象（清空旧版加密字段）
        if let Some(config) = self.provider_configs.get_mut(provider_key) {
            config.encrypted_api_key.clear();
        }

        // Windows 平台特殊处理：使用 target 而不是 service 作为 Entry 的第一个参数
        // keyring 在 Windows 上会将 service 和 username 组合起来，有时会导致 lookup 失败
        // 我们尝试简化 Entry 的创建

        let service_name = "fuyun_tools";
        let user_name = format!("api_key_{}", provider_key);

        if api_key.is_empty() {
            if let Ok(entry) = Entry::new(service_name, &user_name) {
                let _ = entry.delete_credential();
            }
            log::info!("API key cleared for provider: {}", provider_key);
            return Ok(());
        }

        // 尝试创建并写入
        match Entry::new(service_name, &user_name) {
            Ok(entry) => {
                // 添加重试机制
                let mut last_error = String::new();
                for i in 0..3 {
                    match entry.set_password(api_key) {
                        Ok(_) => {
                            log::info!("API key saved for provider: {} (attempt {})", provider_key, i + 1);
                            return Ok(());
                        },
                        Err(e) => {
                            // 如果写入失败，尝试先删除再写入
                            let _ = entry.delete_credential();
                            log::warn!("Failed to save API key (attempt {}): {}", i + 1, e);
                            last_error = e.to_string();
                            thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
                Err(format!("保存API密钥失败(重试3次后): {}", last_error))
            },
            Err(e) => Err(format!("创建密钥入口失败: {}", e))
        }
    }

    /// 获取指定提供商的API密钥（从系统凭据管理器）
    pub fn get_provider_api_key(&self, provider_key: &str) -> Result<String, String> {
        let service_name = "fuyun_tools";
        let user_name = format!("api_key_{}", provider_key);

        let entry = Entry::new(service_name, &user_name)
            .map_err(|e| format!("创建密钥入口失败: {}", e))?;

        // 增加重试机制
        let mut last_error = String::new();
        for i in 0..3 {
            match entry.get_password() {
                Ok(password) => {
                    log::info!("Successfully retrieved API key for provider: {} (attempt {})", provider_key, i + 1);
                    return Ok(password);
                },
                Err(keyring::Error::NoEntry) => {
                    log::info!("No API key found in keyring for provider: {}", provider_key);
                    return Ok(String::new());
                },
                Err(e) => {
                    let error_msg = e.to_string();
                    if error_msg.contains("Element not found") || error_msg.contains("找不到元素") {
                        return Ok(String::new());
                    }

                    log::warn!("Failed to retrieve API key for provider {} (attempt {}): {}", provider_key, i + 1, e);
                    last_error = error_msg;
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }

        log::error!("Failed to retrieve API key after retries for provider {}: {}", provider_key, last_error);
        Err(format!("获取API密钥失败: {}", last_error))
    }

    /// 迁移旧版加密的API密钥到系统凭据管理器
    /// 返回是否发生了迁移
    pub fn migrate_legacy_api_keys(&mut self) -> bool {
        let mut migrated = false;
        let provider_keys: Vec<String> = self.provider_configs.keys().cloned().collect();

        for provider_key in provider_keys {
            if let Some(config) = self.provider_configs.get_mut(&provider_key) {
                if !config.encrypted_api_key.is_empty() {
                    log::info!("发现旧版加密密钥，正在迁移提供商: {}", provider_key);

                    // 解密旧版密钥
                    use base64::engine::general_purpose::STANDARD;
                    use base64::Engine as _;

                    let decrypted_result = STANDARD.decode(&config.encrypted_api_key)
                        .ok()
                        .and_then(|encrypted| {
                            let decrypted: Vec<u8> = encrypted
                                .iter()
                                .enumerate()
                                .map(|(i, &b)| b ^ LEGACY_ENCRYPTION_KEY[i % LEGACY_ENCRYPTION_KEY.len()])
                                .collect();
                            String::from_utf8(decrypted).ok()
                        });

                    if let Some(api_key) = decrypted_result {
                        // 保存到系统凭据管理器
                        if let Ok(entry) = Entry::new("fuyun_tools", &format!("api_key_{}", provider_key)) {
                            if let Err(e) = entry.set_password(&api_key) {
                                log::error!("迁移密钥失败: {}", e);
                            } else {
                                log::info!("密钥迁移成功");
                                migrated = true;
                            }
                        }
                    }

                    // 清除旧版字段
                    config.encrypted_api_key.clear();
                    // 即使迁移失败（如keyring不可用），我们也清除了旧字段，避免死循环或重复尝试
                    // 但如果keyring失败，用户就丢失了key。
                    // 鉴于这是桌面应用，keyring应该可用。
                    // 如果我们不清除，下次还会尝试。
                    // 如果我们清除但没保存成功，用户需要重新输入。
                }
            }
        }
        migrated
    }

    /// 保存当前提供商的配置
    pub fn save_current_provider_config(&mut self, api_key: &str) -> Result<(), String> {
        let provider_key = self.ai_provider.clone();  // 克隆避免借用冲突

        // 设置该提供商的API密钥
        self.set_provider_api_key(&provider_key, api_key)?;

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
        let _ = self.get_provider_api_key(&provider_key);

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
        match self.get_provider_api_key(&self.ai_provider) {
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
        let current_version = get_default_app_version();

        if self.version == current_version {
            log::debug!("当前已是最新版本: {}，无需迁移", self.version);
            return;
        }

        match (self.version.parse::<u32>(), current_version.parse::<u32>()) {
            (Ok(old_ver), Ok(new_ver)) => {
                if old_ver < new_ver {
                    log::debug!("执行版本 {} 到 {} 的迁移", old_ver, new_ver);
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

    /// 执行具体的版本迁移逻辑
    fn perform_version_migration(&mut self, old_version: u32, new_version: u32) {
        log::info!("执行版本迁移: {} -> {}", old_version, new_version);
        // 根据不同版本间的差异执行特定迁移
        if old_version < 3 && new_version >= 3 {
            log::info!("迁移至版本 3: 初始化AI提供商配置");
            self.initialize_ai_provider_configs_if_needed();
        }

        if old_version < 2 && new_version >= 2 {
            log::info!("迁移至版本 2: 确保基础配置完整性");
            self.ensure_basic_config_integrity();
        }
    }

    /// 执行通用迁移（当版本号无法解析时）
    fn perform_generic_migration(&mut self) {
        log::info!("执行通用配置迁移");

        // 确保基础配置完整性
        self.ensure_basic_config_integrity();

        // 初始化AI提供商配置
        self.initialize_ai_provider_configs_if_needed();
    }

    /// 确保基础配置完整性
    fn ensure_basic_config_integrity(&mut self) {
        log::info!("开始确保基础配置完整性");
        log::debug!("迁移前 max_items: {}", self.max_items);

        // 确保必要字段有合理默认值
        if self.max_items < 10 || self.max_items > 1000 {
            let old_value = self.max_items;
            self.max_items = 50;
            log::info!("修复 max_items 从 {} 为默认值: 50", old_value);
        }

        if self.hot_key.is_empty() {
            self.hot_key = DEFAULT_TOGGLE_SHORTCUT.to_string();
            log::info!("修复 hot_key 为默认值: {}", DEFAULT_TOGGLE_SHORTCUT);
        }

        if self.image_hot_key.is_empty() {
            self.image_hot_key = default_image_hot_key();
        }

        if self.clipboard_bottom_offset < 0 || self.clipboard_bottom_offset > 400 {
            self.clipboard_bottom_offset = default_clipboard_bottom_offset();
        }

        log::debug!("迁移后 max_items: {}", self.max_items);
    }

    /// 初始化AI提供商配置（如果需要）
    fn initialize_ai_provider_configs_if_needed(&mut self) {
        // 如果提供商配置为空，初始化默认配置
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

            self.provider_configs.insert(self.ai_provider.clone(), config);
            log::info!("为提供商 {} 创建默认配置", self.ai_provider);
        }
    }

    /// 获取提供商的默认配置
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
            _ => {
                // 自定义提供商使用空默认值
                (String::new(), String::new())
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ClipboardHistoryData {
    pub items: Vec<String>,
    #[serde(default)]
    pub categories: HashMap<String, String>, // item -> category_name
    #[serde(default)]
    pub category_list: Vec<String>, // ordered list of categories
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
        log::info!("首次运行，创建默认设置文件");
        let mut default_settings = AppSettingsData::default();

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

    let keys_migrated = settings.migrate_legacy_api_keys();
    let old_version = settings.version.clone();
    settings.migrate_from_old();

    if old_version != settings.version || keys_migrated {
        log::info!("配置已更新，保存到文件");
        save_settings(&settings)?;
    }

    let _provider_key = settings.ai_provider.to_string();

    Ok(settings)
}

/// 保存剪切板历史记录到文件
pub fn save_history(history: &[String]) -> Result<(), String> {
    let history_path = get_history_file_path();

    let history_data = ClipboardHistoryData {
        items: history.to_vec(),
        categories: HashMap::new(),
        category_list: Vec::new(),
    };

    let json = serde_json::to_string_pretty(&history_data)
        .map_err(|e| format!("序列化历史记录失败: {}", e))?;

    std::fs::write(&history_path, json).map_err(|e| format!("写入历史记录文件失败: {}", e))?;

    Ok(())
}

/// 保存历史记录到文件（带重试）
pub fn save_history_with_retry(history: &Vec<String>, max_retries: u32) -> Result<(), String> {
    save_history_data_with_retry(
        &ClipboardHistoryData {
            items: history.clone(),
            categories: HashMap::new(),
            category_list: Vec::new(),
        },
        max_retries,
    )
}

/// 保存完整的历史数据（包含分类）到文件（带重试）
pub fn save_history_data_with_retry(
    data: &ClipboardHistoryData,
    max_retries: u32,
) -> Result<(), String> {
    let history_path = get_history_file_path();
    let json = serde_json::to_string_pretty(data).map_err(|e| format!("序列化历史记录失败: {}", e))?;

    for i in 0..max_retries {
        match std::fs::write(&history_path, &json) {
            Ok(_) => return Ok(()),
            Err(e) => {
                if i == max_retries - 1 {
                    return Err(format!("写入历史记录文件失败: {}", e));
                }
                log::warn!("写入历史记录失败 (重试 {}/{}): {}", i + 1, max_retries, e);
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
    Ok(())
}

/// 从文件加载历史记录
pub fn load_history() -> Result<Vec<String>, String> {
    load_history_data().map(|data| data.items)
}

/// 从文件加载完整的历史数据（包含分类）
pub fn load_history_data() -> Result<ClipboardHistoryData, String> {
    let history_path = get_history_file_path();

    if !history_path.exists() {
        return Ok(ClipboardHistoryData::default());
    }

    let contents = std::fs::read_to_string(&history_path)
        .map_err(|e| format!("读取历史记录文件失败: {}", e))?;

    // 尝试解析为新结构
    if let Ok(mut data) = serde_json::from_str::<ClipboardHistoryData>(&contents) {
        // 确保 category_list 不为空，如果 categories 有数据但 category_list 为空，则从 categories 恢复
        if data.category_list.is_empty() && !data.categories.is_empty() {
            let mut unique_categories: Vec<String> = data.categories.values().cloned().collect();
            unique_categories.sort();
            unique_categories.dedup();
            // 过滤掉 "未分类" 和 "全部"，因为它们由前端自动添加
            data.category_list = unique_categories
                .into_iter()
                .filter(|c| c != "未分类" && c != "全部")
                .collect();
        }
        Ok(data)
    } else {
        // 尝试解析为旧的 Vec<String> 格式
        match serde_json::from_str::<Vec<String>>(&contents) {
            Ok(items) => Ok(ClipboardHistoryData {
                items,
                categories: HashMap::new(),
                category_list: Vec::new(),
            }),
            Err(_) => {
                // 如果既不是新结构也不是旧结构，可能是文件损坏，或者是一个空的 JSON 对象
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&contents) {
                    if let Some(obj) = json_val.as_object() {
                        // 尝试手动提取字段，兼容部分字段缺失的情况
                        let items = obj.get("items")
                            .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
                            .unwrap_or_default();

                        let categories = obj.get("categories")
                            .and_then(|v| serde_json::from_value::<HashMap<String, String>>(v.clone()).ok())
                            .unwrap_or_default();

                        let mut category_list = obj.get("category_list")
                            .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
                            .unwrap_or_default();

                        // 如果列表为空但有分类数据，尝试恢复
                        if category_list.is_empty() && !categories.is_empty() {
                            let mut unique: Vec<String> = categories.values().cloned().collect();
                            unique.sort();
                            unique.dedup();
                            category_list = unique.into_iter().filter(|c| c != "未分类" && c != "全部").collect();
                        }

                        return Ok(ClipboardHistoryData {
                            items,
                            categories,
                            category_list,
                        });
                    }
                }

                Err(format!("无法解析历史记录文件"))
            },
        }
    }
}

/// 获取日志目录路径
pub fn get_logs_dir_path() -> PathBuf {
    let mut logs_dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    logs_dir.pop();
    logs_dir.push("logs");
    logs_dir
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

/// 文本完整性检测结果
#[derive(Debug, Clone, PartialEq)]
pub enum TextCompleteness {
    /// 完整文本
    Complete,
    /// 缺失前段
    MissingPrefix,
    /// 缺失后段
    MissingSuffix,
    /// 缺失前后段
    MissingBoth,
    /// 无法确定
    Unknown,
}

/// 版本对比结果
#[derive(Debug, Clone)]
pub struct VersionComparison {
    /// 相似度分数 (0.0 - 1.0)
    pub similarity_score: f64,
    /// 新版本的完整性状态
    pub new_completeness: TextCompleteness,
    /// 是否应该替换旧版本
    pub should_replace: bool,
    /// 替换建议原因
    pub reason: String,
}

/// 计算两个文本的相似度
/// 使用最长公共子序列(LCS)算法计算相似度
pub fn calculate_text_similarity(text1: &str, text2: &str) -> f64 {
    if text1.is_empty() && text2.is_empty() {
        return 1.0;
    }

    if text1.is_empty() || text2.is_empty() {
        return 0.0;
    }

    let chars1: Vec<char> = text1.chars().collect();
    let chars2: Vec<char> = text2.chars().collect();
    let len1 = chars1.len();
    let len2 = chars2.len();

    log::debug!("计算相似度: '{}' vs '{}'", text1, text2);
    log::debug!("长度: {} vs {}", len1, len2);

    // 创建DP表
    let mut dp = vec![vec![0; len2 + 1]; len1 + 1];

    // 填充DP表
    for i in 1..=len1 {
        for j in 1..=len2 {
            if chars1[i - 1] == chars2[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // 计算相似度
    let lcs_length = dp[len1][len2];
    let max_len = len1.max(len2);

    let similarity = if max_len == 0 {
        0.0
    } else {
        lcs_length as f64 / max_len as f64
    };

    log::debug!("LCS长度: {}, 最大长度: {}, 相似度: {:.4}", 
                lcs_length, max_len, similarity);

    similarity
}

/// 检测文本完整性
/// 分析文本是否可能是截断版本
pub fn detect_text_completeness(text: &str, reference_text: &str) -> TextCompleteness {
    if text.is_empty() || reference_text.is_empty() {
        return TextCompleteness::Unknown;
    }

    // 如果文本完全相同，认为是完整版本
    if text == reference_text {
        return TextCompleteness::Complete;
    }

    // 如果新文本比参考文本长，认为是完整版本
    if text.len() > reference_text.len() {
        return TextCompleteness::Complete;
    }

    // 检查是否是前缀
    if reference_text.starts_with(text) {
        return TextCompleteness::MissingSuffix;
    }

    // 检查是否是后缀
    if reference_text.ends_with(text) {
        return TextCompleteness::MissingPrefix;
    }

    // 检查是否包含在中间
    if reference_text.contains(text) && text.len() < reference_text.len() {
        return TextCompleteness::MissingBoth;
    }

    // 检查相似度，如果很高但不是上述情况，可能是部分内容缺失
    let similarity = calculate_text_similarity(text, reference_text);
    if similarity > 0.8 {
        // 通过字符位置分析判断缺失类型
        let text_chars: Vec<char> = text.chars().collect();
        let ref_chars: Vec<char> = reference_text.chars().collect();

        // 检查开头是否匹配
        let mut prefix_match = true;
        let min_len = text_chars.len().min(10); // 检查前10个字符
        for i in 0..min_len {
            if i >= ref_chars.len() || text_chars[i] != ref_chars[i] {
                prefix_match = false;
                break;
            }
        }

        // 检查结尾是否匹配
        let mut suffix_match = true;
        let min_len = text_chars.len().min(10); // 检查后10个字符
        for i in 0..min_len {
            let text_idx = text_chars.len() - 1 - i;
            let ref_idx = ref_chars.len() - 1 - i;
            if text_idx >= text_chars.len() || ref_idx >= ref_chars.len() ||
                text_chars[text_idx] != ref_chars[ref_idx] {
                suffix_match = false;
                break;
            }
        }

        match (prefix_match, suffix_match) {
            (true, false) => TextCompleteness::MissingSuffix,
            (false, true) => TextCompleteness::MissingPrefix,
            (false, false) => TextCompleteness::MissingBoth,
            (true, true) => TextCompleteness::Complete, // 可能是完全相同的短文本
        }
    } else {
        TextCompleteness::Unknown
    }
}

/// 统计文本中标点符号数量
fn count_punctuation(text: &str) -> usize {
    let punctuation_chars = ['。', '！', '？', '.', '!', '?', '；', ';', '，', ','];
    text.chars().filter(|&c| punctuation_chars.contains(&c)).count()
}

/// 判断文本是否具有更完整的句子结构
fn is_more_complete_sentence(new_text: &str, old_text: &str) -> bool {
    // 检查新文本是否有句子结束标志而旧文本没有
    let new_ends_with_period = has_sentence_endings(new_text);
    let old_ends_with_period = has_sentence_endings(old_text);

    new_ends_with_period && !old_ends_with_period
}

/// 判断文本是否以句子结束符结尾
fn has_sentence_endings(text: &str) -> bool {
    let ending_chars = ['。', '！', '？', '.', '!', '?'];
    text.trim_end().chars().last().map_or(false, |c| ending_chars.contains(&c))
}

/// 判断文本是否像是被截断的句子
fn is_truncated_sentence(text: &str) -> bool {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return false;
    }

    // 如果文本以某些字符结尾，可能是被截断的
    let last_char = trimmed.chars().last().unwrap();
    let truncation_indicators = ['，', ',', '、', '(', '[', '{', '"', '\''];

    truncation_indicators.contains(&last_char) ||
        // 或者以常见词汇结尾但没有句子结束符
        (!has_sentence_endings(trimmed) &&
            (trimmed.ends_with("但非") ||
                trimmed.ends_with("但是") ||
                trimmed.ends_with("而且") ||
                trimmed.ends_with("并且")))
}

/// 判断new_text是否是old_text的子集（前缀或后缀）
fn is_subset_of(new_text: &str, old_text: &str) -> bool {
    if new_text.is_empty() || old_text.is_empty() {
        return false;
    }

    // 检查是否是前缀
    if old_text.starts_with(new_text) {
        return true;
    }

    // 检查是否是后缀
    if old_text.ends_with(new_text) {
        return true;
    }

    // 检查是否包含在中间
    if old_text.contains(new_text) && new_text.len() < old_text.len() {
        return true;
    }

    false
}

/// 比较两个版本并决定是否应该替换
pub fn compare_versions(old_text: &str, new_text: &str, similarity_threshold: f64) -> VersionComparison {
    let similarity = calculate_text_similarity(old_text, new_text);
    let completeness = detect_text_completeness(new_text, old_text);

    log::debug!("版本对比 - 旧:'{}' 新:'{}'", old_text, new_text);
    log::debug!("相似度: {:.4}, 完整性: {:?}", similarity, completeness);

    let (should_replace, reason) = if similarity >= similarity_threshold {
        match completeness {
            TextCompleteness::Complete => {
                // 改进的完整版本判断逻辑
                if new_text.len() > old_text.len() {
                    (true, "新版本更完整，长度更长".to_string())
                } else if new_text.len() == old_text.len() {
                    // 即使长度相同，如果新版本包含更多标点符号或完整句子结构，也应该替换
                    let new_has_more_punctuation = count_punctuation(new_text) > count_punctuation(old_text);
                    let new_is_more_complete = is_more_complete_sentence(new_text, old_text);

                    if new_has_more_punctuation || new_is_more_complete {
                        (true, "新版本句子结构更完整".to_string())
                    } else {
                        (false, "版本相同，无需替换".to_string())
                    }
                } else {
                    // 新版本更短的情况 - 检查是否是已有完整版本的子集
                    if is_subset_of(new_text, old_text) {
                        (true, "新版本是已有完整版本的子集，移动完整版本到前面".to_string())
                    } else {
                        // 即使新版本稍短，但如果它更完整（如句子结束符），也可以考虑替换
                        let old_is_truncated = is_truncated_sentence(old_text);
                        let new_is_complete = has_sentence_endings(new_text);

                        if old_is_truncated && new_is_complete {
                            (true, "替换不完整的截断版本".to_string())
                        } else {
                            (false, "新版本较短，保持原版本".to_string())
                        }
                    }
                }
            },
            TextCompleteness::MissingPrefix | TextCompleteness::MissingSuffix | TextCompleteness::MissingBoth => {
                // 对于不完整版本，检查是否存在对应的完整版本
                if new_text.len() < old_text.len() && is_subset_of(new_text, old_text) {
                    // 新版本是旧版本的子集，说明是找回完整版本的情况
                    (true, "找回完整版本，将完整版本移动到前面".to_string())
                } else if new_text.len() > old_text.len() && has_sentence_endings(new_text) {
                    // 新版本更长且有句子结束符
                    (true, "新版本虽被标记为不完整但实际更完整".to_string())
                } else {
                    (false, "新版本内容不完整，保持原版本".to_string())
                }
            },
            TextCompleteness::Unknown => {
                // 当无法确定时，基于长度和句子完整性做保守判断
                if new_text.len() > old_text.len() && has_sentence_endings(new_text) && !has_sentence_endings(old_text) {
                    (true, "基于长度和句子完整性判断，新版本更完整".to_string())
                } else {
                    (false, "无法确定版本关系，保持原版本".to_string())
                }
            }
        }
    } else {
        (false, "文本相似度低于阈值，视为不同内容".to_string())
    };

    log::debug!("替换决策: {}, 原因: {}", should_replace, reason);

    VersionComparison {
        similarity_score: similarity,
        new_completeness: completeness,
        should_replace,
        reason,
    }
}

/// 在历史记录中查找相似条目并返回最佳替换候选
pub fn find_best_replacement_candidate(
    new_text: &str,
    history: &[String],
    similarity_threshold: f64,
) -> Option<(usize, VersionComparison)> {
    let mut best_candidate: Option<(usize, VersionComparison)> = None;

    for (index, old_text) in history.iter().enumerate() {
        let comparison = compare_versions(old_text, new_text, similarity_threshold);

        if comparison.should_replace {
            match &best_candidate {
                None => {
                    best_candidate = Some((index, comparison));
                },
                Some((_, existing_comparison)) => {
                    // 选择相似度更高或更完整的版本
                    if comparison.similarity_score > existing_comparison.similarity_score ||
                        (comparison.similarity_score == existing_comparison.similarity_score &&
                            (matches!(comparison.new_completeness, TextCompleteness::Complete) ||
                                comparison.reason.contains("更完整"))) {
                        best_candidate = Some((index, comparison));
                    }
                }
            }
        }
    }

    best_candidate
}
