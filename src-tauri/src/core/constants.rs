//! 应用程序常量定义
//! 集中管理所有魔法数字和配置常量

// ============================================================================
// 数据库相关常量
// ============================================================================

/// 数据库重试次数
pub const DB_RETRY_COUNT: u32 = 3;

/// 数据库重试间隔（毫秒）
pub const DB_RETRY_DELAY_MS: u64 = 50;

/// 数据库繁忙超时（毫秒）
pub const DB_BUSY_TIMEOUT_MS: u64 = 1200;

/// 批量插入大小
pub const DB_BATCH_SIZE: usize = 100;

// ============================================================================
// 剪贴板相关常量
// ============================================================================

/// 长文本去重阈值（字符数）
pub const LONG_TEXT_DEDUP_THRESHOLD: usize = 4000;

/// 长文本去重扫描限制
pub const LONG_TEXT_DEDUP_SCAN_LIMIT: usize = 24;

/// 精确索引缓存容量
pub const EXACT_INDEX_CACHE_CAPACITY: usize = 2048;

/// 布隆过滤器容量
pub const BLOOM_FILTER_CAPACITY: u32 = 10000;

/// 布隆过滤器误判率
pub const BLOOM_FILTER_ERROR_RATE: f32 = 0.01;

/// 相似度阈值
pub const SIMILARITY_THRESHOLD: f64 = 0.8;

/// 防抖延迟（毫秒）
pub const DEBOUNCE_DELAY_MS: u64 = 180;

/// 剪贴板读取重试延迟（毫秒）
pub const CLIPBOARD_READ_RETRY_DELAYS: [u64; 2] = [10, 20];

/// 剪贴板写入重试延迟（毫秒）
pub const CLIPBOARD_WRITE_RETRY_DELAYS: [u64; 9] = [3, 6, 10, 16, 24, 36, 52, 72, 95];

/// 剪贴板写入验证延迟（毫秒）
pub const CLIPBOARD_VERIFY_DELAYS: [u64; 3] = [5, 10, 18];

// ============================================================================
// 图片相关常量
// ============================================================================

/// UI 最大历史记录项数
pub const MAX_UI_HISTORY_ITEMS: usize = 30;

/// 全分辨率图片内存预算（字节）
pub const IMAGE_FULL_RES_MEMORY_BUDGET_BYTES: usize = 256 * 1024 * 1024;

/// 全分辨率缓存保留最近项数
pub const IMAGE_FULL_RES_CACHE_KEEP_RECENT: usize = 6;

/// 全分辨率 LRU 最大容量
pub const IMAGE_FULL_RES_LRU_MAX_CAPACITY: usize = 4096;

/// 图片持久化队列大小
pub const IMAGE_PERSIST_QUEUE_SIZE: usize = 6;

/// 图片预览最大边长（像素）
pub const IMAGE_PREVIEW_MAX_EDGE: u32 = 320;

/// PNG Base64 缓存容量
pub const IMAGE_PNG_BASE64_CACHE_CAPACITY: usize = 64;

/// 图片磁盘限制最小值（MB）
pub const IMAGE_DISK_LIMIT_MIN_MB: u64 = 100;

/// 图片磁盘限制最大值（MB）
pub const IMAGE_DISK_LIMIT_MAX_MB: u64 = 102400;

/// 默认图片磁盘限制（MB）
pub const DEFAULT_IMAGE_DISK_LIMIT_MB: u64 = 2048;

// ============================================================================
// 设置相关常量
// ============================================================================

/// 最大记录数最小值
pub const MAX_ITEMS_MIN: usize = 1;

/// 最大记录数最大值
pub const MAX_ITEMS_MAX: usize = 1000;

/// 默认最大记录数
pub const DEFAULT_MAX_ITEMS: usize = 50;

/// 剪贴板底部偏移量最小值
pub const CLIPBOARD_BOTTOM_OFFSET_MIN: i32 = 0;

/// 剪贴板底部偏移量最大值
pub const CLIPBOARD_BOTTOM_OFFSET_MAX: i32 = 400;

/// 默认剪贴板底部偏移量
pub const DEFAULT_CLIPBOARD_BOTTOM_OFFSET: i32 = 8;

// ============================================================================
// 网络相关常量
// ============================================================================

/// API 密钥保存重试次数
pub const API_KEY_SAVE_RETRY_COUNT: u32 = 3;

/// API 密钥保存重试间隔（毫秒）
pub const API_KEY_SAVE_RETRY_DELAY_MS: u64 = 100;

// ============================================================================
// 日志级别常量
// ============================================================================

/// 调试日志级别
pub const LOG_LEVEL_DEBUG: &str = "debug";

/// 信息日志级别
pub const LOG_LEVEL_INFO: &str = "info";

/// 警告日志级别
pub const LOG_LEVEL_WARN: &str = "warn";

/// 错误日志级别
pub const LOG_LEVEL_ERROR: &str = "error";

// ============================================================================
// 版本迁移相关常量
// ============================================================================

/// 版本 2.0 迁移阈值
pub const MIGRATION_VERSION_2_0: (u32, u32, u32) = (0, 2, 0);

/// 版本 3.0 迁移阈值
pub const MIGRATION_VERSION_3_0: (u32, u32, u32) = (0, 3, 0);

// ============================================================================
// 文件路径相关常量
// ============================================================================

/// 设置文件名
pub const SETTINGS_FILE_NAME: &str = "settings.json";

/// 备份文件扩展名
pub const BACKUP_FILE_EXTENSION: &str = ".bak";

/// 临时文件扩展名
pub const TEMP_FILE_EXTENSION: &str = ".tmp";

/// 历史数据库文件名
pub const HISTORY_DB_FILE_NAME: &str = "history.db";

/// 图片历史数据库文件名
pub const IMAGE_HISTORY_DB_FILE_NAME: &str = "image_history.db";

/// 图片存储目录名
pub const IMAGE_BLOBS_DIR_NAME: &str = "image_history_blobs";

/// 日志目录名
pub const LOGS_DIR_NAME: &str = "logs";

// ============================================================================
// 快捷键相关常量
// ============================================================================

/// 默认剪贴板切换快捷键
pub const DEFAULT_TOGGLE_SHORTCUT: &str = "Ctrl+Alt+C";

/// 默认图片剪贴板切换快捷键
pub const DEFAULT_IMAGE_TOGGLE_SHORTCUT: &str = "Ctrl+Alt+V";

// ============================================================================
// AI 相关常量
// ============================================================================

/// 默认 AI 提供商
pub const DEFAULT_AI_PROVIDER: &str = "deepseek";

/// 默认翻译目标语言
pub const DEFAULT_TRANSLATION_TARGET_LANGUAGE: &str = "简体中文";

/// 默认解释目标语言
pub const DEFAULT_EXPLANATION_TARGET_LANGUAGE: &str = "中文";