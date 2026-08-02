use crate::core::app_state::AppState as SharedAppState;
#[cfg(debug_assertions)]
use crate::core::error::to_frontend_error_string;
use crate::core::error_codes::AppErrorKind;
use crate::features;
use crate::services::ai_client::{AIClient, AIConfig};
use crate::services::ai_services::invalidate_ai_client_cache;
use crate::services::clipboard_manager::set_clipboard_listener_enabled;
use crate::services::image_clipboard_manager::set_image_clipboard_listener_enabled;
use crate::sync::{lock_arc_mutex, Mutex};
use crate::ui::commands_clipboard::*;
use crate::ui::commands_recording::{
    toggle_microphone_from_shortcut, toggle_recording_from_shortcut,
};
use crate::ui::commands_screenshot::close_screenshot_window;
use crate::ui::commands_writeback::{emit_writeback_phase, emit_writeback_result, record_writeback_stage_metric, simulate_paste_with_retry, WriteBackExecutionResult};
use crate::ui::tray_menu::open_settings;
use crate::ui::window_manager::{
    bind_overlay_window_events, destroy_window_by_label, ensure_window_for_label,
    hide_overlay_window_by_label, show_overlay_window_by_label, show_standard_window_by_label,
};
#[cfg(debug_assertions)]
use crate::utils::image_clipboard::get_image_persist_queue_metrics_snapshot;
use crate::utils::image_clipboard::set_image_fill_verify_mode;
#[cfg(debug_assertions)]
use crate::utils::utils_helpers::get_dedup_scan_metrics;
use crate::utils::utils_helpers::{
    default_explanation_prompt_template, default_translation_prompt_template, load_settings,
    save_settings,
};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use xxhash_rust::xxh3::xxh3_64;

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
static SETTINGS_SAVE_MUTEX: OnceLock<tauri::async_runtime::Mutex<()>> = OnceLock::new();

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
    crate::utils::utils_helpers::now_unix_ms_u64()
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
    let dir = std::env::temp_dir().join("fuyun_tools").join("screenshot_boot");
    fs::create_dir_all(&dir).map_err(|e| format!("创建截图临时目录失败: {}", e))?;
    Ok(dir.join(format!("screenshot_boot_{}.png", session_id)))
}

pub(crate) fn write_screenshot_boot_image(
    rgba: &[u8],
    width: u32,
    height: u32,
    session_id: u64,
) -> Result<(PathBuf, Vec<u8>), String> {
    let png_data = crate::features::screenshot::capture::rgba_to_png_bytes(rgba, width, height)?;
    let path = build_screenshot_boot_image_path(session_id)?;
    fs::write(&path, &png_data).map_err(|e| format!("写入截图临时文件失败: {}", e))?;
    replace_screenshot_boot_image_path(Some(path.clone()));
    Ok((path, png_data))
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
    if let Err(e) = hide_overlay_window_by_label(&app, "selection_toolbar") {
        log::warn!("隐藏选区工具栏失败: {}", e);
    }
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
        if let Err(e) = settings_window.emit("navigate-settings-tab", payload) {
            log::warn!("发送设置窗口导航事件失败: {}", e);
        }
    }
    Ok(())
}

pub(crate) fn register_recording_shortcut(
    app: &AppHandle,
    _state: Arc<Mutex<SharedAppState>>,
    hot_key: &str,
) -> Result<(), String> {
    let app_clone = app.clone();
    app.global_shortcut()
        .on_shortcut(hot_key, move |_app, _shortcut, event| {
            if let ShortcutState::Pressed = event.state {
                let app_handle_inner = app_clone.clone();
                tauri::async_runtime::spawn(async move {
                    toggle_recording_from_shortcut(app_handle_inner).await;
                });
            }
        })
        .map_err(|e| frontend_error_kind_params(AppErrorKind::ClipboardHotkeyRegisterFailed, serde_json::json!({"key": hot_key}), e.to_string()))?;
    Ok(())
}

