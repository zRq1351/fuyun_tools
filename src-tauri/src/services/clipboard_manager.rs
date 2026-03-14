use crate::core::app_state::AppState;
use crate::core::config::{
    CLIPBOARD_POLL_IDLE_INTERVAL, CLIPBOARD_POLL_MAX_INTERVAL, CLIPBOARD_POLL_MIN_INTERVAL,
    CLIPBOARD_POLL_REPORT_INTERVAL, CLIPBOARD_POLL_WARM_INTERVAL,
};
use crate::services::adaptive_poll::{AdaptivePollConfig, AdaptivePoller};
use crate::services::clipboard_wakeup::ClipboardWakeBackend;
use crate::services::poll_metrics;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

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

/// 启动剪贴板监听器
pub fn start_clipboard_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    thread::spawn(move || {
        let mut last_content = String::new();
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

            let is_updating = {
                let state_guard = state.lock().unwrap();
                state_guard.is_updating_clipboard
                    || state_guard.is_processing_selection
                    || state_guard.is_visible
                    || state_guard.is_image_visible
            };

            if is_updating {
                poller.mark_busy_skip();
                log_metrics_if_due(&mut poller, "text", metrics_enabled, &metrics_level);
                continue;
            }

            let current_content = {
                let state_guard = state.lock().unwrap();
                let manager = state_guard.clipboard_manager.lock().unwrap();
                manager.get_content(&app_handle)
            };

            if let Some(current_content) = current_content {
                if !current_content.is_empty() && current_content != last_content {
                    add_to_clipboard_history(current_content.clone(), state.clone());
                    last_content = current_content.clone();
                    poller.mark_change();
                    log::info!("检测到剪贴板内容变化，已添加到历史记录");
                } else {
                    poller.mark_idle();
                }
            } else {
                poller.mark_idle();
            }

            log_metrics_if_due(&mut poller, "text", metrics_enabled, &metrics_level);
        }
    });
}

/// 添加到剪贴板历史记录
pub fn add_to_clipboard_history(content: String, state: Arc<Mutex<AppState>>) {
    if content.trim().is_empty() {
        return;
    }

    let should_skip = {
        let state_guard = state.lock().unwrap();
        state_guard.is_processing_selection
    };

    if should_skip {
        log::debug!("正在进行划词操作，跳过添加到历史记录");
        return;
    }

    let manager_result = {
        let state_guard = state.lock().unwrap();
        state_guard.clipboard_manager.clone()
    };

    {
        let manager = manager_result.lock().unwrap();
        manager.add_to_history(content);
    }
}
