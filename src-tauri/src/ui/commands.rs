use crate::core::app_state::AppState as SharedAppState;
use crate::core::config::{AIProvider, ProviderConfig};
use crate::core::error::{to_frontend_error_string, AppError, AppResult, ErrorCode};
use crate::features;
use crate::services::ai_client::{AIClient, AIConfig};
use crate::services::clipboard_manager::set_clipboard_listener_enabled;
use crate::services::image_clipboard_manager::{
    emit_image_history_payload, set_image_clipboard_listener_enabled,
};
use crate::sync::Mutex;
use crate::ui::commands_recording::toggle_recording_from_shortcut;
use crate::ui::tray_menu::open_settings;
use crate::ui::window_manager::{
    hide_clipboard_window, hide_image_clipboard_window, hide_image_preview_window, set_window_position,
    show_clipboard_window, show_image_clipboard_window, show_image_preview_loading_window,
    show_image_preview_window,
};
use crate::utils::clipboard::ClipboardManager;
#[cfg(debug_assertions)]
use crate::utils::image_clipboard::get_image_persist_queue_metrics_snapshot;
use crate::utils::image_clipboard::{
    is_fast_fill_verify_mode_enabled, set_image_fill_verify_mode, ImageClipboardManager,
    ImageHistoryPageData, ImageHistoryPreviewItem,
};
#[cfg(debug_assertions)]
use crate::utils::utils_helpers::get_dedup_scan_metrics;
use crate::utils::utils_helpers::{
    default_explanation_prompt_template, default_translation_prompt_template,
    load_history_page_data_async, load_settings, save_settings, ClipboardHistoryPageData,
};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::sync::{Arc, Mutex as StdMutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_positioner::WindowExt;
use xxhash_rust::xxh3::xxh3_64;

static NEXT_SCREENSHOT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_PINNED_IMAGE_WINDOW_ID: AtomicU64 = AtomicU64::new(1);
static SCREENSHOT_LIFECYCLE_BOUND_FOR_BOOT_WINDOW: AtomicBool = AtomicBool::new(false);
static RECENT_COPY_PASTE: OnceLock<StdMutex<Option<RecentCopyPaste>>> = OnceLock::new();
static COPY_PASTE_DEDUP_ENABLED: AtomicBool = AtomicBool::new(true);
static COPY_PASTE_DEDUP_WINDOW_MS: AtomicU64 = AtomicU64::new(1200);
static COPY_PASTE_DEDUP_LOG_ENABLED: AtomicBool = AtomicBool::new(true);
static COPY_PASTE_DEDUP_TOTAL_REQUESTS: AtomicU64 = AtomicU64::new(0);
static COPY_PASTE_DEDUP_HIT_COUNT: AtomicU64 = AtomicU64::new(0);
static COPY_PASTE_DEDUP_REQUEST_ID_HIT_COUNT: AtomicU64 = AtomicU64::new(0);
static COPY_PASTE_DEDUP_TEXT_HASH_HIT_COUNT: AtomicU64 = AtomicU64::new(0);
static COPY_PASTE_DEDUP_LOG_COUNT: AtomicU64 = AtomicU64::new(0);
static COPY_PASTE_DEDUP_WINDOW_STATS: OnceLock<StdMutex<DedupWindowStats>> = OnceLock::new();
#[cfg(debug_assertions)]
static VC_RUNTIME_FORCE_MISSING: AtomicBool = AtomicBool::new(false);

struct RecentCopyPaste {
    request_id: String,
    text_hash: u64,
    created_at_ms: u64,
}

struct DedupWindowStats {
    window_start_ms: u64,
    requests: u64,
    hits: u64,
    last_hit_at_ms: u64,
}

fn calc_text_hash(text: &str) -> u64 {
    xxh3_64(text.as_bytes())
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn is_duplicate_copy_paste_request(text: &str, request_id: Option<&str>) -> bool {
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
        let same_request_id = !request_id_trimmed.is_empty() && request_id_trimmed == last.request_id;
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

#[cfg(debug_assertions)]
fn get_copy_paste_dedup_debug_state_value() -> serde_json::Value {
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

fn bind_screenshot_window_lifecycle(window: &tauri::WebviewWindow) {
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed => {
            features::screenshot::capture::set_screenshot_in_progress(false);
        }
        _ => {}
    });
}

#[derive(serde::Serialize)]
pub struct HistoryResponse {
    history: Vec<String>,
    categories: HashMap<String, String>,
    category_list: Vec<String>,
    pinned_items: Vec<String>,
}

/// 批量获取剪贴板完整快照（优化 IPC 通信）
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardFullSnapshot {
    pub text_history: Vec<String>,
    pub text_categories: HashMap<String, String>,
    pub text_category_list: Vec<String>,
    pub text_pinned_items: Vec<String>,
    pub image_history: Vec<ImageHistoryPreviewItem>,
    pub image_categories: HashMap<String, String>,
    pub image_category_list: Vec<String>,
    pub image_tags: HashMap<String, Vec<String>>,
    pub image_pinned_items: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardHistoryPageRequest {
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_history_page_limit")]
    limit: usize,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    pinned_only: bool,
    #[serde(default)]
    keyword: Option<String>,
    #[serde(default)]
    sort_by: Option<String>,
    #[serde(default)]
    sort_order: Option<String>,
}

#[tauri::command]
pub async fn get_image_clipboard_history_page(
    request: ImageHistoryPageRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<ImageHistoryPageData, String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };

    let page_request = crate::utils::image_clipboard::ImageHistoryPageRequest {
        offset: request.offset,
        limit: request.limit,
        category: request.category,
        keyword: request.keyword,
        pinned_only: request.pinned_only,
        sort_by: request.sort_by,
        sort_order: request.sort_order,
    };

    Ok(manager.get_history_preview_page_async(page_request).await)
}

fn default_history_page_limit() -> usize {
    50
}

#[derive(serde::Serialize)]
pub struct ImageHistoryResponse {
    history: Vec<ImageHistoryPreviewItem>,
    categories: HashMap<String, String>,
    category_list: Vec<String>,
    image_tags: HashMap<String, Vec<String>>,
    pinned_items: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectAndFillRequest {
    index: usize,
    #[serde(default)]
    op_id: Option<u64>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectAndFillImageByIdRequest {
    item_id: String,
    #[serde(default)]
    op_id: Option<u64>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemIdRequest {
    item_id: String,
}

#[tauri::command]
pub async fn open_image_preview_window_by_id(
    request: ItemIdRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_open_image_preview_window_by_id(request.item_id, state_arc, app)
            .map_err(to_frontend_error_string)
    })
        .await
        .map_err(|e| frontend_error(ErrorCode::SystemError, "打开图片预览任务执行失败", e.to_string()))?
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FillKind {
    Text,
    Image,
}

impl FillKind {
    fn label(self) -> &'static str {
        match self {
            Self::Text => "文本",
            Self::Image => "图片",
        }
    }

    fn window_label(self) -> &'static str {
        match self {
            Self::Text => "clipboard",
            Self::Image => "image_clipboard",
        }
    }

    fn current_seq(self, state: &SharedAppState) -> u64 {
        match self {
            Self::Text => state.text_fill_seq,
            Self::Image => state.image_fill_seq,
        }
    }
}

fn lock_arc_mutex<T>(mutex: &Arc<Mutex<T>>) -> crate::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|never| match never {})
}

fn is_screenshot_feature_enabled(state: &Arc<Mutex<SharedAppState>>) -> bool {
    let guard = lock_arc_mutex(state);
    guard.settings.screenshot_enabled
}

fn register_text_shortcut(
    app: &AppHandle,
    state: Arc<Mutex<SharedAppState>>,
    hot_key: &str,
) -> Result<(), String> {
    let app_clone = app.clone();
    let hot_key_string = hot_key.to_string();
    app.global_shortcut()
        .on_shortcut(hot_key, move |_app, _shortcut, event| {
            if let ShortcutState::Pressed = event.state {
                let sg = lock_arc_mutex(&state);
                if !sg.settings.text_clipboard_enabled {
                    return;
                }
                if !sg.is_visible && !sg.is_image_visible {
                    let state_for_window = state.clone();
                    drop(sg);
                    interrupt_text_fill_flow(&state_for_window);
                    show_clipboard_window(app_clone.clone(), state_for_window);
                    features::mouse_listener::reset_ctrl_key_state();
                }
            }
        })
        .map_err(|e| {
            frontend_error(
                ErrorCode::ValidationError,
                format!("快捷键被占用或注册失败：{}", hot_key_string),
                e.to_string(),
            )
        })?;
    Ok(())
}

fn register_image_shortcut(
    app: &AppHandle,
    state: Arc<Mutex<SharedAppState>>,
    hot_key: &str,
) -> Result<(), String> {
    let app_clone = app.clone();
    let hot_key_string = hot_key.to_string();
    app.global_shortcut()
        .on_shortcut(hot_key, move |_app, _shortcut, event| {
            if let ShortcutState::Pressed = event.state {
                let sg = lock_arc_mutex(&state);
                if !sg.settings.image_clipboard_enabled {
                    return;
                }
                if !sg.is_visible && !sg.is_image_visible {
                    let state_for_window = state.clone();
                    drop(sg);
                    interrupt_image_fill_flow(&state_for_window);
                    show_image_clipboard_window(app_clone.clone(), state_for_window);
                }
            }
        })
        .map_err(|e| {
            frontend_error(
                ErrorCode::ValidationError,
                format!("图片窗口快捷键被占用或注册失败：{}", hot_key_string),
                e.to_string(),
            )
        })?;
    Ok(())
}

fn register_screenshot_shortcut(app: &AppHandle, hot_key: &str) -> Result<(), String> {
    let app_clone = app.clone();
    app.global_shortcut()
        .on_shortcut(hot_key, move |_app, _shortcut, event| {
            if let ShortcutState::Pressed = event.state {
                let app_handle_inner = app_clone.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = open_screenshot_editor(app_handle_inner, None).await {
                        log::error!("截图失败: {}", e);
                    }
                });
            }
        })
        .map_err(|e| frontend_error(ErrorCode::SystemError, "注册截图快捷键失败", e.to_string()))?;
    Ok(())
}

fn begin_fill_sequence(state: &Arc<Mutex<SharedAppState>>, kind: FillKind) -> u64 {
    let mut state_guard = lock_arc_mutex(state);
    state_guard.is_updating_clipboard = true;
    state_guard.is_processing_selection = true;
    state_guard.selection_guard_epoch = state_guard.selection_guard_epoch.wrapping_add(1);
    match kind {
        FillKind::Text => {
            state_guard.text_fill_seq = state_guard.text_fill_seq.wrapping_add(1);
            state_guard.text_fill_seq
        }
        FillKind::Image => {
            state_guard.image_fill_seq = state_guard.image_fill_seq.wrapping_add(1);
            state_guard.image_fill_seq
        }
    }
}

fn is_fill_latest(state: &Arc<Mutex<SharedAppState>>, kind: FillKind, fill_seq: u64) -> bool {
    let guard = lock_arc_mutex(state);
    kind.current_seq(&guard) == fill_seq
}

fn finish_fill_if_latest(state: &Arc<Mutex<SharedAppState>>, kind: FillKind, fill_seq: u64) {
    let mut guard = lock_arc_mutex(state);
    if kind.current_seq(&guard) == fill_seq {
        guard.is_processing_selection = false;
        guard.is_updating_clipboard = false;
    }
}

static IMAGE_PROMOTE_SENDER: OnceLock<Sender<String>> = OnceLock::new();

pub fn interrupt_text_fill_flow(state: &Arc<Mutex<SharedAppState>>) {
    let mut state_guard = lock_arc_mutex(state);
    state_guard.text_fill_seq = state_guard.text_fill_seq.wrapping_add(1);
    state_guard.is_processing_selection = false;
    state_guard.is_updating_clipboard = false;
}

pub fn interrupt_image_fill_flow(state: &Arc<Mutex<SharedAppState>>) {
    let mut state_guard = lock_arc_mutex(state);
    state_guard.image_fill_seq = state_guard.image_fill_seq.wrapping_add(1);
    state_guard.is_processing_selection = false;
    state_guard.is_updating_clipboard = false;
}

