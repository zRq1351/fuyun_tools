use crate::core::app_state::AppState as SharedAppState;
use crate::core::config::{AIProvider, ProviderConfig};
use crate::core::error::{to_frontend_error_string, AppError, ErrorCode};
use crate::features;
use crate::services::ai_client::{AIClient, AIConfig};
use crate::services::clipboard_manager::set_clipboard_listener_enabled;
use crate::services::image_clipboard_manager::set_image_clipboard_listener_enabled;
use crate::sync::Mutex;
use crate::ui::commands_recording::{
    toggle_microphone_from_shortcut, toggle_recording_from_shortcut,
};
use crate::ui::tray_menu::open_settings;
use crate::ui::window_manager::{
    bind_overlay_window_events, hide_overlay_window_by_label, show_overlay_window_by_label,
};
use crate::utils::image_clipboard::get_image_persist_queue_metrics_snapshot;
use crate::utils::image_clipboard::set_image_fill_verify_mode;
#[cfg(debug_assertions)]
use crate::utils::utils_helpers::get_dedup_scan_metrics;
use crate::utils::utils_helpers::{
    default_explanation_prompt_template, default_translation_prompt_template, save_settings,
};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use xxhash_rust::xxh3::xxh3_64;
use crate::ui::commands_screenshot::close_screenshot_window;
use crate::ui::commands_clipboard::*;
use crate::ui::commands_writeback::{WriteBackExecutionResult, simulate_paste_with_retry, emit_writeback_phase, record_writeback_stage_metric, emit_writeback_result};

pub(crate) static NEXT_SCREENSHOT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
pub(crate) static NEXT_PINNED_IMAGE_WINDOW_ID: AtomicU64 = AtomicU64::new(1);
pub(crate) static SCREENSHOT_LIFECYCLE_BOUND_FOR_BOOT_WINDOW: AtomicBool = AtomicBool::new(false);
static SCREENSHOT_BOOT_IMAGE_PATH: OnceLock<StdMutex<Option<PathBuf>>> = OnceLock::new();
static SCREENSHOT_BOOT_IMAGE_PATHS: OnceLock<StdMutex<HashSet<PathBuf>>> = OnceLock::new();
pub(crate) static RECENT_COPY_PASTE: OnceLock<StdMutex<Option<RecentCopyPaste>>> = OnceLock::new();
pub(crate) static COPY_PASTE_DEDUP_ENABLED: AtomicBool = AtomicBool::new(true);
pub(crate) static COPY_PASTE_DEDUP_WINDOW_MS: AtomicU64 = AtomicU64::new(1200);
pub(crate) static COPY_PASTE_DEDUP_LOG_ENABLED: AtomicBool = AtomicBool::new(true);
pub(crate) static COPY_PASTE_DEDUP_TOTAL_REQUESTS: AtomicU64 = AtomicU64::new(0);
pub(crate) static COPY_PASTE_DEDUP_HIT_COUNT: AtomicU64 = AtomicU64::new(0);
pub(crate) static COPY_PASTE_DEDUP_REQUEST_ID_HIT_COUNT: AtomicU64 = AtomicU64::new(0);
pub(crate) static COPY_PASTE_DEDUP_TEXT_HASH_HIT_COUNT: AtomicU64 = AtomicU64::new(0);
pub(crate) static COPY_PASTE_DEDUP_LOG_COUNT: AtomicU64 = AtomicU64::new(0);
pub(crate) static COPY_PASTE_DEDUP_WINDOW_STATS: OnceLock<StdMutex<DedupWindowStats>> = OnceLock::new();
pub(crate) static AUTO_BACKUP_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
pub(crate) static BACKUP_JOB_MUTEX: OnceLock<tauri::async_runtime::Mutex<()>> = OnceLock::new();

pub(crate) struct RecentCopyPaste {
    pub(crate) request_id: String,
    pub(crate) text_hash: u64,
    pub(crate) created_at_ms: u64,
}

pub(crate) struct DedupWindowStats {
    pub(crate) window_start_ms: u64,
    pub(crate) requests: u64,
    pub(crate) hits: u64,
    pub(crate) last_hit_at_ms: u64,
}


pub(crate) fn calc_text_hash(text: &str) -> u64 {
    xxh3_64(text.as_bytes())
}

pub(crate) fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn screenshot_boot_image_slot() -> &'static StdMutex<Option<PathBuf>> {
    SCREENSHOT_BOOT_IMAGE_PATH.get_or_init(|| StdMutex::new(None))
}

pub(crate) fn screenshot_boot_image_paths() -> &'static StdMutex<HashSet<PathBuf>> {
    SCREENSHOT_BOOT_IMAGE_PATHS.get_or_init(|| StdMutex::new(HashSet::new()))
}

pub(crate) fn replace_screenshot_boot_image_path(next_path: Option<PathBuf>) {
    let mut slot = match screenshot_boot_image_slot().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    *slot = next_path.clone();
    if let Some(path) = next_path {
        if let Ok(mut paths) = screenshot_boot_image_paths().lock() {
            paths.insert(path);
        }
    }
}

