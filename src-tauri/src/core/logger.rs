use log::LevelFilter;
use std::time::Instant;
use tauri_plugin_log::{Target, TargetKind};

#[cfg(debug_assertions)]
use crate::utils::utils_helpers::get_logs_dir_path;

/// 日志配置结构体
pub struct LogConfig {
    pub level: LevelFilter,
    pub targets: Vec<Target>,
    pub max_file_size: u128,
}

impl Default for LogConfig {
    fn default() -> Self {
        #[cfg(debug_assertions)]
        let targets = vec![Target::new(TargetKind::Folder {
            path: get_logs_dir_path(),
            file_name: Some(String::from("fuyun_dev")),
        })];

        #[cfg(not(debug_assertions))]
        let targets: Vec<Target> = Vec::new();

        Self {
            level: if cfg!(debug_assertions) {
                LevelFilter::Debug
            } else {
                LevelFilter::Info
            },
            targets,
            max_file_size: 2 * 1024 * 1024, // 2MB
        }
    }
}

/// 配置并构建日志插件
pub fn build_logger() -> tauri_plugin_log::Builder {
    let config = LogConfig::default();

    let mut builder = tauri_plugin_log::Builder::new()
        .level(config.level)
        .max_file_size(config.max_file_size)
        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
        .filter(|metadata| {
            // 过滤掉一些嘈杂的库日志
            if metadata.target().starts_with("tao::")
                || metadata.target().starts_with("mio::")
                || metadata.target().starts_with("hyper::") {
                return false;
            }
            true
        });

    for target in config.targets {
        builder = builder.target(target);
    }

    builder
}

/// 性能埋点工具
pub struct PerfTracer {
    name: String,
    start_time: Instant,
}

impl PerfTracer {
    /// 开始追踪一个操作
    pub fn start(name: impl Into<String>) -> Self {
        let name = name.into();
        log::debug!("[Perf Start] {}", name);
        Self {
            name,
            start_time: Instant::now(),
        }
    }

    /// 结束追踪并记录耗时
    pub fn end(self) {
        let duration = self.start_time.elapsed();
        log::info!("[Perf End] {} - cost: {:?}", self.name, duration);
    }
}