fn image_promote_worker(state: Arc<Mutex<SharedAppState>>, rx: Receiver<String>) {
    while let Ok(mut item_id) = rx.recv() {
        while let Ok(latest_item_id) = rx.try_recv() {
            item_id = latest_item_id;
        }
        let manager_arc = {
            let state_guard = lock_arc_mutex(&state);
            state_guard.image_clipboard_manager.clone()
        };
        let manager = lock_arc_mutex(&manager_arc);
        if let Err(e) = manager.promote_to_top_by_id(&item_id) {
            log::warn!("极速模式异步置顶图片失败: {}", e);
        } else {
            manager.sync_positions_to_store();
        }
    }
}

fn schedule_image_promote_to_top(state: Arc<Mutex<SharedAppState>>, item_id: String) {
    let sender = IMAGE_PROMOTE_SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<String>();
        let state_for_worker = state.clone();
        thread::spawn(move || image_promote_worker(state_for_worker, rx));
        tx
    });
    if let Err(e) = sender.send(item_id) {
        log::warn!("提交极速模式异步置顶任务失败: {}", e);
    }
}

fn wait_for_fill_window_hidden(app: &AppHandle, window_label: &str, label: &str, fast_path: bool) {
    let timeout_ms = if fast_path { 220 } else { 900 };
    let state_arc = app.state::<Arc<Mutex<SharedAppState>>>().inner().clone();
    if let Err(e) = crate::ui::window_manager::wait_for_window_hidden(
        app,
        &state_arc,
        window_label,
        Duration::from_millis(timeout_ms),
    ) {
        log::warn!("等待{}窗口隐藏失败: {}", label, e);
    }
}

fn spawn_fill_task<F>(
    kind: FillKind,
    app_handle: AppHandle,
    state: Arc<Mutex<SharedAppState>>,
    fill_seq: u64,
    operation_id: u64,
    write_stage: F,
) where
    F: FnOnce(&AppHandle, &Arc<Mutex<SharedAppState>>) -> Result<(), String> + Send + 'static,
{
    thread::spawn(move || {
        let started_at = std::time::Instant::now();
        let fast_path = kind == FillKind::Image && is_fast_fill_verify_mode_enabled();
        wait_for_fill_window_hidden(&app_handle, kind.window_label(), kind.label(), fast_path);

        if !is_fill_latest(&state, kind, fill_seq) {
            log::info!("{}回填请求过期，跳过执行: op_id={}", kind.label(), operation_id);
            return;
        }

        let fill_result = write_stage(&app_handle, &state);
        if fill_result.is_ok() {
            if !is_fill_latest(&state, kind, fill_seq) {
                log::info!(
                    "{}回填请求被新请求替代: op_id={}",
                    kind.label(),
                    operation_id
                );
                return;
            }
            simulate_paste_with_retry(kind.label(), Some(operation_id), started_at, fast_path);
        } else if let Err(e) = fill_result {
            log::error!("{}回填失败（写入阶段）: op_id={}, {}", kind.label(), operation_id, e);
        }

        finish_fill_if_latest(&state, kind, fill_seq);
    });
}

fn simulate_paste_with_retry(
    label: &str,
    operation_id: Option<u64>,
    started_at: std::time::Instant,
    fast_path: bool,
) {
    let is_post_paste_ctrl_release_error = |err: &str| err.contains("释放 Ctrl");
    let mode_name = if fast_path { "极速模式" } else { "普通模式" };
    let retry_delays: &[u64] = if fast_path { &[8, 16] } else { &[22, 40, 58] };

    match crate::ui::window_manager::simulate_paste() {
        Ok(_) => {
            if let Some(op_id) = operation_id {
                log::info!(
                    "{}回填完成: op_id={}, 耗时: {}ms",
                    label,
                    op_id,
                    started_at.elapsed().as_millis()
                );
            } else {
                log::info!("{}回填完成，耗时: {}ms", label, started_at.elapsed().as_millis());
            }
        }
        Err(first_error) => {
            if is_post_paste_ctrl_release_error(&first_error) {
                log::warn!(
                    "{}回填检测到粘贴后Ctrl释放异常，跳过二次粘贴以避免重复输入: {}",
                    label,
                    first_error
                );
                if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                    log::warn!("{}回填粘贴后Ctrl异常兜底释放失败: {}", label, release_error);
                }
                return;
            }
            let mut final_error = first_error.clone();
            for delay in retry_delays {
                thread::sleep(Duration::from_millis(*delay));
                if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                    log::warn!("{}回填{}重试前释放Ctrl失败: {}", label, mode_name, release_error);
                }
                match crate::ui::window_manager::simulate_paste() {
                    Ok(_) => {
                        if let Some(op_id) = operation_id {
                            log::warn!(
                                "{}回填{}首次粘贴失败，状态驱动重试成功: op_id={}, {}，总耗时: {}ms",
                                label,
                                mode_name,
                                op_id,
                                first_error,
                                started_at.elapsed().as_millis()
                            );
                        } else {
                            log::warn!(
                                "{}回填{}首次粘贴失败，状态驱动重试成功: {}，总耗时: {}ms",
                                label,
                                mode_name,
                                first_error,
                                started_at.elapsed().as_millis()
                            );
                        }
                        return;
                    }
                    Err(next_error) => {
                        final_error = next_error;
                        if is_post_paste_ctrl_release_error(&final_error) {
                            log::warn!(
                                "{}回填{}检测到粘贴后Ctrl释放异常，停止后续重试: {}",
                                label,
                                mode_name,
                                final_error
                            );
                            if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                                log::warn!("{}回填粘贴后Ctrl异常兜底释放失败: {}", label, release_error);
                            }
                            return;
                        }
                    }
                }
            }
            if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                log::warn!("{}回填{}最终兜底释放Ctrl失败: {}", label, mode_name, release_error);
            }
            if let Some(op_id) = operation_id {
                log::error!(
                    "{}回填{}粘贴失败: op_id={}, 首次错误: {}，最终错误: {}",
                    label,
                    mode_name,
                    op_id,
                    first_error,
                    final_error
                );
            } else {
                log::error!(
                    "{}回填{}粘贴失败，首次错误: {}，最终错误: {}",
                    label,
                    mode_name,
                    first_error,
                    final_error
                );
            }
        }
    }
}

fn set_updating_clipboard(state: &Arc<Mutex<SharedAppState>>, updating: bool) {
    let mut state_guard = lock_arc_mutex(state);
    state_guard.is_updating_clipboard = updating;
}

fn get_clipboard_manager_arc(state: &Arc<Mutex<SharedAppState>>) -> Arc<Mutex<ClipboardManager>> {
    let state_guard = lock_arc_mutex(state);
    state_guard.clipboard_manager.clone()
}

fn get_image_clipboard_manager_arc(
    state: &Arc<Mutex<SharedAppState>>,
) -> Arc<Mutex<ImageClipboardManager>> {
    let state_guard = lock_arc_mutex(state);
    state_guard.image_clipboard_manager.clone()
}

fn frontend_error(code: ErrorCode, message: impl Into<String>, details: impl Into<String>) -> String {
    to_frontend_error_string(AppError::new(code, message).with_details(details.into()))
}

fn with_updating_clipboard<T, F>(
    state: &Arc<Mutex<SharedAppState>>,
    operation: F,
) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    set_updating_clipboard(state, true);
    let result = operation();
    set_updating_clipboard(state, false);
    result
}

fn try_replace_text_clipboard_after_remove(
    state: &Arc<Mutex<SharedAppState>>,
    app: &AppHandle,
    removed_item: &str,
) {
    let manager_arc = get_clipboard_manager_arc(state);
    let current_clipboard = {
        let manager = lock_arc_mutex(&manager_arc);
        manager.get_content(app)
    };

    if current_clipboard.as_deref() != Some(removed_item) {
        return;
    }

    let next_item = {
        let manager = lock_arc_mutex(&manager_arc);
        manager.get_history().first().cloned()
    };
    if let Some(next) = next_item {
        let manager = lock_arc_mutex(&manager_arc);
        if let Err(e) = manager.set_clipboard_content(app, &next) {
            log::warn!("删除文本后写入下一条到剪贴板失败: {}", e);
        }
    }
}

fn try_replace_image_clipboard_after_remove(
    state: &Arc<Mutex<SharedAppState>>,
    app: &AppHandle,
    removed_signature: &str,
) {
    let manager_arc = get_image_clipboard_manager_arc(state);
    let should_replace_clipboard =
        match ImageClipboardManager::read_clipboard_images_rgba(app) {
            Ok(images) if !images.is_empty() => {
                let (rgba, width, height, _) = &images[0];
                crate::utils::image_clipboard::compute_signature(rgba, *width, *height)
                    == removed_signature
            }
            _ => false,
        };

    if !should_replace_clipboard {
        return;
    }

    let next_image = {
        let manager = lock_arc_mutex(&manager_arc);
        manager.get_image_by_index(0).ok()
    };
    if let Some(image) = next_image {
        if let Err(e) =
            ImageClipboardManager::write_clipboard_image(app, &image)
        {
            log::warn!("删除图片后写入下一张到剪贴板失败: {}", e);
        }
    }
}

