use log::LevelFilter;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri_plugin_log::{Target, TargetKind};

use crate::utils::utils_helpers::get_logs_dir_path;

/// 日志落盘运行期开关：通过 filter 回调逐条拦截，切换后即时生效，无需重启
static LOGGING_ENABLED: AtomicBool = AtomicBool::new(true);

/// 设置日志落盘开关
pub fn set_logging_enabled(enabled: bool) {
    LOGGING_ENABLED.store(enabled, Ordering::SeqCst);
}

/// 启动清理时最多保留的日志文件数量（超出部分按修改时间从旧到新删除）
const MAX_LOG_FILES_TO_KEEP: usize = 10;

/// 启动时清理历史日志文件，防止 logs 目录无限累积。
/// 匹配 fuyun*.log（含轮转文件与开发版 fuyun_dev*），当前活跃文件最新、不会被删除。
pub fn cleanup_old_logs() {
    let logs_dir = get_logs_dir_path();
    let Ok(entries) = std::fs::read_dir(&logs_dir) else {
        return;
    };
    let mut log_files: Vec<(std::time::SystemTime, std::path::PathBuf)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("fuyun") && name.ends_with(".log"))
        })
        .filter_map(|path| {
            let modified = path.metadata().and_then(|meta| meta.modified()).ok()?;
            Some((modified, path))
        })
        .collect();
    if log_files.len() <= MAX_LOG_FILES_TO_KEEP {
        return;
    }
    log_files.sort_by_key(|(modified, _)| *modified);
    let remove_count = log_files.len() - MAX_LOG_FILES_TO_KEEP;
    let removed = log_files
        .iter()
        .take(remove_count)
        .filter_map(|(_, path)| std::fs::remove_file(path).ok())
        .count();
    if removed > 0 {
        log::info!(
            "已清理 {} 个历史日志文件（保留最近 {} 个）",
            removed,
            MAX_LOG_FILES_TO_KEEP
        );
    }
}

/// 日志配置结构体
pub struct LogConfig {
    pub level: LevelFilter,
    pub targets: Vec<Target>,
    pub max_file_size: u128,
}

impl Default for LogConfig {
    fn default() -> Self {
        // 开发版与发布版均落盘，便于线上问题诊断；发布版使用独立文件名区分
        #[cfg(debug_assertions)]
        let file_name = String::from("fuyun_dev");
        #[cfg(not(debug_assertions))]
        let file_name = String::from("fuyun");

        let targets = vec![Target::new(TargetKind::Folder {
            path: get_logs_dir_path(),
            file_name: Some(file_name),
        })];

        Self {
            level: LevelFilter::Info,
            targets,
            max_file_size: 2 * 1024 * 1024,
        }
    }
}

/// 配置并构建日志插件
pub fn build_logger() -> tauri_plugin_log::Builder {
    let config = LogConfig::default();

    let mut builder = tauri_plugin_log::Builder::new()
        .level(config.level)
        .max_file_size(config.max_file_size)
        // 插件默认使用 UTC 时间戳，改为本地时区，避免日志时间与用户实际时间不符
        .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
        .filter(|metadata| {
            // 用户可通过设置关闭日志落盘（即时生效）
            if !LOGGING_ENABLED.load(Ordering::SeqCst) {
                return false;
            }
            if metadata.target().starts_with("tao::")
                || metadata.target().starts_with("mio::")
                || metadata.target().starts_with("hyper::")
            {
                return false;
            }
            true
        });

    for target in config.targets {
        builder = builder.target(target);
    }

    builder
}
