use crate::core::app_state::AppState as SharedAppState;
use crate::core::error::{to_frontend_error_string, AppError, AppResult, ErrorCode};
use crate::core::perf_metrics::record_perf_metric;
use crate::features;
use crate::services::image_clipboard_manager::emit_image_history_payload;
use crate::sync::{lock_arc_mutex, Mutex};
use crate::ui::commands_screenshot::open_screenshot_editor;
use crate::ui::commands_writeback::{
    begin_fill_sequence, interrupt_image_fill_flow, interrupt_text_fill_flow,
    schedule_image_promote_to_top, spawn_fill_task,
    FillKind,
};
use crate::ui::window_manager::{
    hide_clipboard_window, hide_image_clipboard_window, hide_image_preview_window,
    set_window_position, show_clipboard_window, show_image_clipboard_window,
    show_image_preview_window,
};
use crate::utils::clipboard::ClipboardManager;
use crate::utils::image_clipboard::{
    is_fast_fill_verify_mode_enabled,
    ImageClipboardManager, ImageHistoryPageData, ImageHistoryPreviewItem,
};
use crate::utils::utils_helpers::{
    load_history_page_data_async, save_settings, ClipboardHistoryPageData,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

#[derive(serde::Serialize)]
pub struct TextHistoryItem {
    pub id: String,
    pub content: String,
}

#[derive(serde::Serialize)]
pub struct HistoryResponse {
    history: Vec<TextHistoryItem>,
    categories: HashMap<String, String>,
    category_list: Vec<String>,
    pinned_items: Vec<String>,
}

/// 批量获取剪贴板完整快照（优化 IPC 通信）
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardFullSnapshot {
    pub text_history: Vec<TextHistoryItem>,
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
    let started_at = std::time::Instant::now();
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

    let result = manager.get_history_preview_page_async(page_request).await;
    record_perf_metric(
        "image.history_page",
        "图片历史分页加载耗时",
        started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    Ok(result)
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
    item_id: String,
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
pub async fn open_text_preview_window(
    text: String,
    item_id: Option<String>,
    app: AppHandle,
) -> Result<(), String> {
    crate::ui::window_manager::show_text_preview_window(app, text, item_id)
}

#[tauri::command]
pub async fn close_text_preview_window(app: AppHandle) -> Result<(), String> {
    crate::ui::window_manager::hide_text_preview_window(app);
    Ok(())
}

#[tauri::command]
pub async fn start_text_preview_window_drag(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("text_preview") {
        let _ = window.start_dragging();
    }
    Ok(())
}

#[tauri::command]
pub async fn open_image_preview_window_by_id(
    request: ItemIdRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    run_blocking("打开图片预览", move || {
        execute_open_image_preview_window_by_id(request.item_id, state_arc, app)
    }).await
}

pub(crate) fn is_screenshot_feature_enabled(state: &Arc<Mutex<SharedAppState>>) -> bool {
    let guard = lock_arc_mutex(state);
    guard.settings.screenshot_enabled
}

pub(crate) fn recompute_selection_related_flags(state: &mut SharedAppState) {
    state.is_processing_selection = state.is_selection_capture_active
        || state.is_text_writeback_active
        || state.is_image_writeback_active;
    state.is_updating_clipboard = state.is_text_writeback_active || state.is_image_writeback_active;
}

pub(crate) fn register_text_shortcut(
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

pub(crate) fn register_image_shortcut(
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

pub(crate) fn register_screenshot_shortcut(app: &AppHandle, hot_key: &str) -> Result<(), String> {
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
        .map_err(|e| frontend_error(ErrorCode::ValidationError, format!("截图快捷键被占用或注册失败：{}", hot_key), e.to_string()))?;
    Ok(())
}


pub(crate) fn set_updating_clipboard(state: &Arc<Mutex<SharedAppState>>, updating: bool) {
    let mut state_guard = lock_arc_mutex(state);
    if !updating {
        state_guard.is_text_writeback_active = false;
        state_guard.is_image_writeback_active = false;
    }
    recompute_selection_related_flags(&mut state_guard);
}

pub(crate) fn get_clipboard_manager_arc(state: &Arc<Mutex<SharedAppState>>) -> Arc<Mutex<ClipboardManager>> {
    let state_guard = lock_arc_mutex(state);
    state_guard.clipboard_manager.clone()
}

pub(crate) fn get_image_clipboard_manager_arc(
    state: &Arc<Mutex<SharedAppState>>,
) -> Arc<Mutex<ImageClipboardManager>> {
    let state_guard = lock_arc_mutex(state);
    state_guard.image_clipboard_manager.clone()
}

trait CategoryOps: Clone + Send + 'static {
    fn set_category_async(&self, item_id: String, category: String) -> impl std::future::Future<Output = Result<(), String>> + Send;
    fn remove_category_async(&self, category: String) -> impl std::future::Future<Output = Result<(), String>> + Send;
    fn add_category_async(&self, category: String) -> impl std::future::Future<Output = Result<(), String>> + Send;
}

impl CategoryOps for ClipboardManager {
    fn set_category_async(&self, item_id: String, category: String) -> impl std::future::Future<Output = Result<(), String>> + Send {
        ClipboardManager::set_category_async(self, item_id, category)
    }
    fn remove_category_async(&self, category: String) -> impl std::future::Future<Output = Result<(), String>> + Send {
        ClipboardManager::remove_category_async(self, category)
    }
    fn add_category_async(&self, category: String) -> impl std::future::Future<Output = Result<(), String>> + Send {
        ClipboardManager::add_category_async(self, category)
    }
}

impl CategoryOps for ImageClipboardManager {
    fn set_category_async(&self, item_id: String, category: String) -> impl std::future::Future<Output = Result<(), String>> + Send {
        ImageClipboardManager::set_category_async(self, item_id, category)
    }
    fn remove_category_async(&self, category: String) -> impl std::future::Future<Output = Result<(), String>> + Send {
        ImageClipboardManager::remove_category_async(self, category)
    }
    fn add_category_async(&self, category: String) -> impl std::future::Future<Output = Result<(), String>> + Send {
        ImageClipboardManager::add_category_async(self, category)
    }
}

async fn category_set<M: CategoryOps>(
    manager_arc: Arc<Mutex<M>>,
    item_id: String,
    category: String,
    label: &str,
) -> Result<(), String> {
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager.set_category_async(item_id, category).await.map_err(|e| {
        to_frontend_error_string(
            AppError::new(ErrorCode::ClipboardError, format!("设置{}分类失败", label)).with_details(e),
        )
    })
}

async fn category_remove<M: CategoryOps>(
    manager_arc: Arc<Mutex<M>>,
    category: String,
    label: &str,
) -> Result<(), String> {
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager.remove_category_async(category).await.map_err(|e| {
        to_frontend_error_string(
            AppError::new(ErrorCode::ClipboardError, format!("删除{}分类失败", label)).with_details(e),
        )
    })
}

async fn category_add<M: CategoryOps>(
    manager_arc: Arc<Mutex<M>>,
    category: String,
    label: &str,
) -> Result<(), String> {
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager.add_category_async(category).await.map_err(|e| {
        to_frontend_error_string(
            AppError::new(ErrorCode::ClipboardError, format!("新增{}分类失败", label)).with_details(e),
        )
    })
}

pub(crate) fn frontend_error(
    code: ErrorCode,
    message: impl Into<String>,
    details: impl Into<String>,
) -> String {
    to_frontend_error_string(AppError::new(code, message).with_details(details.into()))
}

/// 通用的 spawn_blocking 包装器，统一错误映射
pub(crate) async fn run_blocking<T, F>(label: &str, task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> AppResult<T> + Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(task).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(app_err)) => Err(to_frontend_error_string(app_err)),
        Err(join_err) => Err(frontend_error(ErrorCode::SystemError, format!("{}任务执行失败", label), join_err.to_string())),
    }
}

pub(crate) fn with_updating_clipboard<T, F>(
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

pub(crate) fn try_replace_text_clipboard_after_remove(
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
        manager.get_latest_item()
    };
    if let Some(next) = next_item {
        let manager = lock_arc_mutex(&manager_arc);
        if let Err(e) = manager.set_clipboard_content(app, &next) {
            log::warn!("删除文本后写入下一条到剪贴板失败: {}", e);
        }
    }
}

pub(crate) fn try_replace_image_clipboard_after_remove(
    state: &Arc<Mutex<SharedAppState>>,
    app: &AppHandle,
    removed_signature: &str,
) {
    let manager_arc = get_image_clipboard_manager_arc(state);
    let should_replace_clipboard = match ImageClipboardManager::read_clipboard_images_rgba(app) {
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
        if let Err(e) = ImageClipboardManager::write_clipboard_image(app, &image) {
            log::warn!("删除图片后写入下一张到剪贴板失败: {}", e);
        }
    }
}

pub(crate) fn execute_select_and_fill_text(
    request: SelectAndFillRequest,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> AppResult<String> {
    let item_id = request.item_id;
    let fill_seq = begin_fill_sequence(&state, FillKind::Text);
    let operation_id = request.op_id.unwrap_or(fill_seq);
    let manager_arc = get_clipboard_manager_arc(&state);

    let item_content = {
        let manager = lock_arc_mutex(&manager_arc);
        manager.promote_to_top(&item_id).map_err(|e| {
            AppError::new(ErrorCode::ClipboardError, "找不到目标项目".to_string()).with_details(e)
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
            let item_id = crate::utils::database::stable_history_item_id(&item_content_clone);
            let _ = app_handle.emit(
                "text-item-promoted",
                serde_json::json!({
                    "id": item_id,
                }),
            );
            Ok(())
        },
    );

    Ok(item_content)
}

pub(crate) fn execute_remove_clipboard_item(
    item_id: String,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> AppResult<()> {
    log::info!("删除剪贴板项目: {}", item_id);
    let manager_arc = get_clipboard_manager_arc(&state);
    with_updating_clipboard(&state, || -> Result<(), String> {
        let removed_item = {
            let manager = lock_arc_mutex(&manager_arc);
            manager.remove_from_history(&item_id)?
        };
        try_replace_text_clipboard_after_remove(&state, &app, &removed_item);
        Ok(())
    })
        .map_err(|e| AppError::new(ErrorCode::ClipboardError, "删除文本历史失败").with_details(e))
}

pub(crate) fn execute_open_image_preview_window_by_id(
    item_id: String,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> AppResult<()> {
    let request_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .to_string();
    let manager_arc = get_image_clipboard_manager_arc(&state);
    let image_path = {
        let manager = lock_arc_mutex(&manager_arc);
        manager.get_preview_image_path_by_id(&item_id)
            .map_err(|e| AppError::new(ErrorCode::SystemError, "获取预览图片路径失败").with_details(e))?
    };
    let preview_path = ensure_preview_image_path_for_asset(&item_id, &image_path)
        .map_err(|e| AppError::new(ErrorCode::SystemError, "准备预览图片资源失败").with_details(e))?;
    show_image_preview_window(app, request_id, preview_path)
        .map_err(|e| AppError::new(ErrorCode::SystemError, "显示图片预览失败").with_details(e))
}

pub(crate) fn ensure_preview_image_path_for_asset(item_id: &str, image_path: &str) -> Result<String, String> {
    let started_at = std::time::Instant::now();
    let trimmed = image_path.trim();
    if trimmed.is_empty() {
        let error = "图片路径为空".to_string();
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        return Err(error);
    }
    let source_path = PathBuf::from(trimmed);
    if !source_path.exists() {
        let error = format!("图片文件不存在: {}", trimmed);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        return Err(error);
    }
    if !source_path.is_file() {
        let error = format!("图片路径不是文件: {}", trimmed);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        return Err(error);
    }
    let canonical_source = source_path.canonicalize().map_err(|e| {
        let error = format!("规范化图片路径失败: {}", e);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    let ext = canonical_source
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let allowed_ext = ["png", "jpg", "jpeg", "webp", "bmp", "gif"];
    if !allowed_ext.contains(&ext.as_str()) {
        let error = format!("不支持的图片格式: {}", ext);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        return Err(error);
    }

    let mut blobs_dir = std::env::current_exe().map_err(|e| {
        let error = format!("获取程序目录失败: {}", e);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    blobs_dir.pop();
    blobs_dir.push("image_history_blobs");
    fs::create_dir_all(&blobs_dir).map_err(|e| {
        let error = format!("创建图片目录失败: {}", e);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    let canonical_blobs = blobs_dir
        .canonicalize()
        .unwrap_or_else(|_| blobs_dir.clone());
    if canonical_source.starts_with(&canonical_blobs) {
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            true,
            None,
        );
        return Ok(canonical_source.to_string_lossy().to_string());
    }

    let normalized_item_id = sanitize_image_item_id(item_id);
    let target_name = if ext.is_empty() {
        format!("preview_external_{}.png", normalized_item_id)
    } else {
        format!("preview_external_{}.{}", normalized_item_id, ext)
    };
    let target_path = canonical_blobs.join(target_name);
    fs::copy(&canonical_source, &target_path).map_err(|e| {
        let error = format!("复制预览图片到受控目录失败: {}", e);
        record_perf_metric(
            "image.preview_asset_path",
            "图片预览路径准备耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    record_perf_metric(
        "image.preview_asset_path",
        "图片预览路径准备耗时",
        started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    Ok(target_path.to_string_lossy().to_string())
}

pub(crate) fn sanitize_image_item_id(raw: &str) -> String {
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

pub(crate) fn execute_warmup_image_clipboard_item_by_id(
    item_id: String,
    state: Arc<Mutex<SharedAppState>>,
) -> AppResult<()> {
    let started_at = std::time::Instant::now();
    let manager_arc = get_image_clipboard_manager_arc(&state);
    let manager = lock_arc_mutex(&manager_arc);
    manager.warmup_image_by_id(&item_id).map_err(|e| {
        record_perf_metric(
            "image.warmup",
            "图片预热耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(e.clone()),
        );
        AppError::new(ErrorCode::ClipboardError, "预热图片失败").with_details(e)
    })?;
    record_perf_metric(
        "image.warmup",
        "图片预热耗时",
        started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    Ok(())
}

pub(crate) fn execute_promote_image_clipboard_item_by_id(
    item_id: String,
    state: Arc<Mutex<SharedAppState>>,
) -> AppResult<()> {
    let manager_arc = get_image_clipboard_manager_arc(&state);
    let manager = lock_arc_mutex(&manager_arc);
    manager
        .promote_to_top_by_id(&item_id)
        .map_err(|e| AppError::new(ErrorCode::ClipboardError, "置顶图片失败").with_details(e))
}

pub(crate) fn execute_remove_image_clipboard_item_by_id(
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

pub(crate) fn execute_select_and_fill_image_by_id(
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
            let prepare_started_at = std::time::Instant::now();
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
            record_perf_metric(
                "image.fill_prepare",
                "图片回填准备耗时",
                prepare_started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            ImageClipboardManager::write_clipboard_image(app_handle, &image)?;
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
    let history_items: Vec<TextHistoryItem> = manager
        .get_history()
        .into_iter()
        .map(|content| TextHistoryItem {
            id: crate::utils::database::stable_history_item_id(&content),
            content,
        })
        .collect();
    Ok(HistoryResponse {
        history: history_items,
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
    let text_history_items: Vec<TextHistoryItem> = text_manager
        .get_history()
        .into_iter()
        .map(|content| TextHistoryItem {
            id: crate::utils::database::stable_history_item_id(&content),
            content,
        })
        .collect();
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
        text_history: text_history_items,
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
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    request: ClipboardHistoryPageRequest,
) -> Result<ClipboardHistoryPageData, String> {
    let started_at = std::time::Instant::now();
    let history_items = {
        let state_guard = lock_arc_mutex(&state);
        let manager = lock_arc_mutex(&state_guard.clipboard_manager);
        manager.get_history()
    };
    if let Err(e) = crate::utils::database::save_history_items_only_async(&history_items).await {
        log::error!("Failed to sync history before querying page: {}", e);
    }
    let result = load_history_page_data_async(
        request.offset,
        request.limit,
        request.category,
        request.pinned_only,
        request.keyword,
        request.sort_by,
        request.sort_order,
    )
        .await;
    match &result {
        Ok(_) => record_perf_metric(
            "text.history_page",
            "文本历史分页加载耗时",
            started_at.elapsed().as_millis() as u64,
            true,
            None,
        ),
        Err(error) => record_perf_metric(
            "text.history_page",
            "文本历史分页加载耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        ),
    }
    result
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
    item_id: String,
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    category_set(get_clipboard_manager_arc(state.inner()), item_id, category, "文本").await
}

#[tauri::command]
pub async fn remove_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    category_remove(get_clipboard_manager_arc(state.inner()), category, "文本").await
}

#[tauri::command]
pub async fn add_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    category_add(get_clipboard_manager_arc(state.inner()), category, "文本").await
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
    window
        .start_dragging()
        .map_err(|e| format!("拖动窗口失败: {}", e))
}

#[tauri::command]
pub async fn warmup_image_clipboard_item_by_id(
    request: ItemIdRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    run_blocking("预热图片", move || {
        execute_warmup_image_clipboard_item_by_id(request.item_id, state_arc)
    }).await
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
            if let Some(index) = manager
                .get_history()
                .iter()
                .position(|item| item.id == item_id)
            {
                if index < 6 {
                    let _ = manager.warmup_image_by_id(&item_id);
                }
            }
        }
    })
        .await
        .map_err(|e| {
            frontend_error(
                ErrorCode::SystemError,
                "批量预热图片任务执行失败",
                e.to_string(),
            )
        })?;
    Ok(())
}

#[tauri::command]
pub async fn set_image_item_category(
    item_id: String,
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    category_set(get_image_clipboard_manager_arc(state.inner()), item_id, category, "图片").await
}

#[tauri::command]
pub async fn remove_image_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    category_remove(get_image_clipboard_manager_arc(state.inner()), category, "图片").await
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
    manager.set_tags_async(item_id, tags).await.map_err(|e| {
        to_frontend_error_string(
            AppError::new(ErrorCode::ClipboardError, "设置图片标签失败").with_details(e),
        )
    })
}

#[tauri::command]
pub async fn update_text_item(
    item_id: String,
    new_content: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };

    let result = manager
        .update_item_content(&item_id, new_content.clone())
        .await
        .map_err(|e| {
            to_frontend_error_string(
                AppError::new(ErrorCode::ClipboardError, "更新文本内容失败").with_details(e),
            )
        });

    if result.is_ok() {
        let new_item_id = crate::utils::database::stable_history_item_id(&new_content);
        let _ = app.emit("text-item-replaced", serde_json::json!({
            "old_id": item_id,
            "new_id": new_item_id,
            "new_content": new_content
        }));
    }

    result
}

#[tauri::command]
pub async fn set_clipboard_item_pinned(
    item_id: String,
    pinned: bool,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .set_pinned_by_selector_async(&item_id, pinned)
        .await
        .map_err(|e| {
            if e == "索引超出范围" {
                to_frontend_error_string(AppError::new(ErrorCode::ValidationError, "索引超出范围"))
            } else {
                to_frontend_error_string(
                    AppError::new(ErrorCode::ClipboardError, "设置置顶状态失败").with_details(e),
                )
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
        .map_err(|e| {
            to_frontend_error_string(
                AppError::new(ErrorCode::ClipboardError, "设置图片置顶状态失败").with_details(e),
            )
        })
}

#[tauri::command]
pub async fn promote_clipboard_item(
    item_id: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<String, String> {
    let manager_arc = get_clipboard_manager_arc(state.inner());
    let manager = {
        let guard = lock_arc_mutex(&manager_arc);
        guard.clone()
    };
    manager
        .promote_to_top_async(&item_id)
        .await
        .map(|item| crate::utils::database::stable_history_item_id(&item))
        .map_err(|e| {
            to_frontend_error_string(
                AppError::new(ErrorCode::ClipboardError, "置顶文本失败").with_details(e),
            )
        })
}

#[tauri::command]
pub async fn promote_image_clipboard_item_by_id(
    request: ItemIdRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    run_blocking("置顶图片", move || {
        execute_promote_image_clipboard_item_by_id(request.item_id, state_arc)
    }).await
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
        .map_err(|e| {
            to_frontend_error_string(
                AppError::new(ErrorCode::ClipboardError, "清理文本历史失败").with_details(e),
            )
        })
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
        .map_err(|e| {
            to_frontend_error_string(
                AppError::new(ErrorCode::ClipboardError, "清理图片历史失败").with_details(e),
            )
        })?;

    let is_visible = {
        let mut state_guard = lock_arc_mutex(state.inner());

        state_guard.image_history_dirty = true;
        state_guard.is_image_visible
    };

    if is_visible {
        emit_image_history_payload(&app, state.inner().clone());
    }

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
    let image_paths = collect_import_image_paths(paths)
        .map_err(|e| frontend_error(ErrorCode::IoError, "收集可导入图片路径失败", e))?;
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
        match manager
            .import_local_image_paths_async(vec![path.clone()])
            .await
        {
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
    let image_paths = collect_import_image_paths(paths)
        .map_err(|e| frontend_error(ErrorCode::IoError, "统计可导入图片路径失败", e))?;
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

pub(crate) fn collect_import_image_paths(entries: Vec<String>) -> Result<Vec<String>, String> {
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

pub(crate) fn collect_images_from_dir(dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    collect_images_from_dir_inner(dir, out, 0)
}

const MAX_IMPORT_DIR_DEPTH: u32 = 10;

fn collect_images_from_dir_inner(dir: &Path, out: &mut Vec<String>, depth: u32) -> Result<(), String> {
    if depth >= MAX_IMPORT_DIR_DEPTH {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|e| format!("读取目录失败: {}", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
        let path = entry.path();
        // 使用symlink_metadata避免跟随符号链接，防止无限递归
        let metadata = fs::symlink_metadata(&path)
            .map_err(|e| format!("读取文件元数据失败: {}", e))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_images_from_dir_inner(&path, out, depth + 1)?;
        } else if metadata.is_file() && is_importable_image_file(&path) {
            out.push(path.to_string_lossy().to_string());
        }
    }
    Ok(())
}

pub(crate) fn is_importable_image_file(path: &Path) -> bool {
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
    category_add(get_image_clipboard_manager_arc(state.inner()), category, "图片").await
}

#[tauri::command]
pub async fn get_clipboard_bottom_offset(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<i32, String> {
    let state_guard = lock_arc_mutex(state.inner());
    Ok(state_guard.settings.clipboard_bottom_offset)
}

#[tauri::command]
pub async fn preview_clipboard_bottom_offset(offset: i32, app: AppHandle) -> Result<(), String> {
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
    run_blocking("文本回填", move || {
        execute_select_and_fill_text(request, state_arc, app)
    }).await
}

#[tauri::command]
pub async fn remove_clipboard_item(
    item_id: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    run_blocking("删除文本历史", move || {
        execute_remove_clipboard_item(item_id, state_arc, app)
    }).await
}

#[tauri::command]
pub async fn remove_image_clipboard_item_by_id(
    item_id: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    run_blocking("删除图片历史", move || {
        execute_remove_image_clipboard_item_by_id(item_id, state_arc, app)
    }).await
}

#[tauri::command]
pub async fn select_and_fill_image_by_id(
    request: SelectAndFillImageByIdRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    run_blocking("图片回填", move || {
        execute_select_and_fill_image_by_id(request, state_arc, app)
    }).await
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