fn execute_select_and_fill_text(
    request: SelectAndFillRequest,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> AppResult<String> {
    let index = request.index;
    let fill_seq = begin_fill_sequence(&state, FillKind::Text);
    let operation_id = request.op_id.unwrap_or(fill_seq);
    let manager_arc = get_clipboard_manager_arc(&state);

    let item_content = {
        let manager = lock_arc_mutex(&manager_arc);
        manager
            .promote_to_top(index)
            .map_err(|e| {
                AppError::new(ErrorCode::ClipboardError, format!("索引 {} 超出范围", index))
                    .with_details(e)
            })?
    };

    hide_clipboard_window(app.clone(), state.clone());

    let item_content_clone = item_content.clone();
    let manager_arc_for_fill = manager_arc.clone();
    spawn_fill_task(
        FillKind::Text,
        app,
        state,
        fill_seq,
        operation_id,
        move |app_handle, state_ref| {
            let _ = state_ref;
            let manager = lock_arc_mutex(&manager_arc_for_fill);
            manager.set_clipboard_content(app_handle, &item_content_clone)?;
            let _ = app_handle.emit(
                "text-item-promoted",
                serde_json::json!({
                    "content": item_content_clone,
                }),
            );
            Ok(())
        },
    );

    Ok(item_content)
}

fn execute_remove_clipboard_item(
    index: Option<usize>,
    item: Option<String>,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> AppResult<()> {
    log::info!("删除剪贴板项目，索引: {:?}, 内容存在: {}", index, item.is_some());
    let manager_arc = get_clipboard_manager_arc(&state);
    with_updating_clipboard(&state, || -> Result<(), String> {
        let resolved_index = {
            let manager = lock_arc_mutex(&manager_arc);
            if let Some(content) = item.as_ref().filter(|v| !v.trim().is_empty()) {
                manager
                    .get_history()
                    .iter()
                    .position(|entry| entry == content)
                    .or(index)
                    .ok_or_else(|| "索引超出范围".to_string())?
            } else {
                index.ok_or_else(|| "索引超出范围".to_string())?
            }
        };
        let removed_item = {
            let manager = lock_arc_mutex(&manager_arc);
            manager.remove_from_history(resolved_index)?
        };
        try_replace_text_clipboard_after_remove(&state, &app, &removed_item);
        Ok(())
    })
        .map_err(|e| AppError::new(ErrorCode::ClipboardError, "删除文本历史失败").with_details(e))
}

fn execute_open_image_preview_window_by_id(
    item_id: String,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> AppResult<()> {
    let request_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .to_string();
    show_image_preview_loading_window(app.clone(), request_id.clone())
        .map_err(|e| AppError::new(ErrorCode::SystemError, "打开预览加载窗口失败").with_details(e))?;
    let state_clone = state;
    let app_clone = app;
    let request_id_clone = request_id;
    thread::spawn(move || {
        let result: Result<(), String> = (|| {
            let manager_arc = get_image_clipboard_manager_arc(&state_clone);
            let image_path = {
                let manager = lock_arc_mutex(&manager_arc);
                manager.get_preview_image_path_by_id(&item_id)?
            };
            let preview_path = ensure_preview_image_path_for_asset(&item_id, &image_path)?;
            show_image_preview_window(app_clone.clone(), request_id_clone.clone(), preview_path)
        })();
        if let Err(e) = result {
            log::error!("加载预览图片失败: {}", e);
            let _ = app_clone.emit(
                "show-image-preview",
                serde_json::json!({
                    "request_id": request_id_clone,
                    "error_message": e,
                    "is_final": true
                }),
            );
        }
    });
    Ok(())
}

fn ensure_preview_image_path_for_asset(item_id: &str, image_path: &str) -> Result<String, String> {
    let trimmed = image_path.trim();
    if trimmed.is_empty() {
        return Err("图片路径为空".to_string());
    }
    let source_path = PathBuf::from(trimmed);
    if !source_path.exists() {
        return Err(format!("图片文件不存在: {}", trimmed));
    }
    if !source_path.is_file() {
        return Err(format!("图片路径不是文件: {}", trimmed));
    }
    let canonical_source = source_path
        .canonicalize()
        .map_err(|e| format!("规范化图片路径失败: {}", e))?;
    let ext = canonical_source
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let allowed_ext = ["png", "jpg", "jpeg", "webp", "bmp", "gif"];
    if !allowed_ext.contains(&ext.as_str()) {
        return Err(format!("不支持的图片格式: {}", ext));
    }

    let mut blobs_dir = std::env::current_exe()
        .map_err(|e| format!("获取程序目录失败: {}", e))?;
    blobs_dir.pop();
    blobs_dir.push("image_history_blobs");
    fs::create_dir_all(&blobs_dir).map_err(|e| format!("创建图片目录失败: {}", e))?;
    let canonical_blobs = blobs_dir
        .canonicalize()
        .unwrap_or_else(|_| blobs_dir.clone());
    if canonical_source.starts_with(&canonical_blobs) {
        return Ok(canonical_source.to_string_lossy().to_string());
    }

    let normalized_item_id = sanitize_image_item_id(item_id);
    let target_name = if ext.is_empty() {
        format!("preview_external_{}.png", normalized_item_id)
    } else {
        format!("preview_external_{}.{}", normalized_item_id, ext)
    };
    let target_path = canonical_blobs.join(target_name);
    fs::copy(&canonical_source, &target_path)
        .map_err(|e| format!("复制预览图片到受控目录失败: {}", e))?;
    Ok(target_path.to_string_lossy().to_string())
}

fn sanitize_image_item_id(raw: &str) -> String {
    let sanitized: String = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

fn execute_warmup_image_clipboard_item_by_id(
    item_id: String,
    state: Arc<Mutex<SharedAppState>>,
) -> AppResult<()> {
    let manager_arc = get_image_clipboard_manager_arc(&state);
    let manager = lock_arc_mutex(&manager_arc);
    manager
        .warmup_image_by_id(&item_id)
        .map_err(|e| AppError::new(ErrorCode::ClipboardError, "预热图片失败").with_details(e))
}

fn execute_promote_image_clipboard_item_by_id(
    item_id: String,
    state: Arc<Mutex<SharedAppState>>,
) -> AppResult<()> {
    let manager_arc = get_image_clipboard_manager_arc(&state);
    let manager = lock_arc_mutex(&manager_arc);
    manager
        .promote_to_top_by_id(&item_id)
        .map_err(|e| AppError::new(ErrorCode::ClipboardError, "置顶图片失败").with_details(e))
}

fn execute_remove_image_clipboard_item_by_id(
    item_id: String,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> AppResult<()> {
    let manager_arc = get_image_clipboard_manager_arc(&state);
    with_updating_clipboard(&state, || -> Result<(), String> {
        let removed_signature = {
            let manager = lock_arc_mutex(&manager_arc);
            let (_, _, signature) = manager.remove_from_history_by_id(&item_id)?;
            signature
        };
        try_replace_image_clipboard_after_remove(&state, &app, &removed_signature);
        Ok(())
    })
        .map_err(|e| AppError::new(ErrorCode::ClipboardError, "删除图片历史失败").with_details(e))
}

fn execute_select_and_fill_image_by_id(
    request: SelectAndFillImageByIdRequest,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> AppResult<()> {
    let item_id = request.item_id;
    let fill_seq = begin_fill_sequence(&state, FillKind::Image);
    let operation_id = request.op_id.unwrap_or(fill_seq);
    let manager_arc = get_image_clipboard_manager_arc(&state);

    hide_image_clipboard_window(app.clone(), state.clone());

    spawn_fill_task(
        FillKind::Image,
        app,
        state,
        fill_seq,
        operation_id,
        move |app_handle, state_ref| {
            let fast_mode = is_fast_fill_verify_mode_enabled();
            let image = {
                let _ = state_ref;
                let manager = lock_arc_mutex(&manager_arc);
                if fast_mode {
                    manager.promote_to_top_in_memory_by_id(&item_id)?;
                    manager.get_image_by_index_for_fill(0)?
                } else {
                    manager.promote_to_top_by_id(&item_id)?;
                    manager.get_image_by_index_for_fill(0)?
                }
            };
            ImageClipboardManager::write_clipboard_image(
                app_handle, &image,
            )?;
            let _ = app_handle.emit(
                "image-item-pinned",
                serde_json::json!({
                    "itemId": item_id,
                    "pinned": true,
                }),
            );
            if fast_mode {
                schedule_image_promote_to_top(state_ref.clone(), item_id.clone());
            }
            Ok(())
        },
    );

    Ok(())
}

#[tauri::command]
pub async fn get_clipboard_history(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<HistoryResponse, String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = lock_arc_mutex(&manager_arc);
    Ok(HistoryResponse {
        history: manager.get_history(),
        categories: manager.get_categories(),
        category_list: manager.get_category_list(),
        pinned_items: manager.get_pinned_items(),
    })
}

/// 批量获取剪贴板完整快照（优化 IPC 通信）
/// 一次 IPC 调用获取所有需要的数据，减少通信开销
#[tauri::command]
pub async fn get_clipboard_full_snapshot(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<ClipboardFullSnapshot, String> {
    let (text_manager_arc, image_manager_arc) = {
        let state_guard = lock_arc_mutex(state.inner());
        (
            state_guard.clipboard_manager.clone(),
            state_guard.image_clipboard_manager.clone(),
        )
    };

    let text_manager = lock_arc_mutex(&text_manager_arc);
    let text_history = text_manager.get_history();
    let text_categories = text_manager.get_categories();
    let text_category_list = text_manager.get_category_list();
    let text_pinned_items = text_manager.get_pinned_items();
    drop(text_manager);

    let image_manager = lock_arc_mutex(&image_manager_arc);
    let image_history = image_manager.get_history_preview();
    let image_categories = image_manager.get_categories();
    let image_category_list = image_manager.get_category_list();
    let image_tags = image_manager.get_image_tags();
    let image_pinned_items = image_manager.get_pinned_items();
    drop(image_manager);

    Ok(ClipboardFullSnapshot {
        text_history,
        text_categories,
        text_category_list,
        text_pinned_items,
        image_history,
        image_categories,
        image_category_list,
        image_tags,
        image_pinned_items,
    })
}

#[tauri::command]
pub async fn get_clipboard_history_page(
    request: ClipboardHistoryPageRequest,
) -> Result<ClipboardHistoryPageData, String> {
    load_history_page_data_async(
        request.offset,
        request.limit,
        request.category,
        request.pinned_only,
        request.keyword,
        request.sort_by,
        request.sort_order,
    )
        .await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageHistoryPageRequest {
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_image_page_limit")]
    limit: usize,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    keyword: Option<String>,
    #[serde(default)]
    pinned_only: bool,
    #[serde(default)]
    sort_by: Option<String>,
    #[serde(default)]
    sort_order: Option<String>,
}

fn default_image_page_limit() -> usize {
    50
}

#[tauri::command]
pub async fn set_item_category(
    item: String,
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .set_category_async(item, category)
        .await
        .map_err(|e| to_frontend_error_string(AppError::new(ErrorCode::ClipboardError, "设置文本分类失败").with_details(e)))
}

#[tauri::command]
pub async fn remove_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .remove_category_async(category)
        .await
        .map_err(|e| to_frontend_error_string(AppError::new(ErrorCode::ClipboardError, "删除文本分类失败").with_details(e)))
}

#[tauri::command]
pub async fn add_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .add_category_async(category)
        .await
        .map_err(|e| to_frontend_error_string(AppError::new(ErrorCode::ClipboardError, "新增文本分类失败").with_details(e)))
}

#[tauri::command]
pub async fn get_image_clipboard_history(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<ImageHistoryResponse, String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    Ok(ImageHistoryResponse {
        history: manager.get_history_preview(),
        categories: manager.get_categories(),
        category_list: manager.get_category_list(),
        image_tags: manager.get_image_tags(),
        pinned_items: manager.get_pinned_items(),
    })
}

#[tauri::command]
pub async fn close_image_preview_window(app: AppHandle) -> Result<(), String> {
    hide_image_preview_window(app);
    Ok(())
}

#[tauri::command]
pub async fn start_image_preview_window_drag(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("image_preview")
        .ok_or_else(|| "图片预览窗口不存在".to_string())?;
    window.start_dragging().map_err(|e| format!("拖动窗口失败: {}", e))
}

#[tauri::command]
pub async fn warmup_image_clipboard_item_by_id(
    request: ItemIdRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_warmup_image_clipboard_item_by_id(request.item_id, state_arc)
            .map_err(to_frontend_error_string)
    })
        .await
        .map_err(|e| frontend_error(ErrorCode::SystemError, "预热图片任务执行失败", e.to_string()))?
}

/// 优化方案 5：批量预热多个图片到内存缓存，用于滚动时提前加载
#[tauri::command]
pub async fn warmup_multiple_images(
    item_ids: Vec<String>,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let manager_arc = get_image_clipboard_manager_arc(&state_arc);
        let manager = lock_arc_mutex(&manager_arc);
        for item_id in item_ids {
            if let Some(index) = manager.get_history().iter().position(|item| item.id == item_id) {
                if index < 6 {
                    let _ = manager.warmup_image_by_id(&item_id);
                }
            }
        }
    })
        .await
        .map_err(|e| frontend_error(ErrorCode::SystemError, "批量预热图片任务执行失败", e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn set_image_item_category(
    item_id: String,
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .set_category_async(item_id, category)
        .await
        .map_err(|e| to_frontend_error_string(AppError::new(ErrorCode::ClipboardError, "设置图片分类失败").with_details(e)))
}

#[tauri::command]
pub async fn remove_image_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .remove_category_async(category)
        .await
        .map_err(|e| to_frontend_error_string(AppError::new(ErrorCode::ClipboardError, "删除图片分类失败").with_details(e)))
}

#[tauri::command]
pub async fn set_image_item_tags(
    item_id: String,
    tags: Vec<String>,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .set_tags_async(item_id, tags)
        .await
        .map_err(|e| to_frontend_error_string(AppError::new(ErrorCode::ClipboardError, "设置图片标签失败").with_details(e)))
}

#[tauri::command]
pub async fn set_clipboard_item_pinned(
    index: Option<usize>,
    item: Option<String>,
    pinned: bool,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .set_pinned_by_selector_async(index, item, pinned)
        .await
        .map_err(|e| {
            if e == "索引超出范围" {
                to_frontend_error_string(AppError::new(ErrorCode::ValidationError, "索引超出范围"))
            } else {
                to_frontend_error_string(AppError::new(ErrorCode::ClipboardError, "设置置顶状态失败").with_details(e))
            }
        })
}

#[tauri::command]
pub async fn set_image_item_pinned(
    item_id: String,
    pinned: bool,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .set_pinned_async(item_id, pinned)
        .await
        .map_err(|e| to_frontend_error_string(AppError::new(ErrorCode::ClipboardError, "设置图片置顶状态失败").with_details(e)))
}

#[tauri::command]
pub async fn promote_clipboard_item(
    index: usize,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .promote_to_top_async(index)
        .await
        .map(|_| ())
        .map_err(|e| to_frontend_error_string(AppError::new(ErrorCode::ClipboardError, "置顶文本失败").with_details(e)))
}

#[tauri::command]
pub async fn promote_image_clipboard_item_by_id(
    request: ItemIdRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_promote_image_clipboard_item_by_id(request.item_id, state_arc)
            .map_err(to_frontend_error_string)
    })
        .await
        .map_err(|e| frontend_error(ErrorCode::SystemError, "置顶图片任务执行失败", e.to_string()))?
}

#[tauri::command]
pub async fn clear_text_history(
    mode: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<usize, String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .clear_history_by_mode_async(mode.as_str())
        .await
        .map_err(|e| to_frontend_error_string(AppError::new(ErrorCode::ClipboardError, "清理文本历史失败").with_details(e)))
}

#[tauri::command]
pub async fn clear_image_history(
    mode: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<usize, String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    let removed = manager
        .clear_history_by_mode_async(mode.as_str())
        .await
        .map_err(|e| to_frontend_error_string(AppError::new(ErrorCode::ClipboardError, "清理图片历史失败").with_details(e)))?;

    // 通知图片窗口更新数据
    emit_image_history_payload(&app, state.inner().clone());

    Ok(removed)
}

#[tauri::command]
pub async fn import_image_files(
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<usize, String> {
    if paths.is_empty() {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "未选择任何文件或文件夹",
            "paths is empty",
        ));
    }
    let image_paths = collect_import_image_paths(paths).map_err(|e| {
        frontend_error(ErrorCode::IoError, "收集可导入图片路径失败", e)
    })?;
    if image_paths.is_empty() {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "未找到可导入的图片",
            "collected image paths is empty",
        ));
    }
    let total = image_paths.len();
    let manager = {
        let state_guard = lock_arc_mutex(state.inner());
        let manager_guard = lock_arc_mutex(&state_guard.image_clipboard_manager);
        manager_guard.clone()
    };
    let _ = app.emit(
        "image-import-progress",
        serde_json::json!({
            "status": "start",
            "total": total,
            "processed": 0,
            "imported": 0,
            "failed": 0
        }),
    );
    let mut imported = 0usize;
    let mut failed = 0usize;
    let mut processed = 0usize;
    let mut last_error = String::new();
    for path in image_paths {
        match manager.import_local_image_paths_async(vec![path.clone()]).await {
            Ok(count) => {
                imported = imported.saturating_add(count);
            }
            Err(e) => {
                failed = failed.saturating_add(1);
                last_error = e;
            }
        }
        processed = processed.saturating_add(1);
        let _ = app.emit(
            "image-import-progress",
            serde_json::json!({
                "status": "progress",
                "total": total,
                "processed": processed,
                "imported": imported,
                "failed": failed
            }),
        );
    }
    let _ = app.emit(
        "image-import-progress",
        serde_json::json!({
            "status": "finish",
            "total": total,
            "processed": processed,
            "imported": imported,
            "failed": failed
        }),
    );
    if imported > 0 {
        emit_image_history_payload(&app, state.inner().clone());
    }
    if imported == 0 {
        if last_error.is_empty() {
            Err(frontend_error(
                ErrorCode::ClipboardError,
                "未导入任何图片",
                "imported == 0 and no detailed error",
            ))
        } else {
            Err(frontend_error(
                ErrorCode::ClipboardError,
                "导入图片失败",
                last_error,
            ))
        }
    } else {
        Ok(imported)
    }
}