/// 注册麦克风切换快捷键（按住开启，松开关闭）
/// 供运行时启用/禁用录屏时与录屏快捷键一同注册/注销
pub(crate) fn register_mic_toggle_shortcut(
    app: &AppHandle,
    _state: Arc<Mutex<SharedAppState>>,
    hot_key: &str,
) -> Result<(), String> {
    let app_clone = app.clone();
    app.global_shortcut()
        .on_shortcut(hot_key, move |_app, _shortcut, event| {
            let app_handle_inner = app_clone.clone();
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
        })
        .map_err(|e| frontend_error_kind_params(AppErrorKind::ClipboardHotkeyRegisterFailed, serde_json::json!({"key": hot_key}), e.to_string()))?;
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
    if let Err(e) = ensure_window_for_label(&app, "selection_toolbar") {
        log::warn!("创建选区工具栏窗口失败: {}", e);
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
        if let Err(e) = toolbar_window.eval(&script) {
            log::warn!("注入选区工具栏文本脚本失败: {}", e);
        }
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
    let scale_factor = monitor.scale_factor();
    let target_width = (source_size.width as i32).min(monitor_size.width as i32);
    let logical_width = target_width as f64 / scale_factor;
    let target_height = (240.0 * scale_factor) as i32;
    let gap = (8.0 * scale_factor) as i32;
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

    if let Err(e) = window.set_size(tauri::LogicalSize::new(
        logical_width.max(1.0),
        240.0,
    )) {
        log::warn!("设置OCR文本窗口大小失败: {}", e);
    }
    if let Err(e) = window.set_always_on_top(true) {
        log::warn!("设置OCR文本窗口置顶失败: {}", e);
    }
    if let Err(e) = window.set_position(tauri::PhysicalPosition::new(target_x, target_y)) {
        log::warn!("设置OCR文本窗口位置失败: {}", e);
    }
    if let Err(e) = show_overlay_window_by_label(&app, &result_label, true) {
        log::warn!("显示OCR文本窗口失败: {}", e);
    }

    let payload = serde_json::json!({"text": content});
    let script = format!(
        "window.__OCR_TEXT_PAYLOAD__ = {payload}; window.dispatchEvent(new CustomEvent('ocr-text-data', {{ detail: {payload} }}));"
    );
    if let Err(e) = window.eval(&script) {
        log::warn!("注入OCR文本数据脚本失败: {}", e);
    }
    Ok(())
}

#[tauri::command]
pub async fn get_ai_settings(state: State<'_, Arc<Mutex<SharedAppState>>>) -> Result<serde_json::Value, String> {
    let settings = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.settings.clone()
    };
    let mut settings_json = serde_json::to_value(&settings)
        .map_err(|e| frontend_error_kind(AppErrorKind::JsonError, e.to_string()))?;

    let providers = crate::utils::ai_store::get_all_providers().await;
    let mut provider_configs_map = serde_json::Map::new();
    for (key, cfg) in &providers {
        provider_configs_map.insert(key.clone(), serde_json::json!({
            "api_url": cfg.api_url,
            "model_name": cfg.model_name,
            "api_key": cfg.api_key,
        }));
    }

    if let Some(obj) = settings_json.as_object_mut() {
        obj.insert("ai_provider".to_string(), serde_json::Value::String(crate::utils::ai_store::get_current_provider().await));
        obj.insert("provider_configs".to_string(), serde_json::Value::Object(provider_configs_map));
    }

    Ok(settings_json)
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_text_dedup_metrics() -> Result<serde_json::Value, String> {
    serde_json::to_value(get_dedup_scan_metrics())
        .map_err(|e| frontend_error_kind(AppErrorKind::InternalError, e.to_string()))
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
            AppErrorKind::InternalError.to_app_error()
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
            AppErrorKind::InternalError.to_app_error()
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

fn effective_key(new: &Option<String>, fallback: &str) -> String {
    match new {
        Some(val) => val.clone(),
        None => fallback.to_string(),
    }
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
    launcher_hot_key: Option<String>,
    text_clipboard_enabled: Option<bool>,
    image_clipboard_enabled: Option<bool>,
    screenshot_enabled: Option<bool>,
    recording_enabled: Option<bool>,
    launcher_enabled: Option<bool>,
    doc_manager_hot_key: Option<String>,
    doc_manager_enabled: Option<bool>,
    doc_manager_widget_enabled: Option<bool>,
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
    recording_system_audio_device_id: Option<String>,
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

    // 防止并发保存设置导致数据丢失
    let _settings_guard = SETTINGS_SAVE_MUTEX
        .get_or_init(|| tauri::async_runtime::Mutex::new(()))
        .lock()
        .await;

    // 注意：此处克隆后释放锁，修改期间（~800行）并发命令读取到的是旧值。
    // 这是一个已知的设计限制，完整修复需要重构为 write-through 模式。
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
    if let Some(val) = launcher_enabled {
        settings.launcher_enabled = val;
    }
    if let Some(val) = doc_manager_enabled {
        settings.doc_manager_enabled = val;
    }

    if let Some(enabled) = doc_manager_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.doc_manager_hot_key.as_str())
            {
                let app_handle_for_doc = app.clone();
                if let Err(e) = app.global_shortcut().on_shortcut(
                    settings.doc_manager_hot_key.as_str(),
                    move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let _ = crate::ui::window_manager::show_standard_window_by_label(
                                &app_handle_for_doc,
                                "document_manager",
                            );
                        }
                    },
                ) {
                    log::warn!(
                        "注册文档管理快捷键 '{}' 失败: {}",
                        settings.doc_manager_hot_key,
                        e
                    );
                }
            }
        } else if let Err(e) = app
            .global_shortcut()
            .unregister(settings.doc_manager_hot_key.as_str())
        {
            log::warn!(
                "注销文档管理快捷键 '{}' 失败: {}",
                settings.doc_manager_hot_key,
                e
            );
        }
    }
    if let Some(val) = doc_manager_widget_enabled {
        let old = settings.doc_manager_widget_enabled;
        settings.doc_manager_widget_enabled = val;
        if val && !old {
            let _ = crate::ui::window_manager::show_doc_manager_widget_window(&app);
        } else if !val && old {
            let _ = crate::ui::window_manager::hide_doc_manager_widget_window(&app);
        }
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
    if let Some(val) = recording_system_audio_device_id {
        settings.recording_system_audio_device_id = val.trim().to_string();
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
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "hot_key is empty",
            ));
        }

        if hot_key_val != &settings.hot_key {
            if settings.text_clipboard_enabled
                && app.global_shortcut().is_registered(hot_key_val.as_str())
            {
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": hot_key_val}),
                    "hot_key global shortcut already registered",
                ));
            }
            let old_hot_key = settings.hot_key.clone();
            if let Err(e) = app.global_shortcut().unregister(old_hot_key.as_str()) {
                log::warn!(
                    "注销旧文字窗口快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_hot_key,
                    e
                );
            }
            if settings.text_clipboard_enabled {
                if let Err(e) = register_text_shortcut(&app, state.inner().clone(), hot_key_val.as_str()) {
                    log::warn!("注册新文字窗口快捷键 '{}' 失败, 尝试恢复旧快捷键: {}", hot_key_val, e);
                    if let Err(e2) = register_text_shortcut(&app, state.inner().clone(), old_hot_key.as_str()) {
                        log::error!("恢复旧文字窗口快捷键 '{}' 也失败: {}", old_hot_key, e2);
                    }
                    return Err(e);
                }
            }
            settings.hot_key = hot_key_val.clone();
        }
    }

    if let Some(ref image_hot_key_val) = image_hot_key {
        if image_hot_key_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "image_hot_key is empty",
            ));
        }

        if image_hot_key_val != &settings.image_hot_key {
            if let Some(ref hot_key_val) = hot_key {
                if image_hot_key_val == hot_key_val {
                    return Err(frontend_error_kind(
                        AppErrorKind::SettingsHotkeysIdentical,
                        format!(
                            "hot_key={}, image_hot_key={}",
                            hot_key_val, image_hot_key_val
                        ),
                    ));
                }
            } else if image_hot_key_val == &settings.hot_key {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsHotkeysIdentical,
                    format!(
                        "hot_key={}, image_hot_key={}",
                        settings.hot_key, image_hot_key_val
                    ),
                ));
            }

            if settings.image_clipboard_enabled
                && app.global_shortcut().is_registered(image_hot_key_val.as_str())
            {
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": image_hot_key_val}),
                    "image_hot_key global shortcut already registered",
                ));
            }
            let old_image_hot_key = settings.image_hot_key.clone();
            if let Err(e) = app.global_shortcut().unregister(old_image_hot_key.as_str()) {
                log::warn!(
                    "注销旧图片窗口快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_image_hot_key,
                    e
                );
            }
            if settings.image_clipboard_enabled {
                if let Err(e) = register_image_shortcut(&app, state.inner().clone(), image_hot_key_val.as_str()) {
                    log::warn!("注册新图片窗口快捷键 '{}' 失败, 尝试恢复旧快捷键: {}", image_hot_key_val, e);
                    if let Err(e2) = register_image_shortcut(&app, state.inner().clone(), old_image_hot_key.as_str()) {
                        log::error!("恢复旧图片窗口快捷键 '{}' 也失败: {}", old_image_hot_key, e2);
                    }
                    return Err(e);
                }
            }
            settings.image_hot_key = image_hot_key_val.clone();
        }
    }

    if let Some(ref screenshot_hot_key_val) = screenshot_hot_key {
        if screenshot_hot_key_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "screenshot_hot_key is empty",
            ));
        }

        if screenshot_hot_key_val != &settings.screenshot_hot_key {
            let effective_hot_key = effective_key(&hot_key, &settings.hot_key);
            let effective_image_hot_key = effective_key(&image_hot_key, &settings.image_hot_key);
            if screenshot_hot_key_val == &effective_hot_key
                || screenshot_hot_key_val == &effective_image_hot_key
            {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsHotkeysIdentical,
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
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": screenshot_hot_key_val}),
                    "screenshot global shortcut already registered",
                ));
            }

            let old_screenshot_hot_key = settings.screenshot_hot_key.clone();
            if let Err(e) = app
                .global_shortcut()
                .unregister(old_screenshot_hot_key.as_str())
            {
                log::warn!(
                    "注销旧截图快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_screenshot_hot_key,
                    e
                );
            }
            if settings.screenshot_enabled {
                if let Err(e) = register_screenshot_shortcut(&app, screenshot_hot_key_val.as_str()) {
                    log::warn!("注册新截图快捷键 '{}' 失败, 尝试恢复旧快捷键: {}", screenshot_hot_key_val, e);
                    if let Err(e2) = register_screenshot_shortcut(&app, old_screenshot_hot_key.as_str()) {
                        log::error!("恢复旧截图快捷键 '{}' 也失败: {}", old_screenshot_hot_key, e2);
                    }
                    return Err(e);
                }
            }
            settings.screenshot_hot_key = screenshot_hot_key_val.clone();
        }
    }

    if let Some(ref recording_hot_key_val) = recording_hot_key {
        if recording_hot_key_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "recording_hot_key is empty",
            ));
        }
        if recording_hot_key_val != &settings.recording_hot_key {
            let effective_hot_key = effective_key(&hot_key, &settings.hot_key);
            let effective_image_hot_key = effective_key(&image_hot_key, &settings.image_hot_key);
            let effective_screenshot_hot_key = effective_key(&screenshot_hot_key, &settings.screenshot_hot_key);
            if recording_hot_key_val == &effective_hot_key
                || recording_hot_key_val == &effective_image_hot_key
                || recording_hot_key_val == &effective_screenshot_hot_key
            {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsHotkeysIdentical,
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
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": recording_hot_key_val}),
                    "recording global shortcut already registered",
                ));
            }

            let old_recording_hot_key = settings.recording_hot_key.clone();
            if let Err(e) = app
                .global_shortcut()
                .unregister(old_recording_hot_key.as_str())
            {
                log::warn!(
                    "注销旧录屏快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_recording_hot_key,
                    e
                );
            }
            if settings.recording_enabled {
                if let Err(e) = register_recording_shortcut(
                    &app,
                    state.inner().clone(),
                    recording_hot_key_val.as_str(),
                ) {
                    log::warn!("注册新录屏快捷键 '{}' 失败, 尝试恢复旧快捷键: {}", recording_hot_key_val, e);
                    if let Err(e2) = register_recording_shortcut(&app, state.inner().clone(), old_recording_hot_key.as_str()) {
                        log::error!("恢复旧录屏快捷键 '{}' 也失败: {}", old_recording_hot_key, e2);
                    }
                    return Err(e);
                }
            }
            settings.recording_hot_key = recording_hot_key_val.clone();
        }
    }

    if let Some(ref mic_toggle_hot_key_val) = recording_mic_toggle_hot_key {
        if mic_toggle_hot_key_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "recording_mic_toggle_hot_key is empty",
            ));
        }
        if mic_toggle_hot_key_val != &settings.recording_mic_toggle_hot_key {
            let effective_hot_key = effective_key(&hot_key, &settings.hot_key);
            let effective_image_hot_key = effective_key(&image_hot_key, &settings.image_hot_key);
            let effective_screenshot_hot_key = effective_key(&screenshot_hot_key, &settings.screenshot_hot_key);
            let effective_recording_hot_key = effective_key(&recording_hot_key, &settings.recording_hot_key);

            if mic_toggle_hot_key_val == &effective_hot_key
                || mic_toggle_hot_key_val == &effective_image_hot_key
                || mic_toggle_hot_key_val == &effective_screenshot_hot_key
                || mic_toggle_hot_key_val == &effective_recording_hot_key
            {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsHotkeysIdentical,
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
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": mic_toggle_hot_key_val}),
                    "mic toggle global shortcut already registered",
                ));
            }

            let old_mic_toggle_hot_key = settings.recording_mic_toggle_hot_key.clone();
            if let Err(e) = app
                .global_shortcut()
                .unregister(old_mic_toggle_hot_key.as_str())
            {
                log::warn!(
                    "注销旧麦克风切换快捷键 '{}' 失败: {}",
                    old_mic_toggle_hot_key,
                    e
                );
            }

            if settings.recording_enabled {
                let app_handle_for_mic = app.clone();
                let old_mic_key_for_rollback = old_mic_toggle_hot_key.clone();
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
                        "注册麦克风切换快捷键 '{}' 失败, 尝试恢复旧快捷键: {}",
                        mic_toggle_hot_key_val,
                        e
                    );
                    let app_handle_for_rollback = app.clone();
                    if let Err(e2) = app.global_shortcut().on_shortcut(
                        old_mic_key_for_rollback.as_str(),
                        move |_app, _shortcut, event| {
                            let app_handle_inner = app_handle_for_rollback.clone();
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
                        log::error!("恢复旧麦克风切换快捷键 '{}' 也失败: {}", old_mic_key_for_rollback, e2);
                    }
                    return Err(frontend_error_kind_params(
                        AppErrorKind::ClipboardHotkeyRegisterFailed,
                        serde_json::json!({"key": mic_toggle_hot_key_val}),
                        e.to_string(),
                    ));
                }
            }

            settings.recording_mic_toggle_hot_key = mic_toggle_hot_key_val.clone();
        }
    }

    if let Some(ref launcher_hot_key_val) = launcher_hot_key {
        if launcher_hot_key_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "launcher_hot_key is empty",
            ));
        }

        if launcher_hot_key_val != &settings.launcher_hot_key {
            let effective_hot_key = effective_key(&hot_key, &settings.hot_key);
            let effective_image_hot_key = effective_key(&image_hot_key, &settings.image_hot_key);
            let effective_screenshot_hot_key = effective_key(&screenshot_hot_key, &settings.screenshot_hot_key);
            let effective_recording_hot_key = effective_key(&recording_hot_key, &settings.recording_hot_key);
            let effective_mic_toggle_hot_key = effective_key(&recording_mic_toggle_hot_key, &settings.recording_mic_toggle_hot_key);

            if launcher_hot_key_val == &effective_hot_key
                || launcher_hot_key_val == &effective_image_hot_key
                || launcher_hot_key_val == &effective_screenshot_hot_key
                || launcher_hot_key_val == &effective_recording_hot_key
                || launcher_hot_key_val == &effective_mic_toggle_hot_key
            {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsHotkeysIdentical,
                    format!(
                        "hot_key={}, image_hot_key={}, screenshot_hot_key={}, recording_hot_key={}, mic_toggle_hot_key={}, launcher_hot_key={}",
                        effective_hot_key,
                        effective_image_hot_key,
                        effective_screenshot_hot_key,
                        effective_recording_hot_key,
                        effective_mic_toggle_hot_key,
                        launcher_hot_key_val
                    ),
                ));
            }

            if app
                .global_shortcut()
                .is_registered(launcher_hot_key_val.as_str())
            {
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": launcher_hot_key_val}),
                    "launcher global shortcut already registered",
                ));
            }

            let old_launcher_hot_key = settings.launcher_hot_key.clone();
            if let Err(e) = app
                .global_shortcut()
                .unregister(old_launcher_hot_key.as_str())
            {
                log::warn!(
                    "注销旧启动器快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_launcher_hot_key,
                    e
                );
            }
            if settings.launcher_enabled {
                let app_handle_for_launcher = app.clone();
                let old_launcher_key_for_rollback = old_launcher_hot_key.clone();
                if let Err(e) = app.global_shortcut().on_shortcut(
                    launcher_hot_key_val.as_str(),
                    move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let app_handle = app_handle_for_launcher.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = crate::ui::commands_launcher::show_launcher(app_handle).await;
                            });
                        }
                    },
                ) {
                    log::warn!(
                        "注册启动器快捷键 '{}' 失败, 尝试恢复旧快捷键: {}",
                        launcher_hot_key_val,
                        e
                    );
                    let app_handle_for_rollback = app.clone();
                    if let Err(e2) = app.global_shortcut().on_shortcut(
                        old_launcher_key_for_rollback.as_str(),
                        move |_app, _shortcut, event| {
                            if let ShortcutState::Pressed = event.state {
                                let app_handle = app_handle_for_rollback.clone();
                                tauri::async_runtime::spawn(async move {
                                    let _ = crate::ui::commands_launcher::show_launcher(app_handle).await;
                                });
                            }
                        },
                    ) {
                        log::error!("恢复旧启动器快捷键 '{}' 也失败: {}", old_launcher_key_for_rollback, e2);
                    }
                    return Err(frontend_error_kind_params(
                        AppErrorKind::ClipboardHotkeyRegisterFailed,
                        serde_json::json!({"key": launcher_hot_key_val}),
                        e.to_string(),
                    ));
                }
            }
            settings.launcher_hot_key = launcher_hot_key_val.clone();
        }
    }

    if let Some(ref doc_manager_hot_key_val) = doc_manager_hot_key {
        if doc_manager_hot_key_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "doc_manager_hot_key is empty",
            ));
        }

        if doc_manager_hot_key_val != &settings.doc_manager_hot_key {
            let effective_hot_key = effective_key(&hot_key, &settings.hot_key);
            let effective_image_hot_key = effective_key(&image_hot_key, &settings.image_hot_key);
            let effective_screenshot_hot_key = effective_key(&screenshot_hot_key, &settings.screenshot_hot_key);
            let effective_recording_hot_key = effective_key(&recording_hot_key, &settings.recording_hot_key);
            let effective_mic_toggle_hot_key = effective_key(&recording_mic_toggle_hot_key, &settings.recording_mic_toggle_hot_key);
            let effective_launcher_hot_key = effective_key(&launcher_hot_key, &settings.launcher_hot_key);

            if doc_manager_hot_key_val == &effective_hot_key
                || doc_manager_hot_key_val == &effective_image_hot_key
                || doc_manager_hot_key_val == &effective_screenshot_hot_key
                || doc_manager_hot_key_val == &effective_recording_hot_key
                || doc_manager_hot_key_val == &effective_mic_toggle_hot_key
                || doc_manager_hot_key_val == &effective_launcher_hot_key
            {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsHotkeysIdentical,
                    format!("doc_manager_hot_key={} conflicts with existing shortcut", doc_manager_hot_key_val),
                ));
            }

            if app.global_shortcut().is_registered(doc_manager_hot_key_val.as_str()) {
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": doc_manager_hot_key_val}),
                    "doc_manager global shortcut already registered",
                ));
            }

            let old_doc_manager_hot_key = settings.doc_manager_hot_key.clone();
            if let Err(e) = app.global_shortcut().unregister(old_doc_manager_hot_key.as_str()) {
                log::warn!("注销旧文档管理快捷键 '{}' 失败: {}", old_doc_manager_hot_key, e);
            }
            if settings.doc_manager_enabled {
                let app_handle_for_doc = app.clone();
                let old_doc_key_for_rollback = old_doc_manager_hot_key.clone();
                if let Err(e) = app.global_shortcut().on_shortcut(
                    doc_manager_hot_key_val.as_str(),
                    move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let _ = crate::ui::window_manager::show_standard_window_by_label(&app_handle_for_doc, "document_manager");
                        }
                    },
                ) {
                    log::warn!("注册文档管理快捷键 '{}' 失败, 尝试恢复旧快捷键: {}", doc_manager_hot_key_val, e);
                    let app_handle_for_rollback = app.clone();
                    if let Err(e2) = app.global_shortcut().on_shortcut(
                        old_doc_key_for_rollback.as_str(),
                        move |_app, _shortcut, event| {
                            if let ShortcutState::Pressed = event.state {
                                let _ = crate::ui::window_manager::show_standard_window_by_label(&app_handle_for_rollback, "document_manager");
                            }
                        },
                    ) {
                        log::error!("恢复旧文档管理快捷键 '{}' 也失败: {}", old_doc_key_for_rollback, e2);
                    }
                    return Err(frontend_error_kind_params(
                        AppErrorKind::ClipboardHotkeyRegisterFailed,
                        serde_json::json!({"key": doc_manager_hot_key_val}),
                        e.to_string(),
                    ));
                }
            }
            settings.doc_manager_hot_key = doc_manager_hot_key_val.clone();
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
        } else {
            if let Err(e) = app
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
            // 麦克风切换快捷键同样需要随启用即时注册（否则重启前不生效）
            if !app
                .global_shortcut()
                .is_registered(settings.recording_mic_toggle_hot_key.as_str())
            {
                register_mic_toggle_shortcut(
                    &app,
                    state.inner().clone(),
                    settings.recording_mic_toggle_hot_key.as_str(),
                )?;
            }
        } else {
            if let Err(e) = app
                .global_shortcut()
                .unregister(settings.recording_hot_key.as_str())
            {
                log::warn!(
                    "注销录屏快捷键 '{}' 失败: {}",
                    settings.recording_hot_key,
                    e
                );
            }
            if app
                .global_shortcut()
                .is_registered(settings.recording_mic_toggle_hot_key.as_str())
            {
                if let Err(e) = app
                    .global_shortcut()
                    .unregister(settings.recording_mic_toggle_hot_key.as_str())
                {
                    log::warn!(
                        "注销麦克风切换快捷键 '{}' 失败: {}",
                        settings.recording_mic_toggle_hot_key,
                        e
                    );
                }
            }
        }
    }

    if let Some(enabled) = launcher_enabled {
        if enabled {
            // 检查是否有快捷键变更
            if let Some(ref launcher_hot_key_val) = launcher_hot_key {
                if launcher_hot_key_val != &settings.launcher_hot_key {
                    // 快捷键发生变更，先注销旧的
                    if let Err(e) = app
                        .global_shortcut()
                        .unregister(settings.launcher_hot_key.as_str())
                    {
                        log::warn!(
                            "注销旧启动器快捷键 '{}' 失败 (可能从未注册成功): {}",
                            settings.launcher_hot_key,
                            e
                        );
                    }

                    // 注册新的快捷键
                    let app_handle_for_launcher = app.clone();
                    let new_hot_key = launcher_hot_key_val.clone();
                    if let Err(e) = app.global_shortcut().on_shortcut(
                        launcher_hot_key_val.as_str(),
                        move |_app, _shortcut, event| {
                            if let ShortcutState::Pressed = event.state {
                                let app_handle = app_handle_for_launcher.clone();
                                tauri::async_runtime::spawn(async move {
                                    let _ = crate::ui::commands_launcher::show_launcher(app_handle).await;
                                });
                            }
                        },
                    ) {
                        log::warn!(
                            "注册启动器快捷键 '{}' 失败: {}",
                            new_hot_key,
                            e
                        );
                    }
                    settings.launcher_hot_key = launcher_hot_key_val.clone();
                } else if !app
                    .global_shortcut()
                    .is_registered(settings.launcher_hot_key.as_str())
                {
                    // 快捷键未变更但未注册，重新注册
                    let app_handle_for_launcher = app.clone();
                    if let Err(e) = app.global_shortcut().on_shortcut(
                        settings.launcher_hot_key.as_str(),
                        move |_app, _shortcut, event| {
                            if let ShortcutState::Pressed = event.state {
                                let app_handle = app_handle_for_launcher.clone();
                                tauri::async_runtime::spawn(async move {
                                    let _ = crate::ui::commands_launcher::show_launcher(app_handle).await;
                                });
                            }
                        },
                    ) {
                        log::warn!(
                            "注册启动器快捷键 '{}' 失败: {}",
                            settings.launcher_hot_key.as_str(),
                            e
                        );
                    }
                }
            } else if !app
                .global_shortcut()
                .is_registered(settings.launcher_hot_key.as_str())
            {
                // 没有传入新快捷键，但需要确保已注册
                let app_handle_for_launcher = app.clone();
                if let Err(e) = app.global_shortcut().on_shortcut(
                    settings.launcher_hot_key.as_str(),
                    move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let app_handle = app_handle_for_launcher.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = crate::ui::commands_launcher::show_launcher(app_handle).await;
                            });
                        }
                    },
                ) {
                    log::warn!(
                        "注册启动器快捷键 '{}' 失败: {}",
                        settings.launcher_hot_key.as_str(),
                        e
                    );
                }
            }
        } else if let Err(e) = app
            .global_shortcut()
            .unregister(settings.launcher_hot_key.as_str())
        {
            log::warn!(
                "注销启动器快捷键 '{}' 失败: {}",
                settings.launcher_hot_key,
                e
            );
        }
    }

    if let Some(ref ai_provider_val) = ai_provider {
        if ai_provider_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsProviderNameEmpty,
                "ai_provider is empty",
            ));
        }
        crate::utils::ai_store::set_current_provider(ai_provider_val).await
            .map_err(|e| frontend_error_kind(AppErrorKind::SettingsSaveFailed, e))?;
    }

    // 解析当前要操作的提供商
    let provider_key = match ai_provider {
        Some(ref p) if !p.is_empty() => p.clone(),
        _ => crate::utils::ai_store::get_current_provider().await,
    };
    if !provider_key.is_empty() {
        let existing = crate::utils::ai_store::get_provider_config(&provider_key).await;
        let is_new = existing.is_none();
        let mut config = existing.unwrap_or_default();
        let mut changed = is_new;

        if let Some(ref api_url) = ai_api_url {
            if !api_url.is_empty() && config.api_url != *api_url {
                config.api_url = api_url.clone();
                changed = true;
            }
        }
        if let Some(ref model_name) = ai_model_name {
            if !model_name.is_empty() && config.model_name != *model_name {
                config.model_name = model_name.clone();
                changed = true;
            }
        }
        if let Some(ref api_key) = ai_api_key {
            if api_key != "********" && config.api_key != *api_key {
                config.api_key = api_key.clone();
                changed = true;
                if api_key.trim().is_empty() {
                    log::info!("提供商 {} 的API密钥已清空", provider_key);
                }
            }
        }

        if changed {
            crate::utils::ai_store::save_provider_config(&provider_key, &config.api_url, &config.model_name, &config.api_key).await
                .map_err(|e| frontend_error_kind(AppErrorKind::SettingsSaveProviderFailed, e))?;
        }
    }

    invalidate_ai_client_cache();

    settings
        .validate()
        .map_err(|e| frontend_error_kind(AppErrorKind::SettingsValidationFailed, e))?;

    save_settings(&settings)
        .map_err(|e| frontend_error_kind(AppErrorKind::SettingsSaveFailed, e))?;
    set_image_fill_verify_mode(&settings.image_fill_verify_mode);

    let selection_enabled = settings.selection_enabled;
    let text_clipboard_feature_enabled = settings.text_clipboard_enabled;
    let image_clipboard_feature_enabled = settings.image_clipboard_enabled;
    let screenshot_feature_enabled = settings.screenshot_enabled;
    let recording_feature_enabled = settings.recording_enabled;
    let launcher_feature_enabled = settings.launcher_enabled;
    let doc_manager_feature_enabled = settings.doc_manager_enabled;
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
    if screenshot_feature_enabled {
        let _ = ensure_window_for_label(&app, "screenshot");
        let _ = ensure_window_for_label(&app, "longshot_toolbar");
        let _ = ensure_window_for_label(&app, "longshot_border");
    } else {
        let _ = close_screenshot_window(app.clone()).await;
        destroy_window_by_label(&app, "screenshot");
        destroy_window_by_label(&app, "longshot_toolbar");
        destroy_window_by_label(&app, "longshot_border");
    }
    if recording_feature_enabled {
        let _ = ensure_window_for_label(&app, "recording_toolbar");
    } else {
        let _ = crate::features::recording::recorder_service::cancel_recording(
            &app,
            state.inner().clone(),
            crate::features::recording::types::SessionRequest { session_id: None },
        );
        destroy_window_by_label(&app, "recording_toolbar");
    }
    if text_clipboard_feature_enabled {
        let _ = ensure_window_for_label(&app, "clipboard");
        let _ = ensure_window_for_label(&app, "text_preview");
    } else {
        log::info!("[设置] 文本剪切板已禁用，开始销毁窗口");
        destroy_window_by_label(&app, "clipboard");
        destroy_window_by_label(&app, "text_preview");
    }
    if image_clipboard_feature_enabled {
        let _ = ensure_window_for_label(&app, "image_clipboard");
        let _ = ensure_window_for_label(&app, "image_preview");
    } else {
        log::info!("[设置] 图片剪切板已禁用，开始销毁窗口");
        destroy_window_by_label(&app, "image_clipboard");
        destroy_window_by_label(&app, "image_preview");
    }
    if selection_enabled {
        let _ = ensure_window_for_label(&app, "selection_toolbar");
    } else {
        destroy_window_by_label(&app, "selection_toolbar");
    }
    if launcher_feature_enabled {
        let _ = ensure_window_for_label(&app, "launcher");
    } else {
        destroy_window_by_label(&app, "launcher");
    }
    if doc_manager_feature_enabled {
        let _ = ensure_window_for_label(&app, "document_manager");
    } else {
        destroy_window_by_label(&app, "document_manager");
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
) -> Result<String, String> {
    let mut real_api_key = ai_api_key;

    if real_api_key == "********" {
        let provider = match ai_provider {
            Some(p) if !p.is_empty() => p,
            _ => crate::utils::ai_store::get_current_provider().await,
        };
        match crate::utils::ai_store::get_api_key(&provider).await {
            Ok(key) if !key.is_empty() => real_api_key = key,
            _ => return Err(frontend_error_kind(AppErrorKind::SettingsLocalKeyNotFound, "real api key not found")),
        }
    }

    let config = AIConfig {
        api_key: real_api_key,
        base_url: ai_api_url,
        model: ai_model_name,
    };

    let client = AIClient::new(config)
        .map_err(|e| frontend_error_kind(AppErrorKind::AiClientInitFailed, e.to_string()))?;

    match client.test_connection().await {
        Ok(success) => {
            if success {
                Ok("连接成功".to_string())
            } else {
                Err(frontend_error_kind(
                    AppErrorKind::AiConnectionTestNoResponse,
                    "test_connection returned false",
                ))
            }
        }
        Err(e) => {
            log::error!("AI连接测试失败: {}", e);
            Err(frontend_error_kind(
                AppErrorKind::AiConnectionTestFailed,
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
                frontend_error_kind(AppErrorKind::ClipboardCopyTextFailed, e.to_string());
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
        let error = frontend_error_kind(AppErrorKind::ClipboardCopyTextFailed, e.to_string());
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

    emit_writeback_phase(&app, "结果窗", "clipboard_written", None, None);
    emit_writeback_phase(&app, "结果窗", "pasting", None, None);
    let paste_started_at = std::time::Instant::now();
    let app_for_paste = app.clone();
    let paste_result = tauri::async_runtime::spawn_blocking(move || {
        simulate_paste_with_retry(&app_for_paste, "结果窗", None, started_at, false)
    })
    .await
    .map_err(|e| {
        frontend_error_kind(
            AppErrorKind::TaskExecutionFailed,
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
            Err(frontend_error_kind(
                AppErrorKind::ClipboardAutoPasteFailed,
                result.detail,
            ))
        }
    }
}

#[tauri::command]
pub async fn remove_ai_provider(
    provider: String,
    _state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    if provider.is_empty() {
        return Err(frontend_error_kind(AppErrorKind::SettingsProviderNameEmpty, "provider is empty"));
    }

    crate::utils::ai_store::remove_provider(&provider).await
        .map_err(|e| frontend_error_kind(AppErrorKind::AiProviderNotFound, e))?;

    let current = crate::utils::ai_store::get_current_provider().await;
    if current == provider {
        crate::utils::ai_store::set_current_provider("").await.ok();
    }

    invalidate_ai_client_cache();
    Ok(())
}

#[tauri::command]
pub async fn get_all_configured_providers() -> Result<Vec<(String, String)>, String> {
    let providers = crate::utils::ai_store::get_all_providers().await;
    Ok(providers.into_iter().map(|(k, _)| (k.clone(), k)).collect())
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
        frontend_error_kind(
            AppErrorKind::TaskExecutionFailed,
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

/// 显示标准窗口（设置、启动器等）
#[tauri::command]
pub async fn show_standard_window_command(app: AppHandle, label: String) -> Result<(), String> {
    show_standard_window_by_label(&app, &label)
}

/// 显示剪贴板窗口
#[tauri::command]
pub async fn show_clipboard_window_command(app: AppHandle, state: State<'_, Arc<Mutex<SharedAppState>>>) -> Result<(), String> {
    let state_arc = state.inner().clone();
    crate::ui::window_manager::show_clipboard_window(app, state_arc);
    Ok(())
}

/// 开始截图
#[tauri::command]
pub async fn start_screenshot_command(state: State<'_, Arc<Mutex<SharedAppState>>>) -> Result<(), String> {
    let _ = crate::ui::commands_screenshot::start_screenshot(state).await;
    Ok(())
}

/// 切换录屏状态
#[tauri::command]
pub async fn toggle_recording_command(app: AppHandle, _state: State<'_, Arc<Mutex<SharedAppState>>>) -> Result<(), String> {
    use crate::ui::commands_recording::toggle_recording_from_shortcut;
    toggle_recording_from_shortcut(app).await;
    Ok(())
}

// ========================================
// 性能监控命令
// ========================================

/// 获取系统资源使用情况（内存、CPU）
#[tauri::command]
pub async fn get_system_resources() -> Result<crate::core::perf_metrics::SystemResourceSnapshot, String> {
    Ok(crate::core::perf_metrics::get_system_resources())
}

/// 获取性能指标摘要
#[tauri::command]
pub async fn get_perf_summary() -> Result<crate::core::perf_metrics::PerfSummary, String> {
    Ok(crate::core::perf_metrics::get_perf_summary())
}

/// 按类别获取性能指标
#[tauri::command]
pub async fn get_metrics_by_category() -> Result<std::collections::BTreeMap<String, Vec<crate::core::perf_metrics::PerfMetricSnapshot>>, String> {
    Ok(crate::core::perf_metrics::get_metrics_by_category())
}

/// 获取启动相关指标
#[tauri::command]
pub async fn get_startup_metrics() -> Result<Vec<crate::core::perf_metrics::PerfMetricSnapshot>, String> {
    Ok(crate::core::perf_metrics::get_startup_metrics())
}

/// 获取内存相关指标
#[tauri::command]
pub async fn get_memory_metrics() -> Result<Vec<crate::core::perf_metrics::PerfMetricSnapshot>, String> {
    Ok(crate::core::perf_metrics::get_memory_metrics())
}

/// 获取IPC延迟指标
#[tauri::command]
pub async fn get_ipc_metrics() -> Result<Vec<crate::core::perf_metrics::PerfMetricSnapshot>, String> {
    Ok(crate::core::perf_metrics::get_ipc_metrics())
}

/// 获取主题
#[tauri::command]
pub async fn get_theme() -> Result<String, String> {
    let settings = load_settings().map_err(|e| e.to_string())?;
    Ok(settings.theme)
}

/// 设置主题
#[tauri::command]
pub async fn set_theme(theme: String) -> Result<(), String> {
    let mut settings = load_settings().map_err(|e| e.to_string())?;
    settings.theme = theme;
    save_settings(&settings).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn notify_result_window_ready(_window_label: String) -> Result<(), String> {
    Ok(())
}
