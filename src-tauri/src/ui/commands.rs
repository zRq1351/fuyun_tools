use crate::core::app_state::AppState as SharedAppState;
use crate::core::config::{AIProvider, ProviderConfig};
use crate::core::error::{to_frontend_error_string, AppError, AppResult, ErrorCode};
use crate::features;
use crate::services::ai_client::{AIClient, AIConfig};
use crate::services::image_clipboard_manager::emit_image_history_payload;
use crate::sync::Mutex;
use crate::ui::window_manager::{
    hide_clipboard_window, hide_image_clipboard_window, hide_image_preview_window, set_window_position,
    show_clipboard_window, show_image_clipboard_window, show_image_preview_loading_window,
    show_image_preview_window,
};
use crate::utils::clipboard::ClipboardManager;
use crate::utils::image_clipboard::{
    is_fast_fill_verify_mode_enabled, set_image_fill_verify_mode, ImageClipboardManager,
    ImageHistoryPageData, ImageHistoryPreviewItem,
};
use crate::utils::utils_helpers::{
    default_explanation_prompt_template, default_translation_prompt_template, get_dedup_scan_metrics,
    load_history_page_data_async, load_settings, save_settings, ClipboardHistoryPageData,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

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
    Ok(
        manager
            .get_history_preview_page_async(
                request.offset,
                request.limit,
                request.category,
                request.keyword,
                request.pinned_only,
                request.sort_by,
                request.sort_order,
            )
            .await,
    )
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

fn lock_arc_mutex<'a, T>(mutex: &'a Arc<Mutex<T>>) -> crate::sync::MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(e) => match e {},
    }
}

fn lock_mutex<'a, T>(mutex: &'a Mutex<T>) -> crate::sync::MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(e) => match e {},
    }
}

fn begin_fill_sequence(state: &Arc<Mutex<SharedAppState>>, kind: FillKind) -> u64 {
    let mut state_guard = lock_arc_mutex(state);
    state_guard.is_updating_clipboard = true;
    state_guard.is_processing_selection = true;
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

static IMAGE_PROMOTE_PENDING_ID: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static IMAGE_PROMOTE_WORKER_RUNNING: AtomicBool = AtomicBool::new(false);

fn get_image_promote_pending_id_slot() -> &'static Mutex<Option<String>> {
    IMAGE_PROMOTE_PENDING_ID.get_or_init(|| Mutex::new(None))
}

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
    let slot = get_image_promote_pending_id_slot();
    let mut guard = lock_mutex(slot);
    *guard = None;
}

fn schedule_image_promote_to_top(state: Arc<Mutex<SharedAppState>>, item_id: String) {
    {
        let slot = get_image_promote_pending_id_slot();
        let mut guard = lock_mutex(slot);
        *guard = Some(item_id);
    }
    if IMAGE_PROMOTE_WORKER_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(120));
            let next_item_id = {
                let slot = get_image_promote_pending_id_slot();
                let mut guard = lock_mutex(slot);
                guard.take()
            };
            if let Some(item_id) = next_item_id {
                let manager_arc = {
                    let state_guard = lock_arc_mutex(&state);
                    state_guard.image_clipboard_manager.clone()
                };
                let manager = lock_arc_mutex(&manager_arc);
                if let Err(e) = manager.promote_to_top_by_id(&item_id) {
                    log::warn!("极速模式异步置顶图片失败: {}", e);
                }
                continue;
            }
            IMAGE_PROMOTE_WORKER_RUNNING.store(false, Ordering::SeqCst);
            let has_pending = {
                let slot = get_image_promote_pending_id_slot();
                let guard = lock_mutex(slot);
                guard.is_some()
            };
            if has_pending
                && IMAGE_PROMOTE_WORKER_RUNNING
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                continue;
            }
            break;
        }
    });
}