#[tauri::command]
pub async fn count_import_image_files(paths: Vec<String>) -> Result<usize, String> {
    if paths.is_empty() {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "未选择任何文件或文件夹",
            "paths is empty",
        ));
    }
    let image_paths = collect_import_image_paths(paths).map_err(|e| {
        frontend_error(ErrorCode::IoError, "统计可导入图片路径失败", e)
    })?;
    Ok(image_paths.len())
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct RecordingRegionSelectedPayload {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[tauri::command]
pub async fn notify_recording_region_selected(
    app: AppHandle,
    payload: RecordingRegionSelectedPayload,
) -> Result<(), String> {
    app.emit("recording-region-selected", payload)
        .map_err(|e| e.to_string())
}

fn collect_import_image_paths(entries: Vec<String>) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for raw in entries {
        let path = raw.trim();
        if path.is_empty() {
            continue;
        }
        let p = Path::new(path);
        if p.is_file() {
            if is_importable_image_file(p) {
                out.push(path.to_string());
            }
            continue;
        }
        if p.is_dir() {
            collect_images_from_dir(p, &mut out)?;
        }
    }
    Ok(out)
}

fn collect_images_from_dir(dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("读取目录失败: {}", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
        let path = entry.path();
        if path.is_dir() {
            collect_images_from_dir(&path, out)?;
        } else if path.is_file() && is_importable_image_file(&path) {
            out.push(path.to_string_lossy().to_string());
        }
    }
    Ok(())
}

fn is_importable_image_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase());
    matches!(
        ext.as_deref(),
        Some("png")
            | Some("jpg")
            | Some("jpeg")
            | Some("bmp")
            | Some("gif")
            | Some("webp")
            | Some("tif")
            | Some("tiff")
    )
}

#[tauri::command]
pub async fn add_image_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_image_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .add_category_async(category)
        .await
        .map_err(|e| to_frontend_error_string(AppError::new(ErrorCode::ClipboardError, "新增图片分类失败").with_details(e)))
}

#[tauri::command]
pub async fn get_clipboard_bottom_offset(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<i32, String> {
    let state_guard = lock_arc_mutex(state.inner());
    Ok(state_guard.settings.clipboard_bottom_offset)
}

#[tauri::command]
pub async fn preview_clipboard_bottom_offset(
    offset: i32,
    app: AppHandle,
) -> Result<(), String> {
    let final_offset = offset.max(0);
    if let Some(window) = app.get_webview_window("clipboard") {
        set_window_position(&window, final_offset);
    }
    if let Some(window) = app.get_webview_window("image_clipboard") {
        set_window_position(&window, final_offset);
    }
    Ok(())
}

#[tauri::command]
pub async fn save_clipboard_bottom_offset(
    offset: i32,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let final_offset = offset.clamp(0, 400);
    let mut settings = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.settings.clone()
    };
    settings.clipboard_bottom_offset = final_offset;
    save_settings(&settings).map_err(|e| e.to_string())?;

    {
        let mut state_guard = lock_arc_mutex(state.inner());
        state_guard.settings = settings;
    }

    if let Some(window) = app.get_webview_window("clipboard") {
        set_window_position(&window, final_offset);
    }
    if let Some(window) = app.get_webview_window("image_clipboard") {
        set_window_position(&window, final_offset);
    }
    Ok(())
}

#[tauri::command]
pub async fn select_and_fill(
    request: SelectAndFillRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<String, String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_select_and_fill_text(request, state_arc, app).map_err(to_frontend_error_string)
    })
        .await
        .map_err(|e| frontend_error(ErrorCode::SystemError, "文本回填任务执行失败", e.to_string()))?
}

#[tauri::command]
pub async fn remove_clipboard_item(
    index: Option<usize>,
    item: Option<String>,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_remove_clipboard_item(index, item, state_arc, app).map_err(to_frontend_error_string)
    })
        .await
        .map_err(|e| frontend_error(ErrorCode::SystemError, "删除文本历史任务执行失败", e.to_string()))?
}

#[tauri::command]
pub async fn remove_image_clipboard_item_by_id(
    item_id: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_remove_image_clipboard_item_by_id(item_id, state_arc, app)
            .map_err(to_frontend_error_string)
    })
        .await
        .map_err(|e| frontend_error(ErrorCode::SystemError, "删除图片历史任务执行失败", e.to_string()))?
}

#[tauri::command]
pub async fn select_and_fill_image_by_id(
    request: SelectAndFillImageByIdRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_select_and_fill_image_by_id(request, state_arc, app)
            .map_err(to_frontend_error_string)
    })
        .await
        .map_err(|e| frontend_error(ErrorCode::SystemError, "图片回填任务执行失败", e.to_string()))?
}

#[tauri::command]
pub async fn window_blur(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let is_visible = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.is_visible
    };
    if is_visible {
        let state_clone = state.inner().clone();
        hide_clipboard_window(app, state_clone);
    }
    Ok(())
}

#[tauri::command]
pub async fn image_window_blur(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let is_visible = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.is_image_visible
    };
    if is_visible {
        let state_clone = state.inner().clone();
        hide_image_clipboard_window(app, state_clone);
    }
    Ok(())
}

#[tauri::command]
pub async fn selection_toolbar_blur(app: AppHandle) -> Result<(), String> {
    if let Some(toolbar_window) = app.get_webview_window("selection_toolbar") {
        let _ = toolbar_window.hide();
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
        let _ = settings_window.emit("navigate-settings-tab", payload);
    }
    Ok(())
}

fn register_recording_shortcut(
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
        .map_err(|e| frontend_error(ErrorCode::SystemError, "注册录屏快捷键失败", e.to_string()))?;
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
        let _ = tauri::WebviewWindowBuilder::new(
            &app,
            "selection_toolbar",
            tauri::WebviewUrl::App("selection_toolbar.html".into()),
        )
            .title("fuyun_tools")
            .visible(false)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .build()
            .map_err(|e| format!("创建划词工具栏窗口失败: {}", e))?;
        log::info!("show_selection_toolbar_with_text: 已创建selection_toolbar窗口");
    }
    crate::ui::window_manager::show_selection_toolbar_force_impl(
        app.clone(),
        content.clone(),
        Some((x, y)),
    );
    if let Some(toolbar_window) = app.get_webview_window("selection_toolbar") {
        let payload = serde_json::to_string(&content).map_err(|e| format!("序列化文本失败: {}", e))?;
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
        tauri::WebviewWindowBuilder::new(
            &app,
            result_label.clone(),
            tauri::WebviewUrl::App("ocr_text.html".into()),
        )
            .title("OCR识别结果")
            .visible(false)
            .decorations(false)
            .always_on_top(false)
            .resizable(true)
            .inner_size(560.0, 240.0)
            .build()
            .map_err(|e| format!("创建OCR结果窗口失败: {}", e))?
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

    let _ = window.set_size(tauri::PhysicalSize::new(target_width as u32, target_height as u32));
    let _ = window.set_always_on_top(false);
    let _ = window.set_position(tauri::PhysicalPosition::new(target_x, target_y));
    let _ = window.show();
    let _ = window.set_focus();

    let payload = serde_json::json!({"text": content});
    let script = format!(
        "window.__OCR_TEXT_PAYLOAD__ = {payload}; window.dispatchEvent(new CustomEvent('ocr-text-data', {{ detail: {payload} }}));"
    );
    let _ = window.eval(&script);
    Ok(())
}

