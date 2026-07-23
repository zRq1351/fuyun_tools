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

use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;

/// 校验 SHA-256 hex 字符串格式（64位小写十六进制）
pub fn normalize_sha256_hex(raw: &str) -> Option<String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(value)
    } else {
        None
    }
}

/// 从下载地址字符串中分离 URL 和 SHA-256 校验值
/// 格式: `https://example.com/file.exe#sha256=<64位hex>`
pub fn split_download_url_and_sha256(raw: &str) -> Result<(String, Option<String>), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("下载地址为空".to_string());
    }
    if let Some((url, fragment)) = trimmed.split_once("#sha256=") {
        let expected = normalize_sha256_hex(fragment)
            .ok_or_else(|| "SHA-256 格式无效，需要 64 位十六进制字符串".to_string())?;
        return Ok((url.trim().to_string(), Some(expected)));
    }
    Ok((trimmed.to_string(), None))
}

/// 计算文件的 SHA-256 摘要
pub fn compute_file_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("读取下载文件失败: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("读取下载文件失败: {}", e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        hex.push_str(format!("{:02x}", b).as_str());
    }
    Ok(hex)
}

/// 验证下载的可执行文件完整性（MZ 头 + 可选 SHA-256 校验）
pub fn verify_downloaded_exe_integrity(
    path: &Path,
    expected_sha256: Option<&str>,
) -> Result<(), String> {
    let mut header = [0u8; 2];
    let mut file = fs::File::open(path).map_err(|e| format!("读取下载文件失败: {}", e))?;
    file.read_exact(&mut header)
        .map_err(|e| format!("读取下载文件头失败: {}", e))?;
    if header != [b'M', b'Z'] {
        return Err("下载文件不是有效的 Windows 可执行文件".to_string());
    }
    if let Some(expected) = expected_sha256 {
        let actual = compute_file_sha256(path)?;
        if actual != expected {
            return Err(format!(
                "下载文件 SHA-256 校验失败，expected={}, actual={}",
                expected, actual
            ));
        }
    }
    Ok(())
}

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
