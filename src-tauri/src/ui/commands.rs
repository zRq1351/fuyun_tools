use crate::core::app_state::AppState as SharedAppState;
use crate::core::config::{AIProvider, ProviderConfig};
use crate::features;
use crate::services::ai_client::{AIClient, AIConfig};
use crate::services::poll_metrics;
use crate::ui::window_manager::{
    hide_clipboard_window, hide_image_clipboard_window, hide_image_preview_window, set_window_position,
    show_clipboard_window, show_image_clipboard_window, show_image_preview_loading_window,
    show_image_preview_window,
};
use crate::utils::image_clipboard::ImageHistoryPreviewItem;
use crate::utils::utils_helpers::{
    default_explanation_prompt_template, default_translation_prompt_template, load_settings,
    save_settings, get_dedup_scan_metrics,
};
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

#[derive(serde::Serialize)]
pub struct HistoryResponse {
    history: Vec<String>,
    categories: HashMap<String, String>,
    category_list: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct ImageHistoryResponse {
    history: Vec<ImageHistoryPreviewItem>,
    categories: HashMap<String, String>,
    category_list: Vec<String>,
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
pub struct SelectAndFillImageRequest {
    index: usize,
    #[serde(default)]
    op_id: Option<u64>,
}

#[derive(Clone, Copy)]
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

fn begin_fill_sequence(state: &Arc<Mutex<SharedAppState>>, kind: FillKind) -> u64 {
    let mut state_guard = state.lock().unwrap();
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
    let guard = state.lock().unwrap();
    kind.current_seq(&guard) == fill_seq
}

fn finish_fill_if_latest(state: &Arc<Mutex<SharedAppState>>, kind: FillKind, fill_seq: u64) {
    if let Ok(mut guard) = state.lock() {
        if kind.current_seq(&guard) == fill_seq {
            guard.is_processing_selection = false;
            guard.is_updating_clipboard = false;
        }
    }
}

fn wait_for_fill_window_hidden(app: &AppHandle, window_label: &str, label: &str) {
    if let Err(e) = crate::ui::window_manager::wait_for_window_hidden(
        app,
        window_label,
        Duration::from_millis(900),
    ) {
        log::warn!("等待{}窗口隐藏失败: {}", label, e);
    } else {
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
        wait_for_fill_window_hidden(&app_handle, kind.window_label(), kind.label());

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
            simulate_paste_with_retry(kind.label(), Some(operation_id), started_at);
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
) {
    thread::sleep(Duration::from_millis(135));
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
            thread::sleep(Duration::from_millis(140));
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
    if let Ok(mut state_guard) = state.lock() {
        state_guard.is_updating_clipboard = updating;
    }
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
    let current_clipboard = {
        let state_guard = state.lock().unwrap();
        let manager = state_guard.clipboard_manager.lock().unwrap();
        manager.get_content(app)
    };

    if current_clipboard.as_deref() != Some(removed_item) {
        return;
    }

    let next_item = {
        let state_guard = state.lock().unwrap();
        let manager = state_guard.clipboard_manager.lock().unwrap();
        manager.get_history().first().cloned()
    };
    if let Some(next) = next_item {
        let state_guard = state.lock().unwrap();
        let manager = state_guard.clipboard_manager.lock().unwrap();
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
    let should_replace_clipboard =
        match crate::utils::image_clipboard::ImageClipboardManager::read_clipboard_images_rgba(app) {
            Ok(images) if !images.is_empty() => {
                let (rgba, width, height) = &images[0];
                crate::utils::image_clipboard::compute_signature(rgba, *width, *height)
                    == removed_signature
            }
            _ => false,
        };

    if !should_replace_clipboard {
        return;
    }

    let next_image = {
        let state_guard = state.lock().unwrap();
        let manager = state_guard.image_clipboard_manager.lock().unwrap();
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
) -> Result<String, String> {
    let index = request.index;
    let fill_seq = begin_fill_sequence(&state, FillKind::Text);
    let operation_id = request.op_id.unwrap_or(fill_seq);

    let item_content = {
        let state_guard = state.lock().unwrap();
        let manager = state_guard.clipboard_manager.lock().unwrap();
        manager
            .promote_to_top(index)
            .map_err(|e| format!("索引 {} 超出范围: {}", index, e))?
    };

    hide_clipboard_window(app.clone(), state.clone());

    let item_content_clone = item_content.clone();
    spawn_fill_task(
        FillKind::Text,
        app,
        state,
        fill_seq,
        operation_id,
        move |app_handle, state_ref| {
            let state_guard = state_ref.lock().unwrap();
            let manager = state_guard.clipboard_manager.lock().unwrap();
            manager.set_clipboard_content(app_handle, &item_content_clone)
        },
    );

    Ok(item_content)
}

fn execute_remove_clipboard_item(
    index: usize,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("删除剪贴板项目，索引: {}", index);
    with_updating_clipboard(&state, || {
        let removed_item = {
            let state_guard = state.lock().unwrap();
            let manager = state_guard.clipboard_manager.lock().unwrap();
            manager.remove_from_history(index)?
        };
        try_replace_text_clipboard_after_remove(&state, &app, &removed_item);
        Ok(())
    })
}

fn execute_remove_image_clipboard_item(
    index: usize,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> Result<(), String> {
    with_updating_clipboard(&state, || {
        let removed_signature = {
            let state_guard = state.lock().unwrap();
            let manager = state_guard.image_clipboard_manager.lock().unwrap();
            let (_, _, signature) = manager.remove_from_history(index)?;
            signature
        };
        try_replace_image_clipboard_after_remove(&state, &app, &removed_signature);
        Ok(())
    })
}

fn execute_select_and_fill_image(
    request: SelectAndFillImageRequest,
    state: Arc<Mutex<SharedAppState>>,
    app: AppHandle,
) -> Result<(), String> {
    let index = request.index;
    let fill_seq = begin_fill_sequence(&state, FillKind::Image);
    let operation_id = request.op_id.unwrap_or(fill_seq);

    hide_image_clipboard_window(app.clone(), state.clone());

    spawn_fill_task(
        FillKind::Image,
        app,
        state,
        fill_seq,
        operation_id,
        move |app_handle, state_ref| {
            let image = {
                let state_guard = state_ref.lock().unwrap();
                let manager = state_guard.image_clipboard_manager.lock().unwrap();
                manager.promote_to_top(index)?;
                manager.get_image_by_index(0)?
            };
            crate::utils::image_clipboard::ImageClipboardManager::write_clipboard_image(
                app_handle, &image,
            )?;
            Ok(())
        },
    );

    Ok(())
}

#[tauri::command]
pub async fn get_clipboard_history(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<HistoryResponse, String> {
    let state_guard = state.lock().unwrap();
    let manager = state_guard.clipboard_manager.lock().unwrap();
    Ok(HistoryResponse {
        history: manager.get_history(),
        categories: manager.get_categories(),
        category_list: manager.get_category_list(),
    })
}

#[tauri::command]
pub async fn set_item_category(
    item: String,
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_guard = state.lock().unwrap();
    let manager = state_guard.clipboard_manager.lock().unwrap();
    manager.set_category(item, category)
}

#[tauri::command]
pub async fn remove_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_guard = state.lock().unwrap();
    let manager = state_guard.clipboard_manager.lock().unwrap();
    manager.remove_category(category)
}

#[tauri::command]
pub async fn add_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_guard = state.lock().unwrap();
    let manager = state_guard.clipboard_manager.lock().unwrap();
    manager.add_category(category)
}

#[tauri::command]
pub async fn get_image_clipboard_history(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<ImageHistoryResponse, String> {
    let state_guard = state.lock().unwrap();
    let manager = state_guard.image_clipboard_manager.lock().unwrap();
    Ok(ImageHistoryResponse {
        history: manager.get_history_preview(),
        categories: manager.get_categories(),
        category_list: manager.get_category_list(),
    })
}

#[tauri::command]
pub async fn open_image_preview_window(
    index: usize,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    show_image_preview_loading_window(app.clone())?;
    let state_clone = state.inner().clone();
    let app_clone = app.clone();
    thread::spawn(move || {
        let result: Result<(), String> = (|| {
            let (rgba_base64, width, height) = {
                let state_guard = state_clone.lock().unwrap();
                let manager = state_guard.image_clipboard_manager.lock().unwrap();
                manager.get_preview_window_payload_by_index(index)?
            };
            show_image_preview_window(app_clone, rgba_base64, width, height)
        })();
        if let Err(e) = result {
            log::error!("加载预览图片失败: {}", e);
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn close_image_preview_window(app: AppHandle) -> Result<(), String> {
    hide_image_preview_window(app);
    Ok(())
}

#[tauri::command]
pub async fn warmup_image_clipboard_item(
    index: usize,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_guard = state.lock().unwrap();
    let manager = state_guard.image_clipboard_manager.lock().unwrap();
    manager.warmup_image_by_index(index)
}

#[tauri::command]
pub async fn set_image_item_category(
    item_id: String,
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_guard = state.lock().unwrap();
    let manager = state_guard.image_clipboard_manager.lock().unwrap();
    manager.set_category(item_id, category)
}

#[tauri::command]
pub async fn remove_image_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_guard = state.lock().unwrap();
    let manager = state_guard.image_clipboard_manager.lock().unwrap();
    manager.remove_category(category)
}

#[tauri::command]
pub async fn add_image_category(
    category: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let state_guard = state.lock().unwrap();
    let manager = state_guard.image_clipboard_manager.lock().unwrap();
    manager.add_category(category)
}

#[tauri::command]
pub async fn get_clipboard_bottom_offset(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<i32, String> {
    let state_guard = state.lock().unwrap();
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
        let state_guard = state.lock().unwrap();
        state_guard.settings.clone()
    };
    settings.clipboard_bottom_offset = final_offset;
    save_settings(&settings).map_err(|e| e.to_string())?;

    {
        let mut state_guard = state.lock().unwrap();
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
    execute_select_and_fill_text(request, state.inner().clone(), app)
}

#[tauri::command]
pub async fn remove_clipboard_item(
    index: usize,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    execute_remove_clipboard_item(index, state.inner().clone(), app)
}

#[tauri::command]
pub async fn remove_image_clipboard_item(
    index: usize,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    execute_remove_image_clipboard_item(index, state.inner().clone(), app)
}

#[tauri::command]
pub async fn select_and_fill_image(
    request: SelectAndFillImageRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    execute_select_and_fill_image(request, state.inner().clone(), app)
}

#[tauri::command]
pub async fn window_blur(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
    let is_visible = {
        let state_guard = state.lock().unwrap();
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
        let state_guard = state.lock().unwrap();
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
    let settings = load_settings()?;

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
        "clipboard_poll_min_interval_ms".to_string(),
        serde_json::Value::Number(serde_json::Number::from(
            settings.clipboard_poll_min_interval_ms,
        )),
    );
    result.insert(
        "clipboard_poll_warm_interval_ms".to_string(),
        serde_json::Value::Number(serde_json::Number::from(
            settings.clipboard_poll_warm_interval_ms,
        )),
    );
    result.insert(
        "clipboard_poll_idle_interval_ms".to_string(),
        serde_json::Value::Number(serde_json::Number::from(
            settings.clipboard_poll_idle_interval_ms,
        )),
    );
    result.insert(
        "clipboard_poll_max_interval_ms".to_string(),
        serde_json::Value::Number(serde_json::Number::from(
            settings.clipboard_poll_max_interval_ms,
        )),
    );
    result.insert(
        "clipboard_poll_report_interval_secs".to_string(),
        serde_json::Value::Number(serde_json::Number::from(
            settings.clipboard_poll_report_interval_secs,
        )),
    );
    result.insert(
        "clipboard_poll_metrics_enabled".to_string(),
        serde_json::Value::Bool(settings.clipboard_poll_metrics_enabled),
    );
    result.insert(
        "clipboard_poll_metrics_log_level".to_string(),
        serde_json::Value::String(settings.clipboard_poll_metrics_log_level.clone()),
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
                config_map.insert("api_key".to_string(), serde_json::Value::String(api_key));

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
pub async fn get_poll_metrics_history(limit: Option<usize>) -> Result<Vec<serde_json::Value>, String> {
    if !cfg!(debug_assertions) {
        return Err("仅开发环境可用".to_string());
    }
    let points = poll_metrics::list(limit.unwrap_or(120));
    let mapped = points
        .into_iter()
        .filter_map(|item| serde_json::to_value(item).ok())
        .collect();
    Ok(mapped)
}

#[tauri::command]
pub async fn get_poll_metrics_minute_aggregates(
    limit_minutes: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    if !cfg!(debug_assertions) {
        return Err("仅开发环境可用".to_string());
    }
    let rows = poll_metrics::aggregate_by_minute(limit_minutes.unwrap_or(60));
    let mapped = rows
        .into_iter()
        .filter_map(|item| serde_json::to_value(item).ok())
        .collect();
    Ok(mapped)
}

#[tauri::command]
pub async fn export_poll_metrics(format: String, limit: Option<usize>) -> Result<String, String> {
    if !cfg!(debug_assertions) {
        return Err("仅开发环境可用".to_string());
    }
    let limit = limit.unwrap_or(720);
    match format.as_str() {
        "json" => poll_metrics::export_json(limit),
        "csv" => Ok(poll_metrics::export_csv(limit)),
        _ => Err("不支持的导出格式，仅支持 json/csv".to_string()),
    }
}

#[tauri::command]
pub async fn export_poll_metrics_to_file(
    format: String,
    limit: Option<usize>,
    file_path: String,
) -> Result<String, String> {
    if !cfg!(debug_assertions) {
        return Err("仅开发环境可用".to_string());
    }
    if file_path.trim().is_empty() {
        return Err("导出路径不能为空".to_string());
    }
    let content = export_poll_metrics(format, limit).await?;
    fs::write(&file_path, content).map_err(|e| format!("写入导出文件失败: {}", e))?;
    Ok(file_path)
}

#[tauri::command]
pub async fn get_text_dedup_metrics() -> Result<serde_json::Value, String> {
    if !cfg!(debug_assertions) {
        return Err("仅开发环境可用".to_string());
    }
    serde_json::to_value(get_dedup_scan_metrics()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_app_settings(
    max_items: usize,
    ai_provider: String,
    ai_api_url: String,
    ai_model_name: String,
    ai_api_key: String,
    hot_key: String,
    image_hot_key: String,
    selection_enabled: bool,
    grouped_items_protected_from_limit: bool,
    translation_prompt_template: String,
    explanation_prompt_template: String,
    clipboard_poll_min_interval_ms: u64,
    clipboard_poll_warm_interval_ms: u64,
    clipboard_poll_idle_interval_ms: u64,
    clipboard_poll_max_interval_ms: u64,
    clipboard_poll_report_interval_secs: u64,
    clipboard_poll_metrics_enabled: bool,
    clipboard_poll_metrics_log_level: String,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let version = app.package_info().version.to_string();

    let mut settings = {
        let state_guard = state.lock().unwrap();
        state_guard.settings.clone()
    };

    settings.version = version;
    settings.max_items = max_items;
    settings.selection_enabled = selection_enabled;
    settings.grouped_items_protected_from_limit = grouped_items_protected_from_limit;
    settings.clipboard_poll_min_interval_ms = clipboard_poll_min_interval_ms;
    settings.clipboard_poll_warm_interval_ms = clipboard_poll_warm_interval_ms;
    settings.clipboard_poll_idle_interval_ms = clipboard_poll_idle_interval_ms;
    settings.clipboard_poll_max_interval_ms = clipboard_poll_max_interval_ms;
    settings.clipboard_poll_report_interval_secs = clipboard_poll_report_interval_secs;
    settings.clipboard_poll_metrics_enabled = clipboard_poll_metrics_enabled;
    settings.clipboard_poll_metrics_log_level = clipboard_poll_metrics_log_level;
    settings.translation_prompt_template = if translation_prompt_template.trim().is_empty() {
        default_translation_prompt_template()
    } else {
        translation_prompt_template
    };
    settings.explanation_prompt_template = if explanation_prompt_template.trim().is_empty() {
        default_explanation_prompt_template()
    } else {
        explanation_prompt_template
    };

    if hot_key.is_empty() {
        return Err("快捷键不能为空".to_string());
    }

    if image_hot_key.is_empty() {
        return Err("图片窗口快捷键不能为空".to_string());
    }

    if hot_key == image_hot_key {
        return Err("文字与图片窗口快捷键不能相同".to_string());
    }

    if ai_provider.is_empty() {
        return Err("提供商名称不能为空".to_string());
    }

    if ai_api_key.trim().is_empty() {
        return Err("API密钥不能为空，请填写有效的API密钥".to_string());
    }

    if hot_key != settings.hot_key {
        if app.global_shortcut().is_registered(hot_key.as_str()) {
            return Err("快捷键冲突".to_string());
        }

        app.global_shortcut()
            .unregister(settings.hot_key.as_str())
            .map_err(|e| format!("保存配置失败: {}", e.to_string()))?;
        let app_clone = app.clone();
        let state_clone = state.inner().clone();
        app.global_shortcut()
            .on_shortcut(hot_key.as_str(), move |_app, _shortcut, event| {
                if let ShortcutState::Pressed = event.state {
                    let sg = state_clone.lock().unwrap();
                    if !sg.is_visible && !sg.is_processing_selection {
                        let state_for_window = state_clone.clone();
                        drop(sg);
                        show_clipboard_window(app_clone.clone(), state_for_window);
                        features::mouse_listener::reset_ctrl_key_state();
                    }
                }
            })
            .map_err(|e| e.to_string())?;
    }

    if image_hot_key != settings.image_hot_key {
        if app.global_shortcut().is_registered(image_hot_key.as_str()) {
            return Err("图片窗口快捷键冲突".to_string());
        }

        app.global_shortcut()
            .unregister(settings.image_hot_key.as_str())
            .map_err(|e| format!("保存配置失败: {}", e))?;
        let app_clone = app.clone();
        let state_clone = state.inner().clone();
        app.global_shortcut()
            .on_shortcut(image_hot_key.as_str(), move |_app, _shortcut, event| {
                if let ShortcutState::Pressed = event.state {
                    let sg = state_clone.lock().unwrap();
                    if !sg.is_visible && !sg.is_image_visible && !sg.is_processing_selection {
                        let state_for_window = state_clone.clone();
                        drop(sg);
                        show_image_clipboard_window(app_clone.clone(), state_for_window);
                    }
                }
            })
            .map_err(|e| e.to_string())?;
    }

    settings.hot_key = hot_key;
    settings.image_hot_key = image_hot_key;
    settings.ai_provider = ai_provider.clone();

    settings.migrate_from_old();

    let config = settings
        .provider_configs
        .entry(ai_provider.clone())
        .or_insert_with(|| ProviderConfig::default());

    config.api_url = ai_api_url;
    config.model_name = ai_model_name;

    settings
        .save_current_provider_config(&ai_api_key)
        .map_err(|e| format!("保存提供商配置失败: {}", e))?;

    match settings.get_provider_api_key(&ai_provider) {
        Ok(key) if key == ai_api_key => {
            log::info!("密钥保存验证通过");
        },
        Ok(_) => {
            log::warn!("密钥保存验证失败: 读取到的密钥与保存的不一致");
            return Err("系统凭据管理器异常: 密钥保存验证失败，请重试".to_string());
        },
        Err(e) => {
            log::error!("密钥保存验证错误: {}", e);
            return Err(format!("系统凭据管理器错误: 无法读取刚保存的密钥 ({})", e));
        }
    }

    settings
        .validate()
        .map_err(|e| format!("设置验证失败: {}", e))?;

    save_settings(&settings).map_err(|e| e.to_string())?;

    let selection_enabled = settings.selection_enabled;
    {
        let mut state_guard = state.lock().unwrap();
        {
            let mut manager = state_guard.clipboard_manager.lock().unwrap();
            manager.set_max_items(max_items);
            manager.set_grouped_items_protected_from_limit(grouped_items_protected_from_limit);
        }
        {
            let mut manager = state_guard.image_clipboard_manager.lock().unwrap();
            manager.set_max_items(max_items);
            manager.set_grouped_items_protected_from_limit(grouped_items_protected_from_limit);
        }
        state_guard.settings = settings.clone();
    }

    features::mouse_listener::set_selection_listener_enabled(
        app.clone(),
        state.inner().clone(),
        selection_enabled,
    );

    log::info!(
        "设置保存成功: max_items={}, provider={}",
        max_items,
        ai_provider
    );
    Ok(())
}

#[tauri::command]
pub async fn test_ai_connection(
    ai_api_url: String,
    ai_model_name: String,
    ai_api_key: String,
) -> Result<String, String> {
    let config = AIConfig {
        api_key: ai_api_key,
        base_url: ai_api_url,
        model: ai_model_name,
    };

    let client = AIClient::new(config).map_err(|e| format!("客户端初始化失败: {}", e))?;

    match client.test_connection().await {
        Ok(success) => {
            if success {
                Ok("连接成功".to_string())
            } else {
                Err("连接测试未返回预期结果".to_string())
            }
        }
        Err(e) => {
            log::error!("AI连接测试失败: {}", e);
            Err(format!("连接测试失败: {}", e))
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
            let error_msg = format!("复制文本失败: {}", e);
            log::error!("{}", error_msg);
            Err(error_msg)
        }
    }
}

#[tauri::command]
pub async fn copy_and_paste_text(text: String, app: AppHandle) -> Result<(), String> {
    app.clipboard()
        .write_text(text)
        .map_err(|e| format!("复制文本失败: {}", e))?;

    if let Some(window) = app.get_webview_window("result_translation") {
        let _ = window.hide();
    }
    if let Some(window) = app.get_webview_window("result_explanation") {
        let _ = window.hide();
    }

    thread::sleep(Duration::from_millis(80));
    crate::ui::window_manager::simulate_paste().map_err(|e| format!("自动粘贴失败: {}", e))?;
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
        return Err("提供商名称不能为空".to_string());
    }

    let is_builtin = matches!(provider.as_str(), "deepseek" | "qwen" | "xiaomimimo");
    if is_builtin {
        return Err("内置提供商不支持删除".to_string());
    }

    let mut settings = {
        let state_guard = state.lock().unwrap();
        state_guard.settings.clone()
    };

    if settings.provider_configs.remove(&provider).is_none() {
        return Err("未找到该提供商配置".to_string());
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

    save_settings(&settings).map_err(|e| e.to_string())?;

    {
        let mut state_guard = state.lock().unwrap();
        state_guard.settings = settings;
    }

    Ok(())
}

/// 获取所有已配置的提供商列表（包括自定义提供商）
#[tauri::command]
pub async fn get_all_configured_providers(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<Vec<(String, String)>, String> {
    let state_guard = state.lock().unwrap();
    let settings = &state_guard.settings;

    let mut providers: Vec<(String, String)> = Vec::new();

    for (provider_key, _) in &settings.provider_configs {
        providers.push((provider_key.clone(), provider_key.clone()));
    }

    Ok(providers)
}