#[tauri::command]
pub async fn get_ai_settings() -> Result<HashMap<String, serde_json::Value>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings().map_err(|e| {
            frontend_error(ErrorCode::ConfigError, "读取AI设置失败", e)
        })?;

        // 转换为HashMap格式，便于前端处理
        let mut result = HashMap::new();

        // 添加基本设置
        result.insert(
            "version".to_string(),
            serde_json::Value::String(settings.version.clone()),
        );
        result.insert(
            "max_items".to_string(),
            serde_json::Value::Number(serde_json::Number::from(settings.max_items)),
        );
        result.insert(
            "text_max_items".to_string(),
            serde_json::Value::Number(serde_json::Number::from(settings.text_max_items)),
        );
        result.insert(
            "image_max_items".to_string(),
            serde_json::Value::Number(serde_json::Number::from(settings.image_max_items)),
        );
        result.insert(
            "image_disk_limit_mb".to_string(),
            serde_json::Value::Number(serde_json::Number::from(settings.image_disk_limit_mb)),
        );
        result.insert(
            "ai_provider".to_string(),
            serde_json::Value::String(settings.ai_provider.clone()),
        );
        result.insert(
            "hot_key".to_string(),
            serde_json::Value::String(settings.hot_key.clone()),
        );
        result.insert(
            "text_clipboard_enabled".to_string(),
            serde_json::Value::Bool(settings.text_clipboard_enabled),
        );
        result.insert(
            "image_hot_key".to_string(),
            serde_json::Value::String(settings.image_hot_key.clone()),
        );
        result.insert(
            "image_clipboard_enabled".to_string(),
            serde_json::Value::Bool(settings.image_clipboard_enabled),
        );
        result.insert(
            "screenshot_hot_key".to_string(),
            serde_json::Value::String(settings.screenshot_hot_key.clone()),
        );
        result.insert(
            "screenshot_enabled".to_string(),
            serde_json::Value::Bool(settings.screenshot_enabled),
        );
        result.insert(
            "recording_hot_key".to_string(),
            serde_json::Value::String(settings.recording_hot_key.clone()),
        );
        result.insert(
            "recording_enabled".to_string(),
            serde_json::Value::Bool(settings.recording_enabled),
        );
        result.insert(
            "recording_default_fps".to_string(),
            serde_json::Value::Number(serde_json::Number::from(settings.recording_default_fps)),
        );
        result.insert(
            "recording_default_video_bitrate_kbps".to_string(),
            serde_json::Value::Number(serde_json::Number::from(
                settings.recording_default_video_bitrate_kbps,
            )),
        );
        result.insert(
            "recording_default_audio_bitrate_kbps".to_string(),
            serde_json::Value::Number(serde_json::Number::from(
                settings.recording_default_audio_bitrate_kbps,
            )),
        );
        result.insert(
            "recording_capture_cursor".to_string(),
            serde_json::Value::Bool(settings.recording_capture_cursor),
        );
        result.insert(
            "recording_capture_system_audio".to_string(),
            serde_json::Value::Bool(settings.recording_capture_system_audio),
        );
        result.insert(
            "recording_capture_microphone".to_string(),
            serde_json::Value::Bool(settings.recording_capture_microphone),
        );
        result.insert(
            "recording_microphone_device_id".to_string(),
            serde_json::Value::String(settings.recording_microphone_device_id.clone()),
        );
        result.insert(
            "recording_output_dir".to_string(),
            serde_json::Value::String(settings.recording_output_dir.clone()),
        );
        result.insert(
            "recording_auto_open_folder".to_string(),
            serde_json::Value::Bool(settings.recording_auto_open_folder),
        );
        result.insert(
            "recording_toolbar_content_protected".to_string(),
            serde_json::Value::Bool(settings.recording_toolbar_content_protected),
        );
        result.insert(
            "recording_max_duration_minutes".to_string(),
            serde_json::Value::Number(serde_json::Number::from(
                settings.recording_max_duration_minutes,
            )),
        );
        result.insert(
            "recording_file_name_template".to_string(),
            serde_json::Value::String(settings.recording_file_name_template.clone()),
        );
        result.insert(
            "recording_ffmpeg_download_url".to_string(),
            serde_json::Value::String(settings.recording_ffmpeg_download_url.clone()),
        );
        result.insert(
            "recording_window_audio_sync_advance_ms".to_string(),
            serde_json::Value::Number(serde_json::Number::from(
                settings.recording_window_audio_sync_advance_ms,
            )),
        );
        result.insert(
            "selection_enabled".to_string(),
            serde_json::Value::Bool(settings.selection_enabled),
        );
        result.insert(
            "grouped_items_protected_from_limit".to_string(),
            serde_json::Value::Bool(settings.grouped_items_protected_from_limit),
        );
        result.insert(
            "translation_prompt_template".to_string(),
            serde_json::Value::String(settings.translation_prompt_template.clone()),
        );
        result.insert(
            "explanation_prompt_template".to_string(),
            serde_json::Value::String(settings.explanation_prompt_template.clone()),
        );
        result.insert(
            "image_fill_verify_mode".to_string(),
            serde_json::Value::String(settings.image_fill_verify_mode.clone()),
        );

        // 处理provider_configs，将encrypted_api_key替换为解密后的api_key
        let mut provider_configs_map: HashMap<String, serde_json::Value> = HashMap::new();

        let provider_keys: Vec<String> = settings.provider_configs.keys().cloned().collect();

        for provider_key in provider_keys.iter() {
            if let Ok(api_key) = settings.get_provider_api_key(provider_key) {
                if let Some(decrypted_config) = settings.provider_configs.get(provider_key) {
                    let mut config_map = HashMap::new();
                    config_map.insert(
                        "api_url".to_string(),
                        serde_json::Value::String(decrypted_config.api_url.clone()),
                    );
                    config_map.insert(
                        "model_name".to_string(),
                        serde_json::Value::String(decrypted_config.model_name.clone()),
                    );
                    config_map.insert(
                        "api_key".to_string(),
                        serde_json::Value::String(if api_key.is_empty() {
                            "".to_string()
                        } else {
                            "********".to_string()
                        }),
                    );

                    provider_configs_map.insert(
                        provider_key.clone(),
                        serde_json::Value::Object(config_map.into_iter().collect()),
                    );
                }
            }
        }

        result.insert(
            "provider_configs".to_string(),
            serde_json::Value::Object(provider_configs_map.into_iter().collect()),
        );

        Ok(result)
    })
    .await
    .map_err(|e| frontend_error(ErrorCode::SystemError, "读取AI设置任务执行失败", e.to_string()))?
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_text_dedup_metrics() -> Result<serde_json::Value, String> {
    serde_json::to_value(get_dedup_scan_metrics()).map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "序列化去重指标失败",
            e.to_string(),
        )
    })
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
    text_clipboard_enabled: Option<bool>,
    image_clipboard_enabled: Option<bool>,
    screenshot_enabled: Option<bool>,
    recording_enabled: Option<bool>,
    selection_enabled: Option<bool>,
    grouped_items_protected_from_limit: Option<bool>,
    translation_prompt_template: Option<String>,
    explanation_prompt_template: Option<String>,
    image_fill_verify_mode: Option<String>,
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

    // 部分更新：只更新传入的字段
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

    // 处理快捷键更新
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
                log::warn!("注销旧快捷键 '{}' 失败 (可能从未注册成功): {}", old_hot_key, e);
            }
            settings.hot_key = hot_key_val.clone();
        }
    }

    // 处理图片快捷键更新
    if let Some(ref image_hot_key_val) = image_hot_key {
        if image_hot_key_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "图片窗口快捷键不能为空",
                "image_hot_key is empty",
            ));
        }

        if image_hot_key_val != &settings.image_hot_key {
            // 检查是否与文字快捷键冲突
            if let Some(ref hot_key_val) = hot_key {
                if image_hot_key_val == hot_key_val {
                    return Err(frontend_error(
                        ErrorCode::ValidationError,
                        "文字与图片窗口快捷键不能相同",
                        format!("hot_key={}, image_hot_key={}", hot_key_val, image_hot_key_val),
                    ));
                }
            } else if image_hot_key_val == &settings.hot_key {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    "文字与图片窗口快捷键不能相同",
                    format!("hot_key={}, image_hot_key={}", settings.hot_key, image_hot_key_val),
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
                register_recording_shortcut(&app, state.inner().clone(), recording_hot_key_val.as_str())?;
            }
            settings.recording_hot_key = recording_hot_key_val.clone();
        }
    }

    if let Some(enabled) = text_clipboard_enabled {
        if enabled {
            if !app.global_shortcut().is_registered(settings.hot_key.as_str()) {
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
                register_image_shortcut(&app, state.inner().clone(), settings.image_hot_key.as_str())?;
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
            log::warn!("注销截图快捷键 '{}' 失败: {}", settings.screenshot_hot_key, e);
        }
    }

    if let Some(enabled) = recording_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.recording_hot_key.as_str())
            {
                register_recording_shortcut(&app, state.inner().clone(), settings.recording_hot_key.as_str())?;
            }
        } else if let Err(e) = app
            .global_shortcut()
            .unregister(settings.recording_hot_key.as_str())
        {
            log::warn!("注销录屏快捷键 '{}' 失败: {}", settings.recording_hot_key, e);
        }
    }

    // 处理 AI 提供商更新
    if let Some(ref ai_provider_val) = ai_provider {
        if ai_provider_val.is_empty() {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "提供商名称不能为空",
                "ai_provider is empty",
            ));
        }
        settings.ai_provider = ai_provider_val.clone();

        // 处理 API 配置
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

        // 处理 API 密钥
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
                        },
                        Ok(_) => {
                            log::warn!("密钥保存验证失败: 读取到的密钥与保存的不一致");
                            return Err(frontend_error(
                                ErrorCode::SystemError,
                                "系统凭据管理器异常: 密钥保存验证失败，请重试",
                                "saved key mismatch",
                            ));
                        },
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

    save_settings(&settings).map_err(|e| frontend_error(ErrorCode::ConfigError, "保存设置失败", e))?;
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
            Err(frontend_error(ErrorCode::NetworkError, "连接测试失败", e.to_string()))
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
            let error_msg = frontend_error(ErrorCode::ClipboardError, "复制文本失败", e.to_string());
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
) -> Result<(), String> {
    if is_duplicate_copy_paste_request(&text, request_id.as_deref()) {
        if COPY_PASTE_DEDUP_LOG_ENABLED.load(Ordering::Relaxed) {
            log::warn!("检测到短时重复回写请求，已跳过执行");
            COPY_PASTE_DEDUP_LOG_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        return Ok(());
    }
    app.clipboard()
        .write_text(text)
        .map_err(|e| frontend_error(ErrorCode::ClipboardError, "复制文本失败", e.to_string()))?;

    if let Some(window) = app.get_webview_window("result_translation") {
        let _ = window.hide();
    }
    if let Some(window) = app.get_webview_window("result_explanation") {
        let _ = window.hide();
    }

    let paste_result = tauri::async_runtime::spawn_blocking(move || {
        let started_at = std::time::Instant::now();
        let is_post_paste_ctrl_release_error = |err: &str| err.contains("释放 Ctrl");
        let retry_delays = [10_u64, 22_u64, 36_u64];
        match crate::ui::window_manager::simulate_paste() {
            Ok(_) => Ok(()),
            Err(first_error) => {
                if is_post_paste_ctrl_release_error(&first_error) {
                    if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                        log::warn!("复制后自动粘贴检测到Ctrl释放异常时兜底释放失败: {}", release_error);
                    }
                    return Err(first_error);
                }
                let mut final_error = first_error.clone();
                for delay in retry_delays {
                    thread::sleep(Duration::from_millis(delay));
                    if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                        log::warn!("复制后自动粘贴重试前释放Ctrl失败: {}", release_error);
                    }
                    match crate::ui::window_manager::simulate_paste() {
                        Ok(_) => {
                            log::warn!(
                                "复制后自动粘贴首次失败后重试成功: 首次错误={}, 总耗时={}ms",
                                first_error,
                                started_at.elapsed().as_millis()
                            );
                            return Ok(());
                        }
                        Err(next_error) => {
                            final_error = next_error;
                            if is_post_paste_ctrl_release_error(&final_error) {
                                if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                                    log::warn!("复制后自动粘贴检测到Ctrl释放异常时兜底释放失败: {}", release_error);
                                }
                                return Err(final_error);
                            }
                        }
                    }
                }
                if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                    log::warn!("复制后自动粘贴最终兜底释放Ctrl失败: {}", release_error);
                }
                Err(format!("首次错误: {}，最终错误: {}", first_error, final_error))
            }
        }
    })
    .await
    .map_err(|e| frontend_error(ErrorCode::SystemError, "自动粘贴任务执行失败", e.to_string()))?;
    if let Err(e) = paste_result {
        if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
            log::warn!("复制后自动粘贴失败时兜底释放Ctrl失败: {}", release_error);
        }
        return Err(frontend_error(ErrorCode::ClipboardError, "自动粘贴失败", e));
    }
    Ok(())
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

    save_settings(&settings).map_err(|e| frontend_error(ErrorCode::ConfigError, "保存设置失败", e))?;

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
        .map_err(|e| frontend_error(ErrorCode::SystemError, "检查预览状态任务执行失败", e.to_string()))?
}