pub(crate) fn cleanup_all_screenshot_boot_images() {
    if let Ok(mut slot) = screenshot_boot_image_slot().lock() {
        *slot = None;
    }
    let paths = match screenshot_boot_image_paths().lock() {
        Ok(mut guard) => std::mem::take(&mut *guard),
        Err(_) => return,
    };
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

pub(crate) fn build_screenshot_boot_image_path(session_id: u64) -> Result<PathBuf, String> {
    let mut dir = std::env::current_exe().map_err(|e| format!("获取程序目录失败: {}", e))?;
    dir.pop();
    dir.push("screenshot_boot");
    fs::create_dir_all(&dir).map_err(|e| format!("创建截图启动目录失败: {}", e))?;
    Ok(dir.join(format!("screenshot_boot_{}.png", session_id)))
}

pub(crate) fn write_screenshot_boot_image(
    rgba: &[u8],
    width: u32,
    height: u32,
    session_id: u64,
) -> Result<PathBuf, String> {
    let png_data = crate::features::screenshot::capture::rgba_to_png_bytes(rgba, width, height)?;
    let path = build_screenshot_boot_image_path(session_id)?;
    fs::write(&path, png_data).map_err(|e| format!("写入截图临时文件失败: {}", e))?;
    replace_screenshot_boot_image_path(Some(path.clone()));
    Ok(path)
}


pub(crate) fn is_duplicate_copy_paste_request(text: &str, request_id: Option<&str>) -> bool {
    COPY_PASTE_DEDUP_TOTAL_REQUESTS.fetch_add(1, Ordering::Relaxed);
    if !COPY_PASTE_DEDUP_ENABLED.load(Ordering::Relaxed) {
        return false;
    }
    let request_id_trimmed = request_id.unwrap_or("").trim();
    let text_hash = calc_text_hash(text);
    let now_ms = now_unix_ms();
    let dedup_window_ms = COPY_PASTE_DEDUP_WINDOW_MS.load(Ordering::Relaxed);
    let lock = RECENT_COPY_PASTE.get_or_init(|| StdMutex::new(None));
    let mut guard = lock.lock().unwrap_or_else(|poisoned| {
        log::warn!("复制粘贴去重锁中毒，尝试恢复");
        poisoned.into_inner()
    });
    let mut is_hit = false;
    if let Some(last) = guard.as_ref() {
        let within_window = now_ms.saturating_sub(last.created_at_ms) <= dedup_window_ms;
        let same_request_id =
            !request_id_trimmed.is_empty() && request_id_trimmed == last.request_id;
        let same_text_hash = last.text_hash == text_hash;
        if within_window && (same_request_id || same_text_hash) {
            COPY_PASTE_DEDUP_HIT_COUNT.fetch_add(1, Ordering::Relaxed);
            if same_request_id {
                COPY_PASTE_DEDUP_REQUEST_ID_HIT_COUNT.fetch_add(1, Ordering::Relaxed);
            } else {
                COPY_PASTE_DEDUP_TEXT_HASH_HIT_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            is_hit = true;
        }
    }
    let stats_lock = COPY_PASTE_DEDUP_WINDOW_STATS.get_or_init(|| {
        StdMutex::new(DedupWindowStats {
            window_start_ms: now_ms,
            requests: 0,
            hits: 0,
            last_hit_at_ms: 0,
        })
    });
    let mut stats = stats_lock.lock().unwrap_or_else(|poisoned| {
        log::warn!("复制粘贴去重窗口统计锁中毒，尝试恢复");
        poisoned.into_inner()
    });
    if now_ms.saturating_sub(stats.window_start_ms) > dedup_window_ms {
        stats.window_start_ms = now_ms;
        stats.requests = 0;
        stats.hits = 0;
    }
    stats.requests = stats.requests.saturating_add(1);
    if is_hit {
        stats.hits = stats.hits.saturating_add(1);
        stats.last_hit_at_ms = now_ms;
        return true;
    }
    *guard = Some(RecentCopyPaste {
        request_id: request_id_trimmed.to_string(),
        text_hash,
        created_at_ms: now_ms,
    });
    false
}

pub(crate) fn get_copy_paste_dedup_debug_state_value() -> serde_json::Value {
    let now_ms = now_unix_ms();
    let dedup_window_ms = COPY_PASTE_DEDUP_WINDOW_MS.load(Ordering::Relaxed);
    let stats_lock = COPY_PASTE_DEDUP_WINDOW_STATS.get_or_init(|| {
        StdMutex::new(DedupWindowStats {
            window_start_ms: now_ms,
            requests: 0,
            hits: 0,
            last_hit_at_ms: 0,
        })
    });
    let stats = stats_lock.lock().unwrap_or_else(|poisoned| {
        log::warn!("复制粘贴去重窗口统计锁中毒，尝试恢复");
        poisoned.into_inner()
    });
    let mut window_requests = stats.requests;
    let mut window_hits = stats.hits;
    if now_ms.saturating_sub(stats.window_start_ms) > dedup_window_ms {
        window_requests = 0;
        window_hits = 0;
    }
    let window_hit_rate = if window_requests == 0 {
        0.0
    } else {
        (window_hits as f64 / window_requests as f64) * 100.0
    };
    serde_json::json!({
        "enabled": COPY_PASTE_DEDUP_ENABLED.load(Ordering::Relaxed),
        "window_ms": COPY_PASTE_DEDUP_WINDOW_MS.load(Ordering::Relaxed),
        "log_enabled": COPY_PASTE_DEDUP_LOG_ENABLED.load(Ordering::Relaxed),
        "metrics": {
            "total_requests": COPY_PASTE_DEDUP_TOTAL_REQUESTS.load(Ordering::Relaxed),
            "dedup_hits": COPY_PASTE_DEDUP_HIT_COUNT.load(Ordering::Relaxed),
            "request_id_hits": COPY_PASTE_DEDUP_REQUEST_ID_HIT_COUNT.load(Ordering::Relaxed),
            "text_hash_hits": COPY_PASTE_DEDUP_TEXT_HASH_HIT_COUNT.load(Ordering::Relaxed),
            "log_count": COPY_PASTE_DEDUP_LOG_COUNT.load(Ordering::Relaxed),
            "window_requests": window_requests,
            "window_hits": window_hits,
            "window_hit_rate_percent": window_hit_rate,
            "last_hit_at_ms": stats.last_hit_at_ms,
        }
    })
}

pub(crate) fn bind_screenshot_window_lifecycle(window: &tauri::WebviewWindow, app: &AppHandle) {
    bind_overlay_window_events(window, app.clone(), "screenshot");
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed => {
            features::screenshot::capture::set_screenshot_in_progress(false);
        }
        _ => {}
    });
}


#[tauri::command]
pub async fn selection_toolbar_blur(app: AppHandle) -> Result<(), String> {
    let _ = hide_overlay_window_by_label(&app, "selection_toolbar");
    Ok(())
}

#[tauri::command]
pub async fn open_settings_window(
    tab: Option<String>,
    reason: Option<String>,
    app: AppHandle,
) -> Result<(), String> {
    open_settings(&app);
    if let Some(settings_window) = app.get_webview_window("settings") {
        let payload = serde_json::json!({
            "tab": tab.unwrap_or_else(|| "ai".to_string()),
            "reason": reason.unwrap_or_default()
        });
        let _ = settings_window.emit("navigate-settings-tab", payload);
    }
    Ok(())
}