fn wait_for_fill_window_hidden(app: &AppHandle, window_label: &str, label: &str, fast_path: bool) {
    let timeout_ms = if fast_path { 220 } else { 900 };
    if let Err(e) = crate::ui::window_manager::wait_for_window_hidden(
        app,
        window_label,
        Duration::from_millis(timeout_ms),
    ) {
        log::warn!("等待{}窗口隐藏失败: {}", label, e);
    } else if !fast_path {
        thread::sleep(Duration::from_millis(40));
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
    if fast_path {
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
                return;
            }
            Err(first_error) => {
                // 优化方案 4：极速模式下快速重试，仅等待 15ms
                thread::sleep(Duration::from_millis(15));
                match crate::ui::window_manager::simulate_paste() {
                    Ok(_) => {
                        if let Some(op_id) = operation_id {
                            log::warn!(
                                "{}回填极速模式首次粘贴失败，快速重试成功: op_id={}, {}，总耗时: {}ms",
                                label,
                                op_id,
                                first_error,
                                started_at.elapsed().as_millis()
                            );
                        } else {
                            log::warn!(
                                "{}回填极速模式首次粘贴失败，快速重试成功: {}，总耗时: {}ms",
                                label,
                                first_error,
                                started_at.elapsed().as_millis()
                            );
                        }
                        return;
                    }
                    Err(second_error) => {
                        if let Some(op_id) = operation_id {
                            log::error!(
                                "{}回填极速模式粘贴失败: op_id={}, 首次错误: {}，二次错误: {}",
                                label,
                                op_id,
                                first_error,
                                second_error
                            );
                        } else {
                            log::error!(
                                "{}回填极速模式粘贴失败，首次错误: {}，二次错误: {}",
                                label,
                                first_error,
                                second_error
                            );
                        }
                        return;
                    }
                }
            }
        }
    }
    // 优化方案 4：降低普通模式的延迟，从 135ms 降至 90ms
    thread::sleep(Duration::from_millis(90));
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
            // 优化方案 4：二次重试延迟从 140ms 降至 100ms
            thread::sleep(Duration::from_millis(100));
            match crate::ui::window_manager::simulate_paste() {
                Ok(_) => {
                    if let Some(op_id) = operation_id {
                        log::warn!(
                            "{}回填首次粘贴失败，二次重试成功: op_id={}, {}，总耗时: {}ms",
                            label,
                            op_id,
                            first_error,
                            started_at.elapsed().as_millis()
                        );
                    } else {
                        log::warn!(
                            "{}回填首次粘贴失败，二次重试成功: {}，总耗时: {}ms",
                            label,
                            first_error,
                            started_at.elapsed().as_millis()
                        );
                    }
                }
                Err(second_error) => {
                    if let Some(op_id) = operation_id {
                        log::error!(
                            "{}回填粘贴失败: op_id={}, 首次错误: {}，二次错误: {}",
                            label,
                            op_id,
                            first_error,
                            second_error
                        );
                    } else {
                        log::error!(
                            "{}回填粘贴失败，首次错误: {}，二次错误: {}",
                            label,
                            first_error,
                            second_error
                        );
                    }
                }
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
        match crate::utils::image_clipboard::ImageClipboardManager::read_clipboard_images_rgba(app) {
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
            crate::utils::image_clipboard::ImageClipboardManager::write_clipboard_image(app, &image)
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
            show_image_preview_window(app_clone, request_id_clone, image_path)
        })();
        if let Err(e) = result {
            log::error!("加载预览图片失败: {}", e);
        }
    });
    Ok(())
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
            crate::utils::image_clipboard::ImageClipboardManager::write_clipboard_image(
                app_handle, &image,
            )?;
            let _ = app_handle.emit(
                "image-item-promoted",
                serde_json::json!({
                    "itemId": item_id,
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
    let state_guard = lock_arc_mutex(state.inner());

    // 获取文本剪贴板数据
    let text_manager = lock_arc_mutex(&state_guard.clipboard_manager);
    let text_history = text_manager.get_history();
    let text_categories = text_manager.get_categories();
    let text_category_list = text_manager.get_category_list();
    let text_pinned_items = text_manager.get_pinned_items();
    drop(text_manager);

    // 获取图片剪贴板数据
    let image_manager = lock_arc_mutex(&state_guard.image_clipboard_manager);
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
pub async fn get_ai_settings() -> Result<HashMap<String, serde_json::Value>, String> {
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
        "image_hot_key".to_string(),
        serde_json::Value::String(settings.image_hot_key.clone()),
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
                config_map.insert("api_key".to_string(), serde_json::Value::String(if api_key.is_empty() { "".to_string() } else { "********".to_string() }));

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
}

#[tauri::command]
pub async fn get_text_dedup_metrics() -> Result<serde_json::Value, String> {
    if !cfg!(debug_assertions) {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "仅开发环境可用",
            "debug_assertions is false",
        ));
    }
    serde_json::to_value(get_dedup_scan_metrics()).map_err(|e| {
        frontend_error(
            ErrorCode::SystemError,
            "序列化去重指标失败",
            e.to_string(),
        )
    })
}

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
    selection_enabled: Option<bool>,
    grouped_items_protected_from_limit: Option<bool>,
    translation_prompt_template: Option<String>,
    explanation_prompt_template: Option<String>,
    image_fill_verify_mode: Option<String>,
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
            if app.global_shortcut().is_registered(hot_key_val.as_str()) {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    format!("快捷键被占用：{}", hot_key_val),
                    "global shortcut already registered",
                ));
            }

            // 尝试注销旧快捷键，如果旧快捷键未注册成功则忽略错误
            if let Err(e) = app.global_shortcut().unregister(settings.hot_key.as_str()) {
                log::warn!("注销旧快捷键 '{}' 失败 (可能从未注册成功): {}", settings.hot_key, e);
            }
            let app_clone = app.clone();
            let state_clone = state.inner().clone();
            let hot_key_clone = hot_key_val.clone();
            app.global_shortcut()
                .on_shortcut(hot_key_val.as_str(), move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        let sg = lock_arc_mutex(&state_clone);
                        if !sg.is_visible {
                            let state_for_window = state_clone.clone();
                            drop(sg);
                            interrupt_text_fill_flow(&state_for_window);
                            show_clipboard_window(app_clone.clone(), state_for_window);
                            features::mouse_listener::reset_ctrl_key_state();
                        }
                    }
                })
                .map_err(|e| frontend_error(ErrorCode::SystemError, "注册文字窗口快捷键失败", e.to_string()))?;
            settings.hot_key = hot_key_clone;
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

            if app.global_shortcut().is_registered(image_hot_key_val.as_str()) {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    format!("图片窗口快捷键被占用：{}", image_hot_key_val),
                    "image global shortcut already registered",
                ));
            }

            // 尝试注销旧快捷键，如果旧快捷键未注册成功则忽略错误
            if let Err(e) = app.global_shortcut().unregister(settings.image_hot_key.as_str()) {
                log::warn!("注销旧图片快捷键 '{}' 失败 (可能从未注册成功): {}", settings.image_hot_key, e);
            }
            let app_clone = app.clone();
            let state_clone = state.inner().clone();
            let image_hot_key_clone = image_hot_key_val.clone();
            app.global_shortcut()
                .on_shortcut(image_hot_key_val.as_str(), move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        let sg = lock_arc_mutex(&state_clone);
                        if !sg.is_visible && !sg.is_image_visible {
                            let state_for_window = state_clone.clone();
                            drop(sg);
                            interrupt_image_fill_flow(&state_for_window);
                            show_image_clipboard_window(app_clone.clone(), state_for_window);
                        }
                    }
                })
                .map_err(|e| frontend_error(ErrorCode::SystemError, "注册图片窗口快捷键失败", e.to_string()))?;
            settings.image_hot_key = image_hot_key_clone;
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
            if api_key.trim().is_empty() {
                return Err(frontend_error(
                    ErrorCode::ValidationError,
                    "API密钥不能为空，请填写有效的API密钥",
                    "ai_api_key is empty",
                ));
            }

            if api_key != "********" {
                settings
                    .save_current_provider_config(api_key)
                    .map_err(|e| frontend_error(ErrorCode::ConfigError, "保存提供商配置失败", e))?;

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

    settings.migrate_from_old();

    settings
        .validate()
        .map_err(|e| frontend_error(ErrorCode::ValidationError, "设置验证失败", e))?;

    save_settings(&settings).map_err(|e| frontend_error(ErrorCode::ConfigError, "保存设置失败", e))?;
    set_image_fill_verify_mode(&settings.image_fill_verify_mode);

    let selection_enabled = settings.selection_enabled;
    {
        let mut state_guard = lock_arc_mutex(state.inner());
        {
            let mut manager = lock_arc_mutex(&state_guard.clipboard_manager);
            if let Some(val) = text_max_items {
                manager.set_max_items(val);
            }
            if let Some(val) = grouped_items_protected_from_limit {
                manager.set_grouped_items_protected_from_limit(val);
            }
        }
        {
            let mut manager = lock_arc_mutex(&state_guard.image_clipboard_manager);
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
        state_guard.settings = settings.clone();
    }

    features::mouse_listener::set_selection_listener_enabled(
        app.clone(),
        state.inner().clone(),
        selection_enabled,
    );

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
        let state_guard = lock_arc_mutex(state.inner());
        let provider = ai_provider.unwrap_or_else(|| state_guard.settings.ai_provider.clone());
        match state_guard.settings.get_provider_api_key(&provider) {
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
pub async fn copy_and_paste_text(text: String, app: AppHandle) -> Result<(), String> {
    app.clipboard()
        .write_text(text)
        .map_err(|e| frontend_error(ErrorCode::ClipboardError, "复制文本失败", e.to_string()))?;

    if let Some(window) = app.get_webview_window("result_translation") {
        let _ = window.hide();
    }
    if let Some(window) = app.get_webview_window("result_explanation") {
        let _ = window.hide();
    }

    tauri::async_runtime::spawn_blocking(move || {
        thread::sleep(Duration::from_millis(80));
        crate::ui::window_manager::simulate_paste()
    })
        .await
        .map_err(|e| frontend_error(ErrorCode::SystemError, "自动粘贴任务执行失败", e.to_string()))?
        .map_err(|e| frontend_error(ErrorCode::ClipboardError, "自动粘贴失败", e))?;
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

    for (provider_key, _) in &settings.provider_configs {
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

/// 开始截图（全屏）
#[tauri::command]
pub async fn start_screenshot() -> Result<serde_json::Value, String> {
    use crate::features::screenshot::capture;

    log::info!("开始全屏截图");

    match capture::capture_full_screen() {
        Ok((rgba, width, height)) => {
            let png_base64 = capture::rgba_to_base64_png(&rgba, width, height)
                .map_err(|e| format!("转换PNG失败: {}", e))?;

            Ok(serde_json::json!({
                "success": true,
                "width": width,
                "height": height,
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

/// 捕获指定区域
#[tauri::command]
pub async fn capture_region(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<serde_json::Value, String> {
    use crate::features::screenshot::capture;

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
                        match std::fs::write(path_buf, &png_data) {
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
    let label = "pinned_image_window".to_string();
    let x = request.x.unwrap_or(100.0).max(0.0);
    let y = request.y.unwrap_or(100.0).max(0.0);
    let width = request.width.unwrap_or(360.0).max(1.0);
    let height = request.height.unwrap_or(240.0).max(1.0);
    let window = if let Some(existing) = app.get_webview_window(&label) {
        existing
    } else {
        tauri::WebviewWindowBuilder::new(
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
            .build()
            .map_err(|e| format!("创建固定图片窗口失败: {}", e))?
    };

    let window_clone = window.clone();
    let payload = serde_json::json!({
        "label": label,
        "png_base64": request.png_base64,
        "width": width,
        "height": height
    });
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(180));
        let _ = window_clone.set_resizable(true);
        let _ = window_clone.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        let _ = window_clone.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
        let _ = window_clone.show();
        std::thread::sleep(std::time::Duration::from_millis(60));
        for _ in 0..8 {
            let script = format!(
                "window.__PINNED_IMAGE_PAYLOAD__ = {}; window.dispatchEvent(new CustomEvent('pinned-image-data', {{ detail: {} }}));",
                payload, payload
            );
            let _ = window_clone.eval(script);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    Ok(serde_json::json!({ "success": true }))
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
    capture::set_allow_image_clipboard_once(linked);
    Ok(())
}

/// 打开截图编辑窗口
#[tauri::command]
pub async fn open_screenshot_editor(app: AppHandle) -> Result<(), String> {
    log::info!("打开截图编辑窗口");

    use crate::features::screenshot::capture;
    capture::set_screenshot_in_progress(true);
    let (rgba, width, height) = match capture::capture_full_screen() {
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

    // 先发送截图数据，然后再显示窗口
    if let Some(window) = app.get_webview_window("screenshot") {
        // 先发送数据
        let payload = serde_json::json!({
            "png_base64": png_base64,
            "width": width,
            "height": height
        });
        let script = format!(
            "window.dispatchEvent(new CustomEvent('screenshot-data', {{ detail: {} }}));",
            payload
        );
        let _ = window.eval(script);

        // 等待数据处理后再显示窗口
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));

            // 恢复窗口属性（防止之前关闭失败导致属性未重置）
            let _ = window.set_always_on_top(true);
            let _ = window.set_ignore_cursor_events(false);

            let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: 0, y: 0 }));
            let _ = window.show();
            let _ = window.set_focus();

            // 再发送开始区域选择事件
            let script = "window.dispatchEvent(new CustomEvent('start-region-select'));";
            let _ = window.eval(script);
        });
    } else {
        // 创建新的全屏透明覆盖窗口
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
            .build()
            .map_err(|e| {
                capture::set_screenshot_in_progress(false);
                format!("创建截图窗口失败: {}", e)
            })?;

        // 窗口创建后发送数据并显示
        let window_clone = window.clone();
        let png_base64_clone = png_base64.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));

            let _ = window_clone.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
            let _ = window_clone.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: 0, y: 0 }));

            let payload = serde_json::json!({
                "png_base64": png_base64_clone,
                "width": width,
                "height": height
            });
            let script = format!(
                "window.dispatchEvent(new CustomEvent('screenshot-data', {{ detail: {} }}));",
                payload
            );
            let _ = window_clone.eval(script);

            std::thread::sleep(std::time::Duration::from_millis(100));

            let _ = window_clone.show();
            let _ = window_clone.set_focus();

            // 发送开始区域选择事件
            let script = "window.dispatchEvent(new CustomEvent('start-region-select'));";
            let _ = window_clone.eval(script);
        });
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
    crate::features::screenshot::capture::set_screenshot_in_progress(false);

    if let Some(window) = app.get_webview_window("screenshot") {
        // 解除置顶和鼠标拦截，防止在Windows上残留透明幽灵窗口导致桌面无法点击
        let _ = window.set_always_on_top(false);
        let _ = window.set_ignore_cursor_events(true);

        // 短暂延迟后隐藏和关闭
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = window.hide();
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = window.close();
        });
    }

    Ok(())
}