// ========================================
// 截图相关命令
// ========================================

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualLongshotSessionRequest {
    session_id: u64,
}

#[tauri::command]
pub async fn check_vc_runtime_dependencies() -> Result<serde_json::Value, String> {
    #[cfg(windows)]
    {
        let win_dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
        let system32 = PathBuf::from(win_dir).join("System32");
        let app_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|x| x.to_path_buf()));
        let required = ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"];
        let missing: Vec<String> = required
            .iter()
            .filter_map(|name| {
                let in_system32 = system32.join(name).exists();
                let in_app_dir = app_dir
                    .as_ref()
                    .map(|dir| dir.join(name).exists())
                    .unwrap_or(false);
                if in_system32 || in_app_dir {
                    None
                } else {
                    Some((*name).to_string())
                }
            })
            .collect();
        #[cfg(debug_assertions)]
        if VC_RUNTIME_FORCE_MISSING.load(Ordering::Relaxed) {
            return Ok(serde_json::json!({
                "ok": false,
                "missing": required,
                "installUrl": "https://aka.ms/vs/17/release/vc_redist.x64.exe",
                "forcedByDev": true
            }));
        }
        return Ok(serde_json::json!({
            "ok": missing.is_empty(),
            "missing": missing,
            "installUrl": "https://aka.ms/vs/17/release/vc_redist.x64.exe",
            "forcedByDev": false
        }));
    }
    #[cfg(not(windows))]
    {
        Ok(serde_json::json!({
            "ok": true,
            "missing": [],
            "installUrl": ""
        }))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct VcRuntimeDownloadProgress {
    phase: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    progress_percent: Option<u8>,
    message: String,
}

fn normalize_sha256_hex(raw: &str) -> Option<String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(value)
    } else {
        None
    }
}

fn split_download_url_and_sha256(raw: &str) -> Result<(String, Option<String>), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("下载地址不能为空".to_string());
    }
    if let Some((url, fragment)) = trimmed.split_once("#sha256=") {
        let expected = normalize_sha256_hex(fragment)
            .ok_or_else(|| "下载地址中的 sha256 参数格式无效（应为64位十六进制）".to_string())?;
        return Ok((url.trim().to_string(), Some(expected)));
    }
    Ok((trimmed.to_string(), None))
}

fn compute_file_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("读取下载文件失败: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("读取下载文件失败: {}", e))?;
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

fn verify_downloaded_exe_integrity(path: &Path, expected_sha256: Option<&str>) -> Result<(), String> {
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

fn validate_vc_runtime_installer_path(installer_path: &str) -> Result<PathBuf, String> {
    let raw = installer_path.trim();
    if raw.is_empty() {
        return Err("安装包路径不能为空".to_string());
    }
    let path = PathBuf::from(raw);
    if !path.exists() || !path.is_file() {
        return Err("安装包文件不存在，请重新下载".to_string());
    }
    let canonical = fs::canonicalize(&path).map_err(|e| format!("解析安装包路径失败: {}", e))?;
    let file_name = canonical
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if file_name != "vc_redist.x64.exe" {
        return Err("安装包文件名不合法，拒绝执行".to_string());
    }
    let allowed_root = fs::canonicalize(std::env::temp_dir().join("fuyun_tools"))
        .map_err(|e| format!("解析安装目录失败: {}", e))?;
    if !canonical.starts_with(&allowed_root) {
        return Err("安装包路径不在受信任目录，拒绝执行".to_string());
    }
    Ok(canonical)
}

#[tauri::command]
pub async fn download_vc_runtime_installer(
    download_url: Option<String>,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    #[cfg(windows)]
    {
        let default_url = "https://aka.ms/vs/17/release/vc_redist.x64.exe".to_string();
        let raw_url = download_url
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .unwrap_or(default_url);
        let (url, expected_sha256) = split_download_url_and_sha256(&raw_url)?;
        let parsed = reqwest::Url::parse(&url).map_err(|e| format!("下载地址无效: {}", e))?;
        if parsed.scheme() != "https" {
            return Err("下载地址必须使用 HTTPS".to_string());
        }
        let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
        if expected_sha256.is_none() && host != "aka.ms" {
            return Err("未提供 sha256 时，仅允许从 aka.ms 下载 VC Runtime".to_string());
        }
        let target_dir = std::env::temp_dir().join("fuyun_tools");
        fs::create_dir_all(&target_dir).map_err(|e| format!("创建目录失败: {}", e))?;
        let installer_path = target_dir.join("vc_redist.x64.exe");
        let tmp_path = target_dir.join("vc_redist.x64.exe.tmp");
        if tmp_path.exists() {
            let _ = fs::remove_file(&tmp_path);
        }

        let _ = app.emit(
            "vc-runtime-download-progress",
            VcRuntimeDownloadProgress {
                phase: "start".to_string(),
                downloaded_bytes: 0,
                total_bytes: None,
                progress_percent: Some(0),
                message: "开始下载 VC Runtime 安装包".to_string(),
            },
        );

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("下载请求失败: {}", e))?;
        if !response.status().is_success() {
            return Err(format!(
                "下载 VC Runtime 失败，HTTP 状态: {}",
                response.status()
            ));
        }
        let total_bytes = response.content_length();
        let mut downloaded_bytes: u64 = 0;
        let mut stream = response.bytes_stream();
        let mut file = fs::File::create(&tmp_path).map_err(|e| format!("创建临时文件失败: {}", e))?;

        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res.map_err(|e| format!("下载数据流失败: {}", e))?;
            file.write_all(&chunk)
                .map_err(|e| format!("写入临时文件失败: {}", e))?;
            downloaded_bytes = downloaded_bytes.saturating_add(chunk.len() as u64);
            let progress_percent = total_bytes.and_then(|total| {
                if total == 0 {
                    None
                } else {
                    Some(((downloaded_bytes.saturating_mul(100)) / total).min(100) as u8)
                }
            });
            let _ = app.emit(
                "vc-runtime-download-progress",
                VcRuntimeDownloadProgress {
                    phase: "downloading".to_string(),
                    downloaded_bytes,
                    total_bytes,
                    progress_percent,
                    message: "正在下载 VC Runtime 安装包".to_string(),
                },
            );
        }
        file.flush().map_err(|e| format!("刷新下载文件失败: {}", e))?;
        let metadata = fs::metadata(&tmp_path).map_err(|e| format!("读取下载文件失败: {}", e))?;
        if metadata.len() == 0 {
            let _ = fs::remove_file(&tmp_path);
            return Err("下载结果为空文件，请重试".to_string());
        }
        verify_downloaded_exe_integrity(&tmp_path, expected_sha256.as_deref()).inspect_err(|_| {
            let _ = fs::remove_file(&tmp_path);
        })?;
        fs::rename(&tmp_path, &installer_path)
            .or_else(|_| {
                if installer_path.exists() {
                    let _ = fs::remove_file(&installer_path);
                }
                fs::rename(&tmp_path, &installer_path)
            })
            .map_err(|e| format!("写入安装包失败: {}", e))?;

        let _ = app.emit(
            "vc-runtime-download-progress",
            VcRuntimeDownloadProgress {
                phase: "completed".to_string(),
                downloaded_bytes,
                total_bytes,
                progress_percent: Some(100),
                message: "VC Runtime 安装包下载完成".to_string(),
            },
        );

        return Ok(serde_json::json!({
            "installerPath": installer_path.to_string_lossy().to_string(),
            "downloadUrl": url
        }));
    }
    #[cfg(not(windows))]
    {
        Err("当前平台不支持 VC Runtime 下载".to_string())
    }
}

#[tauri::command]
pub async fn open_vc_runtime_installer(installer_path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        let path = validate_vc_runtime_installer_path(&installer_path)?;
        std::process::Command::new(&path)
            .spawn()
            .map_err(|e| format!("启动安装程序失败: {}", e))?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        Err("当前平台不支持该操作".to_string())
    }
}

#[tauri::command]
pub async fn install_vc_runtime_and_wait(installer_path: String) -> Result<serde_json::Value, String> {
    #[cfg(windows)]
    {
        let path = validate_vc_runtime_installer_path(&installer_path)?;
        let status = tauri::async_runtime::spawn_blocking(move || {
            std::process::Command::new(&path)
                .arg("/install")
                .arg("/passive")
                .arg("/norestart")
                .status()
        })
            .await
            .map_err(|e| format!("启动安装程序失败: {}", e))?
            .map_err(|e| format!("执行安装程序失败: {}", e))?;
        let exit_code = status.code().unwrap_or(-1);
        let success = matches!(exit_code, 0 | 1638 | 3010);
        let cancelled = exit_code == 1602;
        let reboot_required = exit_code == 3010;
        return Ok(serde_json::json!({
            "success": success,
            "cancelled": cancelled,
            "rebootRequired": reboot_required,
            "exitCode": exit_code
        }));
    }
    #[cfg(not(windows))]
    {
        Err("当前平台不支持该操作".to_string())
    }
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_vc_runtime_debug_state() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "forceMissing": VC_RUNTIME_FORCE_MISSING.load(Ordering::Relaxed)
    }))
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn set_vc_runtime_debug_config(force_missing: Option<bool>) -> Result<serde_json::Value, String> {
    if let Some(enabled) = force_missing {
        VC_RUNTIME_FORCE_MISSING.store(enabled, Ordering::Relaxed);
    }
    Ok(serde_json::json!({
        "forceMissing": VC_RUNTIME_FORCE_MISSING.load(Ordering::Relaxed)
    }))
}

#[tauri::command]
pub async fn copy_image_clipboard_item_to_directory(
    item_id: String,
    target_directory: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let manager_arc = get_image_clipboard_manager_arc(&state_arc);
        let manager = lock_arc_mutex(&manager_arc);
        let source_path = manager.get_preview_image_path_by_id(&item_id)?;
        drop(manager);

        let source = PathBuf::from(&source_path);
        if !source.exists() {
            return Err("源图片文件不存在".to_string());
        }
        let file_name = source
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .ok_or_else(|| "无法解析源文件名".to_string())?
            .to_string();

        let target_dir = PathBuf::from(target_directory.trim());
        if target_dir.as_os_str().is_empty() {
            return Err("目标目录不能为空".to_string());
        }
        fs::create_dir_all(&target_dir).map_err(|e| format!("创建目标目录失败: {}", e))?;

        let stem = Path::new(&file_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image");
        let ext = Path::new(&file_name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("png");

        let mut target_path = target_dir.join(&file_name);
        if target_path.exists() {
            for idx in 1..10000u32 {
                let candidate = target_dir.join(format!("{} ({idx}).{}", stem, ext));
                if !candidate.exists() {
                    target_path = candidate;
                    break;
                }
            }
        }

        fs::copy(&source, &target_path).map_err(|e| format!("复制图片失败: {}", e))?;
        Ok(serde_json::json!({
            "success": true,
            "sourcePath": source.to_string_lossy(),
            "savedPath": target_path.to_string_lossy(),
        }))
    })
        .await
        .map_err(|e| frontend_error(ErrorCode::SystemError, "复制图片任务执行失败", e.to_string()))?
}