pub(crate) fn register_recording_shortcut(
    app: &AppHandle,
    state: Arc<Mutex<SharedAppState>>,
    hot_key: &str,
) -> Result<(), String> {
    let app_clone = app.clone();
    app.global_shortcut()
        .on_shortcut(hot_key, move |_app, _shortcut, event| {
            if let ShortcutState::Pressed = event.state {
                let app_handle_inner = app_clone.clone();
                let state_inner = state.clone();
                tauri::async_runtime::spawn(async move {
                    toggle_recording_from_shortcut(app_handle_inner, state_inner).await;
                });
            }
        })
        .map_err(|e| frontend_error(ErrorCode::ValidationError, format!("录屏快捷键被占用或注册失败：{}", hot_key), e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn show_selection_toolbar_with_text(
    app: AppHandle,
    text: String,
    x: i32,
    y: i32,
) -> Result<(), String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        log::warn!("show_selection_toolbar_with_text 收到空文本，忽略");
        return Ok(());
    }
    let content = trimmed.to_string();
    log::info!(
        "show_selection_toolbar_with_text: len={}, x={}, y={}",
        content.chars().count(),
        x,
        y
    );
    if app.get_webview_window("selection_toolbar").is_none() {
        let toolbar_window = tauri::WebviewWindowBuilder::new(
            &app,
            "selection_toolbar",
            tauri::WebviewUrl::App("selection_toolbar.html".into()),
        )
        .title("fuyun_tools")
        .visible(false)
        .resizable(false)
        .decorations(false)
        .shadow(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .accept_first_mouse(true)
        .build()
        .map_err(|e| format!("创建划词工具栏窗口失败: {}", e))?;
        bind_overlay_window_events(&toolbar_window, app.clone(), "selection_toolbar");
        log::info!("show_selection_toolbar_with_text: 已创建selection_toolbar窗口");
    }
    crate::ui::window_manager::show_selection_toolbar_force_impl(
        app.clone(),
        content.clone(),
        Some((x, y)),
    );
    if let Some(toolbar_window) = app.get_webview_window("selection_toolbar") {
        let payload =
            serde_json::to_string(&content).map_err(|e| format!("序列化文本失败: {}", e))?;
        let script = format!(
            "window.__SELECTION_TOOLBAR_TEXT__ = {payload}; window.dispatchEvent(new CustomEvent('selection-toolbar-text', {{ detail: {payload} }}));"
        );
        let _ = toolbar_window.eval(&script);
    }
    Ok(())
}

#[tauri::command]
pub async fn show_ocr_text_window(
    app: AppHandle,
    source_label: String,
    text: String,
) -> Result<(), String> {
    let content = text.trim().to_string();
    if content.is_empty() {
        return Ok(());
    }

    let source = app
        .get_webview_window(&source_label)
        .ok_or_else(|| "源窗口不存在".to_string())?;
    let source_pos = source
        .outer_position()
        .map_err(|e| format!("获取源窗口位置失败: {}", e))?;
    let source_size = source
        .outer_size()
        .map_err(|e| format!("获取源窗口尺寸失败: {}", e))?;
    let monitor = source
        .current_monitor()
        .map_err(|e| format!("获取显示器信息失败: {}", e))?
        .ok_or_else(|| "未找到显示器信息".to_string())?;

    let result_label = format!("ocr_text_{}", source_label.replace('-', "_"));
    let window = if let Some(existing) = app.get_webview_window(&result_label) {
        existing
    } else {
        let window = tauri::WebviewWindowBuilder::new(
            &app,
            result_label.clone(),
            tauri::WebviewUrl::App("ocr_text.html".into()),
        )
        .title("OCR识别结果")
        .visible(false)
        .decorations(false)
        .always_on_top(true)
        .resizable(true)
        .inner_size(560.0, 240.0)
        .build()
        .map_err(|e| format!("创建OCR结果窗口失败: {}", e))?;
        bind_overlay_window_events(&window, app.clone(), result_label.clone());
        window
    };

    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let target_width = (source_size.width as i32).min(monitor_size.width as i32);
    let target_height = 240i32;
    let gap = 8i32;
    let min_x = monitor_pos.x;
    let min_y = monitor_pos.y;
    let max_x = monitor_pos.x + monitor_size.width as i32 - target_width;
    let max_y = monitor_pos.y + monitor_size.height as i32 - target_height;

    let mut target_x = source_pos.x.clamp(min_x, max_x.max(min_x));
    let below_y = source_pos.y + source_size.height as i32 + gap;
    let above_y = source_pos.y - target_height - gap;
    let target_y = if below_y <= max_y {
        below_y
    } else if above_y >= min_y {
        above_y
    } else {
        below_y.clamp(min_y, max_y.max(min_y))
    };
    target_x = target_x.clamp(min_x, max_x.max(min_x));

    let _ = window.set_size(tauri::PhysicalSize::new(
        target_width as u32,
        target_height as u32,
    ));
    let _ = window.set_always_on_top(true);
    let _ = window.set_position(tauri::PhysicalPosition::new(target_x, target_y));
    let _ = show_overlay_window_by_label(&app, &result_label, true);

    let payload = serde_json::json!({"text": content});
    let script = format!(
        "window.__OCR_TEXT_PAYLOAD__ = {payload}; window.dispatchEvent(new CustomEvent('ocr-text-data', {{ detail: {payload} }}));"
    );
    let _ = window.eval(&script);
    Ok(())
}

#[tauri::command]
pub async fn get_ai_settings(state: State<'_, Arc<Mutex<SharedAppState>>>) -> Result<serde_json::Value, String> {
    // 直接从内存中的 AppState 获取配置，避免重复从磁盘加载
    let settings = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.settings.clone()
    };

    tauri::async_runtime::spawn_blocking(move || {
        // 直接将整个 settings 序列化为 JSON Value
        let mut settings_json = serde_json::to_value(&settings)
            .map_err(|e| frontend_error(ErrorCode::SystemError, "序列化设置失败", e.to_string()))?;

        // 脱敏处理 provider_configs 中的 API 密钥
        if let Some(settings_obj) = settings_json.as_object_mut() {
            let mut provider_configs_map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            
            for provider_key in settings.provider_configs.keys() {
                if let Some(decrypted_config) = settings.provider_configs.get(provider_key) {
                    // 不再调用 get_provider_api_key()，直接返回脱敏标记
                    // 这样可以避免每次访问都读取密钥环，提升性能
                    let config_obj = serde_json::json!({
                        "api_url": decrypted_config.api_url,
                        "model_name": decrypted_config.model_name,
                        "api_key": "********"  // 始终返回脱敏标记
                    });
                    provider_configs_map.insert(provider_key.clone(), config_obj);
                }
            }

            // 替换 provider_configs 为脱敏后的版本
            settings_obj.insert(
                "provider_configs".to_string(),
                serde_json::Value::Object(provider_configs_map),
            );
        }

        Ok(settings_json)
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "读取AI设置任务执行失败",
            e.to_string(),
        )
    })?
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_text_dedup_metrics() -> Result<serde_json::Value, String> {
    serde_json::to_value(get_dedup_scan_metrics())
        .map_err(|e| frontend_error(ErrorCode::SystemError, "序列化去重指标失败", e.to_string()))
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_image_storage_metrics(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    let metrics = manager.get_storage_metrics();
    serde_json::to_value(metrics).map_err(|e| {
        to_frontend_error_string(
            AppError::new(ErrorCode::SystemError, "序列化图片存储指标失败")
                .with_details(e.to_string()),
        )
    })
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_copy_paste_dedup_debug_state() -> Result<serde_json::Value, String> {
    Ok(get_copy_paste_dedup_debug_state_value())
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_image_persist_queue_metrics() -> Result<serde_json::Value, String> {
    serde_json::to_value(get_image_persist_queue_metrics_snapshot()).map_err(|e| {
        to_frontend_error_string(
            AppError::new(ErrorCode::SystemError, "序列化图片持久化队列指标失败")
                .with_details(e.to_string()),
        )
    })
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn set_copy_paste_dedup_debug_config(
    enabled: Option<bool>,
    window_ms: Option<u64>,
    log_enabled: Option<bool>,
    reset_metrics: Option<bool>,
) -> Result<serde_json::Value, String> {
    if let Some(enabled) = enabled {
        COPY_PASTE_DEDUP_ENABLED.store(enabled, Ordering::Relaxed);
    }
    if let Some(window_ms) = window_ms {
        let clamped = window_ms.clamp(50, 10_000);
        COPY_PASTE_DEDUP_WINDOW_MS.store(clamped, Ordering::Relaxed);
        if let Some(lock) = COPY_PASTE_DEDUP_WINDOW_STATS.get() {
            let mut stats = lock.lock().unwrap_or_else(|poisoned| {
                log::warn!("复制粘贴去重窗口统计锁中毒，尝试恢复");
                poisoned.into_inner()
            });
            stats.window_start_ms = now_unix_ms();
            stats.requests = 0;
            stats.hits = 0;
        }
    }
    if let Some(log_enabled) = log_enabled {
        COPY_PASTE_DEDUP_LOG_ENABLED.store(log_enabled, Ordering::Relaxed);
    }
    if reset_metrics.unwrap_or(false) {
        COPY_PASTE_DEDUP_TOTAL_REQUESTS.store(0, Ordering::Relaxed);
        COPY_PASTE_DEDUP_HIT_COUNT.store(0, Ordering::Relaxed);
        COPY_PASTE_DEDUP_REQUEST_ID_HIT_COUNT.store(0, Ordering::Relaxed);
        COPY_PASTE_DEDUP_TEXT_HASH_HIT_COUNT.store(0, Ordering::Relaxed);
        COPY_PASTE_DEDUP_LOG_COUNT.store(0, Ordering::Relaxed);
        if let Some(lock) = COPY_PASTE_DEDUP_WINDOW_STATS.get() {
            let mut stats = lock.lock().unwrap_or_else(|poisoned| {
                log::warn!("复制粘贴去重窗口统计锁中毒，尝试恢复");
                poisoned.into_inner()
            });
            stats.window_start_ms = now_unix_ms();
            stats.requests = 0;
            stats.hits = 0;
            stats.last_hit_at_ms = 0;
        }
    }
    Ok(get_copy_paste_dedup_debug_state_value())
}

#[tauri::command]
pub async fn save_app_settings(
    text_max_items: Option<usize>,
    image_max_items: Option<usize>,
    image_disk_limit_mb: Option<u64>,
    ai_provider: Option<String>,
    ai_api_url: Option<String>,
    ai_model_name: Option<String>,
    ai_api_key: Option<String>,
    hot_key: Option<String>,
    image_hot_key: Option<String>,
    screenshot_hot_key: Option<String>,
    recording_hot_key: Option<String>,
    recording_mic_toggle_hot_key: Option<String>,
    text_clipboard_enabled: Option<bool>,
    image_clipboard_enabled: Option<bool>,
    screenshot_enabled: Option<bool>,
    recording_enabled: Option<bool>,
    selection_enabled: Option<bool>,
    selection_modifier_key: Option<String>,
    selection_custom_prompts: Option<Vec<crate::utils::settings_model::CustomPrompt>>,
    selection_web_search_enabled: Option<bool>,
    selection_web_search_engine: Option<String>,
    grouped_items_protected_from_limit: Option<bool>,
    translation_prompt_template: Option<String>,
    explanation_prompt_template: Option<String>,
    image_fill_verify_mode: Option<String>,
    ocr_engine: Option<String>,
    recording_default_fps: Option<u32>,
    recording_default_video_bitrate_kbps: Option<u32>,
    recording_default_audio_bitrate_kbps: Option<u32>,
    recording_capture_cursor: Option<bool>,
    recording_capture_system_audio: Option<bool>,
    recording_capture_microphone: Option<bool>,
    recording_microphone_device_id: Option<String>,
    recording_output_dir: Option<String>,
    recording_auto_open_folder: Option<bool>,
    recording_toolbar_content_protected: Option<bool>,
    recording_max_duration_minutes: Option<u32>,
    recording_file_name_template: Option<String>,
    recording_ffmpeg_download_url: Option<String>,
    recording_window_audio_sync_advance_ms: Option<u32>,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let version = app.package_info().version.to_string();

    let mut settings = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.settings.clone()
    };

    settings.version = version;

    if let Some(val) = text_max_items {
        settings.max_items = val;
        settings.text_max_items = val;
    }
    if let Some(val) = image_max_items {
        settings.image_max_items = val;
    }
    if let Some(val) = image_disk_limit_mb {
        settings.image_disk_limit_mb = val;
    }
    if let Some(val) = text_clipboard_enabled {
        settings.text_clipboard_enabled = val;
    }
    if let Some(val) = image_clipboard_enabled {
        settings.image_clipboard_enabled = val;
    }
    if let Some(val) = screenshot_enabled {
        settings.screenshot_enabled = val;
    }
    if let Some(val) = recording_enabled {
        settings.recording_enabled = val;
    }
    if let Some(val) = selection_enabled {
        settings.selection_enabled = val;
    }
    if let Some(val) = selection_modifier_key {
        settings.selection_modifier_key = val;
    }
    if let Some(val) = selection_custom_prompts {
        settings.selection_custom_prompts = val;
    }
    if let Some(val) = selection_web_search_enabled {
        settings.selection_web_search_enabled = val;
    }
    if let Some(val) = selection_web_search_engine {
        settings.selection_web_search_engine = val;
    }
    if let Some(val) = grouped_items_protected_from_limit {
        settings.grouped_items_protected_from_limit = val;
    }
    if let Some(val) = translation_prompt_template {
        settings.translation_prompt_template = if val.trim().is_empty() {
            default_translation_prompt_template()
        } else {
            val
        };
    }
    if let Some(val) = explanation_prompt_template {
        settings.explanation_prompt_template = if val.trim().is_empty() {
            default_explanation_prompt_template()
        } else {
            val
        };
    }
    if let Some(val) = image_fill_verify_mode {
        settings.image_fill_verify_mode = if val == "fast" {
            "fast".to_string()
        } else {
            "strict".to_string()
        };
    }
    if let Some(val) = ocr_engine {
        settings.ocr_engine = if val == "windows-native" {
            "windows-native".to_string()
        } else {
            "ocr-rs".to_string()
        };
    }
    if let Some(val) = recording_default_fps {
        settings.recording_default_fps = val.clamp(1, 120);
    }
    if let Some(val) = recording_default_video_bitrate_kbps {
        settings.recording_default_video_bitrate_kbps = val.clamp(500, 50000);
    }
    if let Some(val) = recording_default_audio_bitrate_kbps {
        settings.recording_default_audio_bitrate_kbps = val.clamp(32, 512);
    }
    if let Some(val) = recording_capture_cursor {
        settings.recording_capture_cursor = val;
    }
    if let Some(val) = recording_capture_system_audio {
        settings.recording_capture_system_audio = val;
    }
    if let Some(val) = recording_capture_microphone {
        settings.recording_capture_microphone = val;
    }
    if let Some(val) = recording_microphone_device_id {
        settings.recording_microphone_device_id = val.trim().to_string();
    }
    if let Some(val) = recording_output_dir {
        settings.recording_output_dir = val.trim().to_string();
    }
    if let Some(val) = recording_auto_open_folder {
        settings.recording_auto_open_folder = val;
    }
    if let Some(val) = recording_toolbar_content_protected {
        settings.recording_toolbar_content_protected = val;
    }
    if let Some(val) = recording_max_duration_minutes {
        settings.recording_max_duration_minutes = val.clamp(1, 1440);
    }
    if let Some(val) = recording_file_name_template {
        settings.recording_file_name_template = if val.trim().is_empty() {
            "{timestamp}".to_string()
        } else {
            val
        };
    }
    if let Some(val) = recording_ffmpeg_download_url {
        settings.recording_ffmpeg_download_url = if val.trim().is_empty() {
            settings.recording_ffmpeg_download_url
        } else {
            val.trim().to_string()
        };
    }
    if let Some(val) = recording_window_audio_sync_advance_ms {
        settings.recording_window_audio_sync_advance_ms = val.clamp(0, 500);
    }

    if let Some(ref hot_key_val) = hot_key {
        if hot_key_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "快捷键不能为空",
                "hot_key is empty",
            ));
        }

        if hot_key_val != &settings.hot_key {
            let old_hot_key = settings.hot_key.clone();
            if settings.text_clipboard_enabled {
                register_text_shortcut(&app, state.inner().clone(), hot_key_val.as_str())?;
            }
            if let Err(e) = app.global_shortcut().unregister(old_hot_key.as_str()) {
                log::warn!(
                    "注销旧快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_hot_key,
                    e
                );
            }
            settings.hot_key = hot_key_val.clone();
        }
    }

    if let Some(ref image_hot_key_val) = image_hot_key {
        if image_hot_key_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "图片窗口快捷键不能为空",
                "image_hot_key is empty",
            ));
        }

        if image_hot_key_val != &settings.image_hot_key {
            if let Some(ref hot_key_val) = hot_key {
                if image_hot_key_val == hot_key_val {
                    return Err(frontend_error(
                        ErrorCode::ValidationError,
                        "文字与图片窗口快捷键不能相同",
                        format!(
                            "hot_key={}, image_hot_key={}",
                            hot_key_val, image_hot_key_val
                        ),
                    ));
                }
            } else if image_hot_key_val == &settings.hot_key {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    "文字与图片窗口快捷键不能相同",
                    format!(
                        "hot_key={}, image_hot_key={}",
                        settings.hot_key, image_hot_key_val
                    ),
                ));
            }

            let old_image_hot_key = settings.image_hot_key.clone();
            if settings.image_clipboard_enabled {
                register_image_shortcut(&app, state.inner().clone(), image_hot_key_val.as_str())?;
            }
            if let Err(e) = app.global_shortcut().unregister(old_image_hot_key.as_str()) {
                log::warn!(
                    "注销旧图片快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_image_hot_key,
                    e
                );
            }
            settings.image_hot_key = image_hot_key_val.clone();
        }
    }

    if let Some(ref screenshot_hot_key_val) = screenshot_hot_key {
        if screenshot_hot_key_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "截图快捷键不能为空",
                "screenshot_hot_key is empty",
            ));
        }

        if screenshot_hot_key_val != &settings.screenshot_hot_key {
            let effective_hot_key = hot_key.clone().unwrap_or_else(|| settings.hot_key.clone());
            let effective_image_hot_key = image_hot_key
                .clone()
                .unwrap_or_else(|| settings.image_hot_key.clone());
            if screenshot_hot_key_val == &effective_hot_key
                || screenshot_hot_key_val == &effective_image_hot_key
            {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    "截图快捷键不能与文字或图片窗口快捷键相同",
                    format!(
                        "hot_key={}, image_hot_key={}, screenshot_hot_key={}",
                        effective_hot_key, effective_image_hot_key, screenshot_hot_key_val
                    ),
                ));
            }

            if app
                .global_shortcut()
                .is_registered(screenshot_hot_key_val.as_str())
            {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    format!("截图快捷键被占用：{}", screenshot_hot_key_val),
                    "screenshot global shortcut already registered",
                ));
            }

            if let Err(e) = app
                .global_shortcut()
                .unregister(settings.screenshot_hot_key.as_str())
            {
                log::warn!(
                    "注销旧截图快捷键 '{}' 失败 (可能从未注册成功): {}",
                    settings.screenshot_hot_key,
                    e
                );
            }
            if settings.screenshot_enabled {
                register_screenshot_shortcut(&app, screenshot_hot_key_val.as_str())?;
            }
            settings.screenshot_hot_key = screenshot_hot_key_val.clone();
        }
    }

    if let Some(ref recording_hot_key_val) = recording_hot_key {
        if recording_hot_key_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "录屏快捷键不能为空",
                "recording_hot_key is empty",
            ));
        }
        if recording_hot_key_val != &settings.recording_hot_key {
            let effective_hot_key = hot_key.clone().unwrap_or_else(|| settings.hot_key.clone());
            let effective_image_hot_key = image_hot_key
                .clone()
                .unwrap_or_else(|| settings.image_hot_key.clone());
            let effective_screenshot_hot_key = screenshot_hot_key
                .clone()
                .unwrap_or_else(|| settings.screenshot_hot_key.clone());
            if recording_hot_key_val == &effective_hot_key
                || recording_hot_key_val == &effective_image_hot_key
                || recording_hot_key_val == &effective_screenshot_hot_key
            {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    "录屏快捷键不能与文字、图片或截图快捷键相同",
                    format!(
                        "hot_key={}, image_hot_key={}, screenshot_hot_key={}, recording_hot_key={}",
                        effective_hot_key,
                        effective_image_hot_key,
                        effective_screenshot_hot_key,
                        recording_hot_key_val
                    ),
                ));
            }

            if app
                .global_shortcut()
                .is_registered(recording_hot_key_val.as_str())
            {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    format!("录屏快捷键被占用：{}", recording_hot_key_val),
                    "recording global shortcut already registered",
                ));
            }

            if let Err(e) = app
                .global_shortcut()
                .unregister(settings.recording_hot_key.as_str())
            {
                log::warn!(
                    "注销旧录屏快捷键 '{}' 失败 (可能从未注册成功): {}",
                    settings.recording_hot_key,
                    e
                );
            }
            if settings.recording_enabled {
                register_recording_shortcut(
                    &app,
                    state.inner().clone(),
                    recording_hot_key_val.as_str(),
                )?;
            }
            settings.recording_hot_key = recording_hot_key_val.clone();
        }
    }

    if let Some(ref mic_toggle_hot_key_val) = recording_mic_toggle_hot_key {
        if mic_toggle_hot_key_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "麦克风切换快捷键不能为空",
                "recording_mic_toggle_hot_key is empty",
            ));
        }
        if mic_toggle_hot_key_val != &settings.recording_mic_toggle_hot_key {
            let effective_hot_key = hot_key.clone().unwrap_or_else(|| settings.hot_key.clone());
            let effective_image_hot_key = image_hot_key
                .clone()
                .unwrap_or_else(|| settings.image_hot_key.clone());
            let effective_screenshot_hot_key = screenshot_hot_key
                .clone()
                .unwrap_or_else(|| settings.screenshot_hot_key.clone());
            let effective_recording_hot_key = recording_hot_key
                .clone()
                .unwrap_or_else(|| settings.recording_hot_key.clone());

            if mic_toggle_hot_key_val == &effective_hot_key
                || mic_toggle_hot_key_val == &effective_image_hot_key
                || mic_toggle_hot_key_val == &effective_screenshot_hot_key
                || mic_toggle_hot_key_val == &effective_recording_hot_key
            {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    "麦克风切换快捷键不能与其他快捷键相同",
                    format!(
                        "hot_key={}, image_hot_key={}, screenshot_hot_key={}, recording_hot_key={}, mic_toggle_hot_key={}",
                        effective_hot_key,
                        effective_image_hot_key,
                        effective_screenshot_hot_key,
                        effective_recording_hot_key,
                        mic_toggle_hot_key_val
                    ),
                ));
            }

            if app
                .global_shortcut()
                .is_registered(mic_toggle_hot_key_val.as_str())
            {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    format!("麦克风切换快捷键被占用：{}", mic_toggle_hot_key_val),
                    "mic toggle global shortcut already registered",
                ));
            }

            if let Err(e) = app
                .global_shortcut()
                .unregister(settings.recording_mic_toggle_hot_key.as_str())
            {
                log::warn!(
                    "注销旧麦克风切换快捷键 '{}' 失败: {}",
                    settings.recording_mic_toggle_hot_key,
                    e
                );
            }

            if settings.recording_enabled {
                let app_handle_for_mic = app.clone();
                if let Err(e) = app.global_shortcut().on_shortcut(
                    mic_toggle_hot_key_val.as_str(),
                    move |_app, _shortcut, event| {
                        let app_handle_inner = app_handle_for_mic.clone();
                        match event.state {
                            ShortcutState::Pressed => {
                                tauri::async_runtime::spawn(async move {
                                    toggle_microphone_from_shortcut(app_handle_inner, true).await;
                                });
                            }
                            ShortcutState::Released => {
                                tauri::async_runtime::spawn(async move {
                                    toggle_microphone_from_shortcut(app_handle_inner, false).await;
                                });
                            }
                        }
                    },
                ) {
                    log::warn!(
                        "注册麦克风切换快捷键 '{}' 失败: {}",
                        mic_toggle_hot_key_val,
                        e
                    );
                    return Err(frontend_error(
                        ErrorCode::ValidationError,
                        format!("麦克风切换快捷键被占用或注册失败：{}", mic_toggle_hot_key_val),
                        e.to_string(),
                    ));
                }
            }

            settings.recording_mic_toggle_hot_key = mic_toggle_hot_key_val.clone();
        }
    }

    if let Some(enabled) = text_clipboard_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.hot_key.as_str())
            {
                register_text_shortcut(&app, state.inner().clone(), settings.hot_key.as_str())?;
            }
        } else if let Err(e) = app.global_shortcut().unregister(settings.hot_key.as_str()) {
            log::warn!("注销文字快捷键 '{}' 失败: {}", settings.hot_key, e);
        }
    }

    if let Some(enabled) = image_clipboard_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.image_hot_key.as_str())
            {
                register_image_shortcut(
                    &app,
                    state.inner().clone(),
                    settings.image_hot_key.as_str(),
                )?;
            }
        } else if let Err(e) = app
            .global_shortcut()
            .unregister(settings.image_hot_key.as_str())
        {
            log::warn!("注销图片快捷键 '{}' 失败: {}", settings.image_hot_key, e);
        }
    }

    if let Some(enabled) = screenshot_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.screenshot_hot_key.as_str())
            {
                register_screenshot_shortcut(&app, settings.screenshot_hot_key.as_str())?;
            }
        } else if let Err(e) = app
            .global_shortcut()
            .unregister(settings.screenshot_hot_key.as_str())
        {
            log::warn!(
                "注销截图快捷键 '{}' 失败: {}",
                settings.screenshot_hot_key,
                e
            );
        }
    }

    if let Some(enabled) = recording_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.recording_hot_key.as_str())
            {
                register_recording_shortcut(
                    &app,
                    state.inner().clone(),
                    settings.recording_hot_key.as_str(),
                )?;
            }
        } else if let Err(e) = app
            .global_shortcut()
            .unregister(settings.recording_hot_key.as_str())
        {
            log::warn!(
                "注销录屏快捷键 '{}' 失败: {}",
                settings.recording_hot_key,
                e
            );
        }
    }

    if let Some(ref ai_provider_val) = ai_provider {
        if ai_provider_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "提供商名称不能为空",
                "ai_provider is empty",
            ));
        }
        settings.ai_provider = ai_provider_val.clone();

        let mut need_update_config = false;
        let config = settings
            .provider_configs
            .entry(ai_provider_val.clone())
            .or_insert_with(|| {
                need_update_config = true;
                ProviderConfig::default()
            });

        if let Some(ref api_url) = ai_api_url {
            config.api_url = api_url.clone();
        }
        if let Some(ref model_name) = ai_model_name {
            config.model_name = model_name.clone();
        }

        if let Some(ref api_key) = ai_api_key {
            if api_key != "********" {
                settings
                    .save_current_provider_config(api_key)
                    .map_err(|e| frontend_error(ErrorCode::ConfigError, "保存提供商配置失败", e))?;

                if api_key.trim().is_empty() {
                    log::info!("提供商 {} 的API密钥已清空", ai_provider_val);
                } else {
                    match settings.get_provider_api_key(ai_provider_val) {
                        Ok(key) if key == *api_key => {
                            log::info!("密钥保存验证通过");
                        }
                        Ok(_) => {
                            log::warn!("密钥保存验证失败: 读取到的密钥与保存的不一致");
                            return Err(frontend_error(
                                ErrorCode::SystemError,
                                "系统凭据管理器异常: 密钥保存验证失败，请重试",
                                "saved key mismatch",
                            ));
                        }
                        Err(e) => {
                            log::error!("密钥保存验证错误: {}", e);
                            return Err(frontend_error(
                                ErrorCode::SystemError,
                                "系统凭据管理器错误: 无法读取刚保存的密钥",
                                e,
                            ));
                        }
                    }
                }
            }
        }
    }

    settings.migrate_from_old();

    settings
        .validate()
        .map_err(|e| frontend_error(ErrorCode::ValidationError, "设置验证失败", e))?;

    save_settings(&settings)
        .map_err(|e| frontend_error(ErrorCode::ConfigError, "保存设置失败", e))?;
    set_image_fill_verify_mode(&settings.image_fill_verify_mode);

    let selection_enabled = settings.selection_enabled;
    let text_clipboard_feature_enabled = settings.text_clipboard_enabled;
    let image_clipboard_feature_enabled = settings.image_clipboard_enabled;
    let screenshot_feature_enabled = settings.screenshot_enabled;
    let recording_feature_enabled = settings.recording_enabled;
    let (clipboard_manager_arc, image_manager_arc) = {
        let mut state_guard = lock_arc_mutex(state.inner());
        state_guard.settings = settings.clone();
        (
            state_guard.clipboard_manager.clone(),
            state_guard.image_clipboard_manager.clone(),
        )
    };
    {
        let mut manager = lock_arc_mutex(&clipboard_manager_arc);
        if let Some(val) = text_max_items {
            manager.set_max_items(val);
        }
        if let Some(val) = grouped_items_protected_from_limit {
            manager.set_grouped_items_protected_from_limit(val);
        }
    }
    {
        let mut manager = lock_arc_mutex(&image_manager_arc);
        if let Some(val) = image_max_items {
            manager.set_max_items(val);
        }
        if let Some(val) = image_disk_limit_mb {
            manager.set_disk_limit_mb(val);
        }
        if let Some(val) = grouped_items_protected_from_limit {
            manager.set_grouped_items_protected_from_limit(val);
        }
    }

    features::mouse_listener::set_selection_listener_enabled(
        app.clone(),
        state.inner().clone(),
        selection_enabled,
    );
    set_clipboard_listener_enabled(
        app.clone(),
        state.inner().clone(),
        text_clipboard_feature_enabled,
    );
    set_image_clipboard_listener_enabled(
        app.clone(),
        state.inner().clone(),
        image_clipboard_feature_enabled,
    );
    if !screenshot_feature_enabled {
        let _ = close_screenshot_window(app.clone()).await;
    }
    if !recording_feature_enabled {
        let _ = crate::features::recording::recorder_service::cancel_recording(
            &app,
            state.inner().clone(),
            crate::features::recording::types::SessionRequest { session_id: None },
        );
    }
    if let Some(content_protected) = recording_toolbar_content_protected {
        if let Some(window) = app.get_webview_window("recording_toolbar") {
            let _ = window.set_content_protected(content_protected);
        }
    }

    log::info!("设置保存成功（部分更新）");
    Ok(())
}

