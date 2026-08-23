//! 应用日志模块。
//!
//! # 日志级别规范
//!
//! 全项目统一遵循以下约定（新增日志前请对照）：
//!
//! | 级别 | 用途 | 预期频率 |
//! |---|---|---|
//! | `error!` | 功能失效、数据丢失风险、需要干预的故障 | 越少越好 |
//! | `warn!` | 异常但已自动恢复/降级/纠偏（看门狗重启、降级轮询、状态自愈） | 个位数/天 |
//! | `info!` | 仅限低频生命周期事件：启动/退出、功能启停、监听器启停、窗口创建销毁、设置保存、用户关键动作完成（如"录制已保存"） | 每次用户操作 ≤2 条 |
//! | `debug!` | 一切流程细节：状态机转换、参数值、重试过程、每次鼠标/键盘/剪贴板事件的常规路径 | 无上限 |
//! | `trace!` | 本项目暂不使用 | - |
//!
//! 判别口诀：**这条日志在正常运行的一天里会出现多少次？** 超过几十次的必须是 `debug!`。
//!
//! # 级别策略
//!
//! - 开发版（debug_assertions）：文件落盘 `Info`，便于调试；
//! - 发布版：文件落盘 `Warn` 及以上，只保留故障与自愈信号，避免热路径噪音；
//! - 运行期可通过设置 `logging_enabled` 整体关闭落盘（即时生效）。

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

        // 开发版记录 Info 便于调试；发布版只落盘 Warn 及以上——
        // 代码热路径（划词/剪贴板）存在大量流程性 Info 日志，全量落盘会产生无用噪音。
        // 发布版保留的 Warn/Error：看门狗恢复、钩子安装失败、panic 拦截、剪贴板恢复失败等关键信号
        #[cfg(debug_assertions)]
        let level = LevelFilter::Info;
        #[cfg(not(debug_assertions))]
        let level = LevelFilter::Warn;

        let targets = vec![Target::new(TargetKind::Folder {
            path: get_logs_dir_path(),
            file_name: Some(file_name),
        })];

        Self {
            level,
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