/// 开始截图（全屏）
#[tauri::command]
pub async fn start_screenshot(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    use crate::features::screenshot::capture;
    if !is_screenshot_feature_enabled(state.inner()) {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "截图功能已停用",
            "screenshot feature disabled",
        ));
    }

    log::info!("开始全屏截图");

    match capture::capture_full_screen() {
        Ok((rgba, width, height, origin_x, origin_y)) => {
            let png_base64 = capture::rgba_to_base64_png(&rgba, width, height)
                .map_err(|e| format!("转换PNG失败: {}", e))?;

            Ok(serde_json::json!({
                "success": true,
                "width": width,
                "height": height,
                "origin_x": origin_x,
                "origin_y": origin_y,
                "png_base64": png_base64
            }))
        }
        Err(e) => {
            log::error!("截图失败: {}", e);
            Ok(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

#[tauri::command]
pub async fn start_manual_longshot(
    request: crate::features::screenshot::longshot::StartManualLongshotRequest,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    if !is_screenshot_feature_enabled(state.inner()) {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "截图功能已停用",
            "screenshot feature disabled",
        ));
    }
    // 在真正启动采样前先隐藏截图窗，避免首帧录入UI边框
    if let Some(window) = app.get_webview_window("screenshot") {
        let _ = window.hide();
    }
    if let Some(window) = app.get_webview_window("longshot_border") {
        let _ = window.hide();
    }
    tauri::async_runtime::spawn_blocking(|| std::thread::sleep(std::time::Duration::from_millis(90)))
        .await
        .map_err(|e| format!("等待截图窗口隐藏失败: {}", e))?;
    crate::features::screenshot::longshot::start_manual_longshot(app, request)
}

#[tauri::command]
pub async fn pause_manual_longshot(
    request: ManualLongshotSessionRequest,
    app: AppHandle,
) -> Result<(), String> {
    crate::features::screenshot::longshot::pause_manual_longshot(request.session_id, app)
}

#[tauri::command]
pub async fn resume_manual_longshot(
    request: ManualLongshotSessionRequest,
    app: AppHandle,
) -> Result<(), String> {
    crate::features::screenshot::longshot::resume_manual_longshot(request.session_id, app)
}

#[tauri::command]
pub async fn cancel_manual_longshot(
    request: ManualLongshotSessionRequest,
    app: AppHandle,
) -> Result<(), String> {
    let session_id = request.session_id;
    tauri::async_runtime::spawn_blocking(move || {
        crate::features::screenshot::longshot::cancel_manual_longshot(session_id, app)
    })
    .await
    .map_err(|e| format!("取消长截图任务执行失败: {}", e))?
}

#[tauri::command]
pub async fn finish_manual_longshot(
    request: ManualLongshotSessionRequest,
    app: AppHandle,
) -> Result<crate::features::screenshot::longshot::ManualLongshotFinishResult, String> {
    let session_id = request.session_id;
    tauri::async_runtime::spawn_blocking(move || {
        crate::features::screenshot::longshot::finish_manual_longshot(session_id, app)
    })
    .await
    .map_err(|e| format!("完成长截图任务执行失败: {}", e))?
}

#[tauri::command]
pub async fn get_manual_longshot_status(
    request: ManualLongshotSessionRequest,
) -> Result<crate::features::screenshot::longshot::ManualLongshotStatus, String> {
    crate::features::screenshot::longshot::get_manual_longshot_status(request.session_id)
}

#[tauri::command]
pub async fn recognize_image_ocr(png_base64: String) -> Result<serde_json::Value, String> {
    match crate::services::native_ocr::recognize_png_base64(&png_base64).await {
        Ok(result) => Ok(serde_json::json!({
            "success": true,
            "paragraphs": result.paragraphs
        })),
        Err(e) => Ok(serde_json::json!({
            "success": false,
            "error": e
        })),
    }
}

/// 捕获指定区域
#[tauri::command]
pub async fn capture_region(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    use crate::features::screenshot::capture;
    if !is_screenshot_feature_enabled(state.inner()) {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "截图功能已停用",
            "screenshot feature disabled",
        ));
    }

    log::info!("捕获区域: ({}, {}) {}x{}", x, y, width, height);

    if width < 1 || height < 1 {
        return Ok(serde_json::json!({
            "success": false,
            "error": "区域尺寸无效"
        }));
    }

    match capture::capture_screen_region(x, y, width, height) {
        Ok((rgba, w, h)) => {
            let png_base64 = capture::rgba_to_base64_png(&rgba, w, h)
                .map_err(|e| format!("转换PNG失败: {}", e))?;

            Ok(serde_json::json!({
                "success": true,
                "width": w,
                "height": h,
                "png_base64": png_base64
            }))
        }
        Err(e) => {
            log::error!("区域截图失败: {}", e);
            Ok(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

/// 保存截图到文件
#[tauri::command]
pub async fn save_screenshot(
    png_base64: String,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    use base64::Engine;
    use std::time::{SystemTime, UNIX_EPOCH};

    log::info!("保存截图到文件");

    // 解码Base64
    let png_data = base64::engine::general_purpose::STANDARD
        .decode(&png_base64)
        .map_err(|e| format!("Base64解码失败: {}", e))?;

    // 生成文件名
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let filename = format!("screenshot_{}.png", timestamp);

    // 获取保存路径（用户选择）
    app.dialog()
        .file()
        .add_filter("PNG图片", &["png"])
        .set_file_name(&filename)
        .save_file(move |path| {
            match path {
                Some(file_path) => {
                    // 尝试将FilePath转换为PathBuf
                    if let Some(path_buf) = file_path.as_path() {
                        match fs::write(path_buf, &png_data) {
                            Ok(_) => {
                                log::info!("截图已保存到: {}", path_buf.display());
                            }
                            Err(e) => {
                                log::error!("写入文件失败: {}", e);
                            }
                        }
                    } else {
                        log::error!("无法获取保存路径");
                    }
                }
                None => {
                    log::info!("用户取消保存");
                }
            }
        });

    Ok(serde_json::json!({
        "success": true,
        "message": "保存对话框已打开"
    }))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinScreenshotRequest {
    png_base64: String,
    x: Option<f64>,
    y: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
}

#[tauri::command]
pub async fn pin_screenshot_on_screen(
    request: PinScreenshotRequest,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    let label = format!(
        "pinned_image_{}",
        NEXT_PINNED_IMAGE_WINDOW_ID.fetch_add(1, Ordering::Relaxed)
    );
    let x = request.x.unwrap_or(100.0).max(0.0);
    let y = request.y.unwrap_or(100.0).max(0.0);
    let width = request.width.unwrap_or(360.0).max(1.0);
    let height = request.height.unwrap_or(240.0).max(1.0);
    let payload = serde_json::json!({
        "label": label,
        "png_base64": request.png_base64,
        "width": width,
        "height": height
    });
    let payload_init_script = format!("window.__PINNED_IMAGE_PAYLOAD__ = {};", payload);
    let window = tauri::WebviewWindowBuilder::new(
        &app,
        label.clone(),
        tauri::WebviewUrl::App("pinned_image.html".into()),
    )
        .title("固定截图")
        .visible(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(true)
        .initialization_script(&payload_init_script)
        .build()
        .map_err(|e| format!("创建固定图片窗口失败: {}", e))?;

    let window_clone = window.clone();
    let _ = window_clone.set_resizable(true);
    let _ = window_clone.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
    let _ = window_clone.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
    let _ = window_clone.show();
    let script = format!(
        "window.__PINNED_IMAGE_PAYLOAD__ = {}; window.dispatchEvent(new CustomEvent('pinned-image-data', {{ detail: {} }}));",
        payload, payload
    );
    let _ = window_clone.eval(script);

    Ok(serde_json::json!({ "success": true, "label": label }))
}

#[tauri::command]
pub async fn close_pinned_image_window(label: String, app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.close();
    }
    Ok(())
}

#[tauri::command]
pub async fn get_pinned_image_window_position(
    label: String,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    if let Some(window) = app.get_webview_window(&label) {
        if let Ok(pos) = window.outer_position() {
            return Ok(serde_json::json!({
                "success": true,
                "x": pos.x,
                "y": pos.y
            }));
        }
    }
    Ok(serde_json::json!({
        "success": false
    }))
}

#[tauri::command]
pub async fn move_pinned_image_window(
    label: String,
    x: i32,
    y: i32,
    app: AppHandle,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
    }
    Ok(())
}

/// 获取屏幕尺寸
#[tauri::command]
pub async fn get_screen_size() -> Result<serde_json::Value, String> {
    use crate::features::screenshot::capture;

    match capture::get_screen_size() {
        Ok((width, height)) => {
            Ok(serde_json::json!({
                "success": true,
                "width": width,
                "height": height
            }))
        }
        Err(e) => {
            Ok(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

#[tauri::command]
pub async fn set_screenshot_clipboard_link_once(linked: bool) -> Result<(), String> {
    use crate::features::screenshot::capture;
    if let Ok(settings) = load_settings() {
        if !settings.screenshot_enabled {
            return Ok(());
        }
    }
    capture::set_allow_image_clipboard_once(linked);
    Ok(())
}

fn set_screenshot_window_passthrough_internal(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let Some(window) = app.get_webview_window("screenshot") else {
        return Ok(());
    };
    window
        .set_ignore_cursor_events(enabled)
        .map_err(|e| format!("设置截图窗口输入穿透失败: {}", e))?;
    if !enabled {
        let _ = window.set_focus();
    }
    Ok(())
}

fn set_screenshot_window_visibility_internal(app: &AppHandle, visible: bool) -> Result<(), String> {
    let Some(window) = app.get_webview_window("screenshot") else {
        return Ok(());
    };
    if visible {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        let _ = window.hide();
    }
    Ok(())
}

fn ensure_longshot_toolbar_window(app: &AppHandle) -> Result<(tauri::WebviewWindow, bool), String> {
    let label = "longshot_toolbar";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok((existing, false));
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("longshot_toolbar.html".into()),
    )
    .title("长截图工具栏")
    .visible(false)
    .resizable(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .inner_size(320.0, 180.0)
    .build()
    .map_err(|e| format!("创建长截图工具栏窗口失败: {}", e))?;
    Ok((window, true))
}

fn ensure_longshot_border_window(app: &AppHandle) -> Result<(tauri::WebviewWindow, bool), String> {
    let label = "longshot_border";
    if let Some(existing) = app.get_webview_window(label) {
        return Ok((existing, false));
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("longshot_border.html".into()),
    )
    .title("长截图边框")
    .visible(false)
    .resizable(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .build()
    .map_err(|e| format!("创建长截图边框窗口失败: {}", e))?;
    let _ = window.set_ignore_cursor_events(true);
    Ok((window, true))
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LongshotToolbarAnchor {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn place_longshot_toolbar_near_anchor(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    anchor: Option<LongshotToolbarAnchor>,
) {
    let Some(anchor) = anchor else {
        let _ = window.move_window(tauri_plugin_positioner::Position::TopRight);
        return;
    };
    let (toolbar_w, toolbar_h) = (260i32, 430i32);
    let default_x = anchor.x + anchor.width as i32 + 12;
    let default_y = anchor.y + (anchor.height as i32 / 2) - (toolbar_h / 2);
    let Some(screen_window) = app.get_webview_window("screenshot") else {
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            default_x, default_y,
        )));
        return;
    };
    let Ok(Some(monitor)) = screen_window.current_monitor() else {
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            default_x, default_y,
        )));
        return;
    };
    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let min_x = mon_pos.x + 8;
    let max_x = mon_pos.x + mon_size.width as i32 - toolbar_w - 8;
    let min_y = mon_pos.y + 8;
    let max_y = mon_pos.y + mon_size.height as i32 - toolbar_h - 8;

    let mut x = default_x;
    if x > max_x {
        x = anchor.x - toolbar_w - 12;
    }
    if x < min_x {
        x = min_x;
    }
    let y = default_y.clamp(min_y, max_y);
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x, y)));
}

#[tauri::command]
pub async fn set_screenshot_input_passthrough(enabled: bool, app: AppHandle) -> Result<(), String> {
    set_screenshot_window_passthrough_internal(&app, enabled)
}

#[tauri::command]
pub async fn set_screenshot_window_visible(visible: bool, app: AppHandle) -> Result<(), String> {
    set_screenshot_window_visibility_internal(&app, visible)
}

#[tauri::command]
pub async fn show_longshot_toolbar(
    app: AppHandle,
    anchor: Option<LongshotToolbarAnchor>,
) -> Result<(), String> {
    let (window, _created) = ensure_longshot_toolbar_window(&app)?;
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
        width: 260.0,
        height: 430.0,
    }));
    place_longshot_toolbar_near_anchor(&app, &window, anchor);
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
pub async fn show_longshot_border(
    app: AppHandle,
    anchor: LongshotToolbarAnchor,
) -> Result<(), String> {
    let (window, _created) = ensure_longshot_border_window(&app)?;
    // 边框窗外扩，确保边框不进入实际采集区域
    const BORDER_OUTSET: i32 = 2;
    let width = (anchor.width as i32 + BORDER_OUTSET * 2).max(2) as u32;
    let height = (anchor.height as i32 + BORDER_OUTSET * 2).max(2) as u32;
    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        anchor.x - BORDER_OUTSET,
        anchor.y - BORDER_OUTSET,
    )));
    let _ = window.show();
    Ok(())
}

