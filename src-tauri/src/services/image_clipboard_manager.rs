use crate::core::app_state::AppState;
use crate::core::config::{
    CLIPBOARD_POLL_IDLE_INTERVAL, CLIPBOARD_POLL_MAX_INTERVAL, CLIPBOARD_POLL_MIN_INTERVAL,
    CLIPBOARD_POLL_REPORT_INTERVAL, CLIPBOARD_POLL_WARM_INTERVAL,
};
use crate::services::adaptive_poll::{AdaptivePollConfig, AdaptivePoller};
use crate::services::clipboard_wakeup::ClipboardWakeBackend;
use crate::services::poll_metrics;
use crate::utils::image_clipboard::ImageClipboardManager;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

fn resolve_poll_config_from_state(state: &Arc<Mutex<AppState>>) -> AdaptivePollConfig {
    let guard = state.lock().unwrap();
    let settings = &guard.settings;
    let min_ms = settings.clipboard_poll_min_interval_ms.max(20);
    let warm_ms = settings.clipboard_poll_warm_interval_ms.max(min_ms);
    let idle_ms = settings.clipboard_poll_idle_interval_ms.max(warm_ms);
    let max_ms = settings.clipboard_poll_max_interval_ms.max(idle_ms);
    let report_secs = settings.clipboard_poll_report_interval_secs.max(5);
    AdaptivePollConfig {
        min_interval: Duration::from_millis(min_ms),
        warm_interval: Duration::from_millis(warm_ms),
        idle_interval: Duration::from_millis(idle_ms),
        max_interval: Duration::from_millis(max_ms),
        report_interval: Duration::from_secs(report_secs),
    }
}

fn log_metrics_if_due(
    poller: &mut AdaptivePoller,
    scope: &str,
    metrics_enabled: bool,
    metrics_level: &str,
) {
    if !metrics_enabled {
        return;
    }
    if let Some(report) = poller.metrics_report_if_due(scope) {
        let line = format!(
            "自适应轮询[{}]: mode={}, interval={}ms, wakeups={}, changes={}, busy_skips={}, wakeups_per_sec={:.2}, change_ratio={:.3}",
            report.source,
            report.mode,
            report.interval_ms,
            report.wakeups,
            report.changes,
            report.busy_skips,
            report.wakeups_per_sec,
            report.change_ratio
        );
        poll_metrics::record(report);
        match metrics_level {
            "trace" => log::trace!("{}", line),
            "debug" => log::debug!("{}", line),
            "warn" => log::warn!("{}", line),
            _ => log::info!("{}", line),
        }
    }
}

pub fn start_image_clipboard_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    thread::spawn(move || {
        let mut last_signature = String::new();
        let mut last_error = String::new();
        let mut wake_backend = ClipboardWakeBackend::new();
        let mut poller = AdaptivePoller::new(AdaptivePollConfig {
            min_interval: CLIPBOARD_POLL_MIN_INTERVAL,
            warm_interval: CLIPBOARD_POLL_WARM_INTERVAL,
            idle_interval: CLIPBOARD_POLL_IDLE_INTERVAL,
            max_interval: CLIPBOARD_POLL_MAX_INTERVAL,
            report_interval: CLIPBOARD_POLL_REPORT_INTERVAL,
        });

        loop {
            let (metrics_enabled, metrics_level) = {
                let guard = state.lock().unwrap();
                (
                    guard.settings.clipboard_poll_metrics_enabled,
                    guard.settings.clipboard_poll_metrics_log_level.clone(),
                )
            };
            let runtime_cfg = resolve_poll_config_from_state(&state);
            if poller.config() != runtime_cfg {
                poller.reconfigure(runtime_cfg);
            }
            wake_backend.wait(poller.next_wait());

            let should_skip = {
                let state_guard = state.lock().unwrap();
                state_guard.is_updating_clipboard
                    || state_guard.is_processing_selection
                    || state_guard.is_visible
                    || state_guard.is_image_visible
            };

            if should_skip {
                poller.mark_busy_skip();
                log_metrics_if_due(&mut poller, "image", metrics_enabled, &metrics_level);
                continue;
            }

            let image = ImageClipboardManager::read_clipboard_images_rgba(&app_handle);
            if let Ok(images) = image {
                last_error.clear();
                let signature = build_fast_signature(&images);

                if signature != last_signature {
                    let manager_arc = {
                        let state_guard = state.lock().unwrap();
                        state_guard.image_clipboard_manager.clone()
                    };
                    let manager = manager_arc.lock().unwrap();
                    for (rgba, width, height) in images {
                        manager.add_rgba_image(rgba, width, height);
                    }
                    let _ = app_handle.emit("image-history-updated", serde_json::json!({}));
                    last_signature = signature;
                    poller.mark_change();
                } else {
                    poller.mark_idle();
                }
            } else if let Err(e) = image {
                if e != last_error {
                    if e.contains("当前剪贴板不是位图格式") {
                        log::trace!("图片剪贴板监听读取提示: {}", e);
                    } else {
                        log::debug!("图片剪贴板监听读取失败: {}", e);
                    }
                    last_error = e;
                }
                poller.mark_idle();
            }

            log_metrics_if_due(&mut poller, "image", metrics_enabled, &metrics_level);
        }
    });
}

fn build_fast_signature(images: &[(Vec<u8>, u32, u32)]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    images.len().hash(&mut hasher);
    for (rgba, width, height) in images {
        width.hash(&mut hasher);
        height.hash(&mut hasher);
        rgba.len().hash(&mut hasher);
        if !rgba.is_empty() {
            let step = (rgba.len() / 16).max(1);
            let mut idx = 0usize;
            while idx < rgba.len() {
                rgba[idx].hash(&mut hasher);
                idx += step;
            }
            rgba[rgba.len() - 1].hash(&mut hasher);
        }
    }
    format!("{:x}", hasher.finish())
}
