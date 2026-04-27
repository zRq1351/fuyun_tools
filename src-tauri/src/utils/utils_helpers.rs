pub use crate::utils::database::{
    get_history_db_path, load_history_data, load_history_data_async, load_history_page_data_async,
    ClipboardHistoryData, ClipboardHistoryPageData, ClipboardHistoryPageItem,
};
pub use crate::utils::settings_model::{
    default_explanation_prompt_template, default_translation_prompt_template,
    initialize_builtin_providers, AppSettingsData,
};
pub use crate::utils::system_utils::{
    atomic_write_with_backup, get_default_app_version, get_logs_dir_path, get_settings_file_path,
    load_settings, read_text_with_backup, save_settings,
};
pub use crate::utils::text_utils::{
    calculate_text_similarity, compare_versions, detect_text_completeness,
    find_best_replacement_candidate, get_dedup_scan_metrics, DedupScanMetrics, TextCompleteness,
    VersionComparison,
};

/// 统一的当前时间戳工具函数（毫秒，i64）
/// 替代各文件中重复定义的 now_unix_ms()
pub fn now_unix_ms_i64() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 统一的当前时间戳工具函数（毫秒，u64）
/// 替代各文件中重复定义的 now_unix_ms_u64()
pub fn now_unix_ms_u64() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