#[tauri::command]
pub async fn hide_longshot_border(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("longshot_border") {
        let _ = window.hide();
    }
    Ok(())
}

#[tauri::command]
pub async fn snap_longshot_toolbar_window(app: AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("longshot_toolbar") else {
        return Ok(());
    };
    let Ok(pos) = window.outer_position() else {
        return Ok(());
    };
    let Ok(size) = window.outer_size() else {
        return Ok(());
    };
    let Ok(Some(monitor)) = window.current_monitor() else {
        return Ok(());
    };
    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let left = mon_pos.x + 8;
    let right = mon_pos.x + mon_size.width as i32 - size.width as i32 - 8;
    let top = mon_pos.y + 8;
    let threshold = 28;

    let mut next_x = pos.x;
    let mut next_y = pos.y;
    if (pos.x - left).abs() <= threshold {
        next_x = left;
    } else if (pos.x - right).abs() <= threshold {
        next_x = right;
    }
    if (pos.y - top).abs() <= threshold {
        next_y = top;
    }
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        next_x, next_y,
    )));
    Ok(())
}

#[tauri::command]
pub async fn hide_longshot_toolbar(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("longshot_toolbar") {
        let _ = window.hide();
    }
    Ok(())
}

#[tauri::command]
pub async fn longshot_toolbar_action(action: String, app: AppHandle) -> Result<(), String> {
    let Some(session_id) = crate::features::screenshot::longshot::active_manual_longshot_session_id() else {
        return Ok(());
    };
    match action.as_str() {
        "pause" => {
            crate::features::screenshot::longshot::pause_manual_longshot(session_id, app.clone())?;
        }
        "resume" => {
            crate::features::screenshot::longshot::resume_manual_longshot(session_id, app.clone())?;
        }
        "finish" => {
            let app_for_finish = app.clone();
            let result = tauri::async_runtime::spawn_blocking(move || {
                crate::features::screenshot::longshot::finish_manual_longshot(session_id, app_for_finish)
            })
            .await
            .map_err(|e| format!("完成长截图任务执行失败: {}", e))??;
            let _ = app.emit(
                "manual-longshot-shortcut-finished",
                serde_json::json!({
                    "sessionId": result.session_id,
                    "pngBase64": result.png_base64,
                    "width": result.width,
                    "height": result.height,
                    "frameCount": result.frame_count,
                    "droppedFrames": result.dropped_frames,
                }),
            );
            let _ = hide_longshot_border(app.clone()).await;
            let _ = hide_longshot_toolbar(app.clone()).await;
            let _ = set_screenshot_window_visibility_internal(&app, true);
            return Ok(());
        }
        "cancel" => {
            let app_for_cancel = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                crate::features::screenshot::longshot::cancel_manual_longshot(session_id, app_for_cancel)
            })
            .await
            .map_err(|e| format!("取消长截图任务执行失败: {}", e))??;
            let _ = app.emit(
                "manual-longshot-shortcut-canceled",
                serde_json::json!({
                    "sessionId": session_id
                }),
            );
            let _ = hide_longshot_border(app.clone()).await;
            let _ = hide_longshot_toolbar(app.clone()).await;
            let _ = set_screenshot_window_visibility_internal(&app, true);
            return Ok(());
        }
        _ => return Err("不支持的长截图操作".to_string()),
    }
    Ok(())
}

pub async fn finish_manual_longshot_from_shortcut(app: AppHandle) -> Result<(), String> {
    let Some(session_id) = crate::features::screenshot::longshot::active_manual_longshot_session_id() else {
        return Ok(());
    };
    let app_for_finish = app.clone();
    match tauri::async_runtime::spawn_blocking(move || {
        crate::features::screenshot::longshot::finish_manual_longshot(session_id, app_for_finish)
    })
    .await
    .map_err(|e| format!("完成长截图任务执行失败: {}", e))?
    {
        Ok(result) => {
            let _ = app.emit(
                "manual-longshot-shortcut-finished",
                serde_json::json!({
                    "sessionId": result.session_id,
                    "pngBase64": result.png_base64,
                    "width": result.width,
                    "height": result.height,
                    "frameCount": result.frame_count,
                    "droppedFrames": result.dropped_frames,
                }),
            );
            let _ = hide_longshot_border(app.clone()).await;
            let _ = hide_longshot_toolbar(app.clone()).await;
            let _ = set_screenshot_window_visibility_internal(&app, true);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

pub async fn cancel_manual_longshot_from_shortcut(app: AppHandle) -> Result<(), String> {
    let Some(session_id) = crate::features::screenshot::longshot::active_manual_longshot_session_id() else {
        return Ok(());
    };
    let app_for_cancel = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::features::screenshot::longshot::cancel_manual_longshot(session_id, app_for_cancel)
    })
    .await
    .map_err(|e| format!("取消长截图任务执行失败: {}", e))??;
    let _ = app.emit(
        "manual-longshot-shortcut-canceled",
        serde_json::json!({
            "sessionId": session_id
        }),
    );
    let _ = hide_longshot_border(app.clone()).await;
    let _ = hide_longshot_toolbar(app.clone()).await;
    let _ = set_screenshot_window_visibility_internal(&app, true);
    Ok(())
}

pub async fn toggle_manual_longshot_pause_from_shortcut(app: AppHandle) -> Result<(), String> {
    let Some(session_id) = crate::features::screenshot::longshot::active_manual_longshot_session_id() else {
        return Ok(());
    };
    let status = crate::features::screenshot::longshot::get_manual_longshot_status(session_id)?;
    if status.state == "running" {
        crate::features::screenshot::longshot::pause_manual_longshot(session_id, app.clone())?;
        let _ = app.emit(
            "manual-longshot-shortcut-paused",
            serde_json::json!({
                "sessionId": session_id
            }),
        );
        return Ok(());
    }
    if status.state == "paused" {
        crate::features::screenshot::longshot::resume_manual_longshot(session_id, app.clone())?;
        let _ = app.emit(
            "manual-longshot-shortcut-resumed",
            serde_json::json!({
                "sessionId": session_id
            }),
        );
    }
    Ok(())
}

/// 打开截图编辑窗口
#[tauri::command]
pub async fn open_screenshot_editor(app: AppHandle, mode: Option<String>) -> Result<(), String> {
    let selection_mode = mode
        .as_ref()
        .map(|m| m.to_lowercase())
        .unwrap_or_else(|| "screenshot".to_string());
    if let Ok(settings) = load_settings() {
        if !settings.screenshot_enabled && selection_mode != "recording_region" {
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "截图功能已停用",
                "screenshot feature disabled",
            ));
        }
    }
    log::info!("打开截图编辑窗口");

    use crate::features::screenshot::capture;
    if !capture::try_begin_screenshot() {
        log::info!("截图任务已在进行中，忽略重复触发");
        return Ok(());
    }
    let (rgba, width, height, origin_x, origin_y) = match capture::capture_full_screen() {
        Ok(data) => data,
        Err(e) => {
            capture::set_screenshot_in_progress(false);
            return Err(format!("截图失败: {}", e));
        }
    };

    let png_base64 = capture::rgba_to_base64_png(&rgba, width, height)
        .map_err(|e| {
            capture::set_screenshot_in_progress(false);
            format!("转换PNG失败: {}", e)
        })?;
    let session_id = NEXT_SCREENSHOT_SESSION_ID.fetch_add(1, Ordering::SeqCst);

    let selection_mode = selection_mode;
    if let Some(window) = app.get_webview_window("screenshot") {
        if SCREENSHOT_LIFECYCLE_BOUND_FOR_BOOT_WINDOW
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            bind_screenshot_window_lifecycle(&window);
        }
        let payload = serde_json::json!({
            "png_base64": png_base64,
            "width": width,
            "height": height,
            "origin_x": origin_x,
            "origin_y": origin_y,
            "session_id": session_id
        });
        let script = format!(
            "window.__SCREENSHOT_BOOT__ = window.__SCREENSHOT_BOOT__ || {{ pendingData: null, pendingStartSessionId: 0 }};\
window.__SCREENSHOT_BOOT__.pendingData = {payload};\
window.__SCREENSHOT_BOOT__.pendingStartSessionId = {session_id};\
window.__SCREENSHOT_BOOT__.pendingMode = '{selection_mode}';\
window.dispatchEvent(new CustomEvent('screenshot-data', {{ detail: {payload} }}));\
window.dispatchEvent(new CustomEvent('start-region-select', {{ detail: {{ session_id: {session_id}, mode: '{selection_mode}' }} }}));"
        );

        thread::spawn(move || {
            let _ = window.set_always_on_top(true);
            let _ = window.set_ignore_cursor_events(false);
            let _ = window.set_fullscreen(true);
            let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                x: origin_x,
                y: origin_y,
            }));
            let mut injected = false;
            for _ in 0..20 {
                if window.eval(&script).is_ok() {
                    injected = true;
                    break;
                }
                thread::sleep(Duration::from_millis(25));
            }
            if injected {
                let _ = window.show();
                let _ = window.set_focus();
            } else {
                let _ = window.hide();
                capture::set_screenshot_in_progress(false);
            }
        });
    } else {
        let payload = serde_json::json!({
            "png_base64": png_base64,
            "width": width,
            "height": height,
            "origin_x": origin_x,
            "origin_y": origin_y,
            "session_id": session_id
        });
        let boot_script = format!(
            "window.__SCREENSHOT_BOOT__ = window.__SCREENSHOT_BOOT__ || {{ pendingData: null, pendingStartSessionId: 0 }};\
window.__SCREENSHOT_BOOT__.pendingData = {};\
window.__SCREENSHOT_BOOT__.pendingStartSessionId = {};",
            payload, session_id
        );
        let window = tauri::WebviewWindowBuilder::new(
            &app,
            "screenshot",
            tauri::WebviewUrl::App("screenshot.html".into()),
        )
            .title("截图选择")
            .visible(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .inner_size(width as f64, height as f64)
            .position(origin_x as f64, origin_y as f64)
            .fullscreen(true)
            .on_page_load(move |window, _| {
                let _ = window.eval(&boot_script);
                let _ = window.show();
                let _ = window.set_focus();
            })
            .build()
            .map_err(|e| {
                capture::set_screenshot_in_progress(false);
                format!("创建截图窗口失败: {}", e)
            })?;
        bind_screenshot_window_lifecycle(&window);
        let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: origin_x, y: origin_y }));
    }

    Ok(())
}

/// 获取窗口列表
#[tauri::command]
pub async fn get_window_list() -> Result<serde_json::Value, String> {
    use crate::features::screenshot::window_detect;

    match window_detect::get_window_list() {
        Ok(windows) => {
            Ok(serde_json::json!({
                "success": true,
                "windows": windows
            }))
        }
        Err(e) => {
            log::error!("获取窗口列表失败: {}", e);
            Ok(serde_json::json!({
                "success": false,
                "error": e.to_string(),
                "windows": []
            }))
        }
    }
}

/// 关闭截图窗口并释放焦点
#[tauri::command]
pub async fn close_screenshot_window(app: AppHandle) -> Result<(), String> {
    log::info!("关闭截图窗口");
    if let Some(window) = app.get_webview_window("screenshot") {
        // 解除置顶和鼠标拦截，防止在Windows上残留透明幽灵窗口导致桌面无法点击
        let _ = window.set_always_on_top(false);
        let _ = window.set_ignore_cursor_events(true);
        let _ = window.eval(
            "window.dispatchEvent(new CustomEvent('screenshot-reset'));\
window.__SCREENSHOT_BOOT__ = window.__SCREENSHOT_BOOT__ || { pendingData: null, pendingStartSessionId: 0 };\
window.__SCREENSHOT_BOOT__.pendingData = null;\
window.__SCREENSHOT_BOOT__.pendingStartSessionId = 0;",
        );
        let _ = window.hide();
    }
    features::screenshot::capture::set_screenshot_in_progress(false);

    Ok(())
}