#[tauri::command]
pub async fn test_ai_connection(
    ai_provider: Option<String>,
    ai_api_url: String,
    ai_model_name: String,
    ai_api_key: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<String, String> {
    let mut real_api_key = ai_api_key;

    // 如果前端传过来的是脱敏的密钥，则从状态中获取真实的密钥
    if real_api_key == "********" {
        let (provider, settings_snapshot) = {
            let state_guard = lock_arc_mutex(state.inner());
            (
                ai_provider.unwrap_or_else(|| state_guard.settings.ai_provider.clone()),
                state_guard.settings.clone(),
            )
        };
        let key = settings_snapshot.get_provider_api_key(&provider);
        match key {
            Ok(key) if !key.is_empty() => {
                real_api_key = key;
            }
            _ => {
                return Err(frontend_error(
                    ErrorCode::ConfigError,
                    "未能获取到真实的 API 密钥",
                    "real api key not found",
                ));
            }
        }
    }

    let config = AIConfig {
        api_key: real_api_key,
        base_url: ai_api_url,
        model: ai_model_name,
    };

    let client = AIClient::new(config)
        .map_err(|e| frontend_error(ErrorCode::NetworkError, "客户端初始化失败", e.to_string()))?;

    match client.test_connection().await {
        Ok(success) => {
            if success {
                Ok("连接成功".to_string())
            } else {
                Err(frontend_error(
                    ErrorCode::NetworkError,
                    "连接测试未返回预期结果",
                    "test_connection returned false",
                ))
            }
        }
        Err(e) => {
            log::error!("AI连接测试失败: {}", e);
            Err(frontend_error(
                ErrorCode::NetworkError,
                "连接测试失败",
                e.to_string(),
            ))
        }
    }
}

#[tauri::command]
pub async fn copy_text(text: String, app: AppHandle) -> Result<(), String> {
    match app.clipboard().write_text(text) {
        Ok(()) => {
            log::info!("文本已复制到剪贴板");
            Ok(())
        }
        Err(e) => {
            let error_msg =
                frontend_error(ErrorCode::ClipboardError, "复制文本失败", e.to_string());
            log::error!("{}", error_msg);
            Err(error_msg)
        }
    }
}

#[tauri::command]
pub async fn copy_and_paste_text(
    text: String,
    request_id: Option<String>,
    app: AppHandle,
) -> Result<WriteBackExecutionResult, String> {
    let started_at = std::time::Instant::now();
    if is_duplicate_copy_paste_request(&text, request_id.as_deref()) {
        if COPY_PASTE_DEDUP_LOG_ENABLED.load(Ordering::Relaxed) {
            log::warn!("检测到短时重复回写请求，已跳过执行");
            COPY_PASTE_DEDUP_LOG_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        return Ok(WriteBackExecutionResult {
            source: "结果窗".to_string(),
            success: true,
            stage: "deduplicated".to_string(),
            target_window_title: String::new(),
            target_window_pid: 0,
            detail: "检测到重复回写请求，已跳过".to_string(),
            operation_id: None,
        });
    }
    let clipboard_started_at = std::time::Instant::now();
    app.clipboard().write_text(text).map_err(|e| {
        let error = frontend_error(ErrorCode::ClipboardError, "复制文本失败", e.to_string());
        record_writeback_stage_metric(
            "结果窗",
            "write_clipboard",
            "结果窗回写写入剪贴板耗时",
            clipboard_started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        record_writeback_stage_metric(
            "结果窗",
            "total",
            "结果窗回写总耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    record_writeback_stage_metric(
        "结果窗",
        "write_clipboard",
        "结果窗回写写入剪贴板耗时",
        clipboard_started_at.elapsed().as_millis() as u64,
        true,
        None,
    );

    let _ = hide_overlay_window_by_label(&app, "result_translation");
    let _ = hide_overlay_window_by_label(&app, "result_explanation");

    emit_writeback_phase(&app, "结果窗", "clipboard_written", None, None);
    emit_writeback_phase(&app, "结果窗", "pasting", None, None);
    let paste_started_at = std::time::Instant::now();
    let app_for_paste = app.clone();
    let paste_result = tauri::async_runtime::spawn_blocking(move || {
        simulate_paste_with_retry(&app_for_paste, "结果窗", None, started_at, false)
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "自动粘贴任务执行失败",
            e.to_string(),
        )
    })?;
    match paste_result {
        Ok(result) => {
            record_writeback_stage_metric(
                "结果窗",
                "paste",
                "结果窗回写粘贴耗时",
                paste_started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            record_writeback_stage_metric(
                "结果窗",
                "total",
                "结果窗回写总耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            emit_writeback_phase(
                &app,
                "结果窗",
                "completed",
                result.operation_id,
                Some(result.detail.clone()),
            );
            emit_writeback_result(&app, &result);
            Ok(result)
        }
        Err(result) => {
            record_writeback_stage_metric(
                "结果窗",
                "paste",
                "结果窗回写粘贴耗时",
                paste_started_at.elapsed().as_millis() as u64,
                false,
                Some(result.detail.clone()),
            );
            record_writeback_stage_metric(
                "结果窗",
                "total",
                "结果窗回写总耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(result.detail.clone()),
            );
            if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                log::warn!("复制后自动粘贴失败时兜底释放Ctrl失败: {}", release_error);
            }
            emit_writeback_phase(
                &app,
                "结果窗",
                "failed",
                result.operation_id,
                Some(result.detail.clone()),
            );
            emit_writeback_result(&app, &result);
            Err(frontend_error(
                ErrorCode::ClipboardError,
                "自动粘贴失败",
                result.detail,
            ))
        }
    }
}

#[tauri::command]
pub async fn get_provider_config(provider: AIProvider) -> Result<(String, String), String> {
    let (url, model) = provider.get_default_config();
    Ok((url, model))
}

#[tauri::command]
pub async fn remove_ai_provider(
    provider: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    if provider.is_empty() {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "提供商名称不能为空",
            "provider is empty",
        ));
    }

    let is_builtin = matches!(provider.as_str(), "deepseek" | "qwen" | "xiaomimimo");
    if is_builtin {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "内置提供商不支持删除",
            provider.clone(),
        ));
    }

    let mut settings = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.settings.clone()
    };

    if settings.provider_configs.remove(&provider).is_none() {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "未找到该提供商配置",
            provider.clone(),
        ));
    }

    if settings.ai_provider == provider {
        let fallback = "deepseek".to_string();
        if settings.provider_configs.contains_key(&fallback) {
            settings.ai_provider = fallback;
        } else if let Some(first_provider) = settings.provider_configs.keys().next() {
            settings.ai_provider = first_provider.clone();
        } else {
            settings.ai_provider = "deepseek".to_string();
        }
    }

    save_settings(&settings)
        .map_err(|e| frontend_error(ErrorCode::ConfigError, "保存设置失败", e))?;

    {
        let mut state_guard = lock_arc_mutex(state.inner());
        state_guard.settings = settings;
    }

    Ok(())
}

/// 获取所有已配置的提供商列表（包括自定义提供商）
#[tauri::command]
pub async fn get_all_configured_providers(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<Vec<(String, String)>, String> {
    let state_guard = lock_arc_mutex(state.inner());
    let settings = &state_guard.settings;

    let mut providers: Vec<(String, String)> = Vec::new();

    for provider_key in settings.provider_configs.keys() {
        providers.push((provider_key.clone(), provider_key.clone()));
    }

    Ok(providers)
}

/// 获取图片预览（优先使用已生成的，否则尝试从异步缓存获取）
#[tauri::command]
pub async fn get_image_preview_by_id(
    item_id: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<Option<(u32, u32, String)>, String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = lock_arc_mutex(&manager_arc);

    match manager.get_image_preview(&item_id) {
        Ok((width, height, base64)) => Ok(Some((width, height, base64))),
        Err(e) if e == "预览正在生成中" => Ok(None),
        Err(e) => Err(e),
    }
}

/// 批量检查预览是否已就绪
#[tauri::command]
pub async fn check_previews_ready(
    item_ids: Vec<String>,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<Vec<(String, bool)>, String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let manager_arc = get_image_clipboard_manager_arc(&state_arc);
        let manager = lock_arc_mutex(&manager_arc);

        let mut results = Vec::new();
        for item_id in item_ids {
            let ready = manager.is_image_preview_ready(&item_id);
            results.push((item_id, ready));
        }

        Ok(results)
    })
    .await
    .map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "检查预览状态任务执行失败",
            e.to_string(),
        )
    })?
}


// ========================================
// 备份系统
// ========================================


#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualLongshotSessionRequest {
    pub session_id: u64,
}
