use crate::core::app_state::AppState as SharedAppState;
use crate::core::error::{AppError, AppResult};
use crate::core::error_codes::AppErrorKind;
use crate::core::perf_metrics::record_perf_metric;
use crate::services::ai_client::{AIClient, AIConfig};
use crate::sync::{lock_arc_mutex, Mutex};
use crate::ui::window_manager::{
    hide_selection_toolbar_impl, show_result_window,
};
use crate::utils::utils_helpers::{
    default_explanation_prompt_template, default_translation_prompt_template,
};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};

/// 缓存的AI客户端（当配置不变时复用），使用 Arc 避免每次命中时 clone 配置字符串
static CACHED_AI_CLIENT: LazyLock<Mutex<Option<(Arc<AIConfig>, Arc<AIClient>)>>> =
    LazyLock::new(|| Mutex::new(None));

/// 强制清除 AI 客户端缓存，下次请求会重新从凭据管理器读取 API Key 并创建新客户端
pub fn invalidate_ai_client_cache() {
    let mut cache = CACHED_AI_CLIENT.lock().unwrap_or_else(|never| match never {});
    *cache = None;
    log::info!("AI客户端缓存已强制清除");
}

async fn build_ai_config(_state: &Arc<Mutex<SharedAppState>>) -> AppResult<AIConfig> {
    let provider_key = crate::utils::ai_store::get_current_provider().await;
    if provider_key.is_empty() {
        return Err(AppErrorKind::AiNotConfigured.to_app_error());
    }

    let config = crate::utils::ai_store::get_provider_config(&provider_key)
        .await
        .ok_or_else(|| AppErrorKind::AiProviderNotFound.to_app_error())?;

    if config.api_url.is_empty() {
        return Err(AppErrorKind::AiApiUrlEmpty.to_app_error());
    }
    if config.model_name.is_empty() {
        return Err(AppErrorKind::AiModelNameEmpty.to_app_error());
    }

    let is_secure = config.api_url.starts_with("https://");
    let is_localhost = config.api_url.starts_with("http://localhost")
        || config.api_url.starts_with("http://127.0.0.1")
        || config.api_url.starts_with("http://[::1]");
    if !is_secure && !is_localhost {
        return Err(AppErrorKind::AiApiUrlInvalid.to_app_error());
    }

    log::info!("正在验证提供商 {} 的配置", provider_key);
    if config.api_key.is_empty() {
        log::warn!("提供商 {} 的API密钥为空", provider_key);
        return Err(AppErrorKind::AiApiKeyNotConfigured.to_app_error());
    }
    let mask = if config.api_key.chars().count() > 6 {
        let tail: String = config.api_key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
        format!("***{}", tail)
    } else { "***".to_string() };
    log::info!("提供商 {} 配置验证通过，密钥: {}", provider_key, mask);

    Ok(AIConfig {
        api_key: config.api_key,
        base_url: config.api_url,
        model: config.model_name,
    })
}

/// 获取或创建AI客户端（配置不变时复用缓存的客户端）
pub async fn get_or_create_ai_client(state: Arc<Mutex<SharedAppState>>) -> AppResult<Arc<AIClient>> {
    let current_config = build_ai_config(&state).await?;

    // 检查缓存的客户端是否仍然有效（比较 Arc 内的配置值）
    {
        let cache = CACHED_AI_CLIENT.lock().unwrap_or_else(|never| match never {});
        if let Some((cached_config, cached_client)) = cache.as_ref() {
            if cached_config.api_key == current_config.api_key
                && cached_config.base_url == current_config.base_url
                && cached_config.model == current_config.model
            {
                log::info!("AI客户端缓存命中，复用已有连接");
                return Ok(Arc::clone(cached_client));
            }
            log::info!("AI客户端配置变更，创建新连接 (key变更={}, url变更={}, model变更={})",
                cached_config.api_key != current_config.api_key,
                cached_config.base_url != current_config.base_url,
                cached_config.model != current_config.model);
        } else {
            log::info!("AI客户端缓存为空，首次创建");
        }
    }

    // 配置已变更，创建新客户端并包装为 Arc
    let client = AIClient::new(current_config.clone()).map_err(|e| {
        AppErrorKind::AiClientInitFailed.to_app_error_with_details(e.to_string())
    })?;
    let client = Arc::new(client);
    let config = Arc::new(current_config);

    // 更新缓存
    {
        let mut cache = CACHED_AI_CLIENT.lock().unwrap_or_else(|never| match never {});
        *cache = Some((config, Arc::clone(&client)));
    }

    Ok(client)
}

fn fill_prompt_template(
    template: &str,
    text: &str,
    source_language: Option<&str>,
    target_language: &str,
) -> String {
    let source = source_language.unwrap_or("自动识别");
    let mut prompt = template.replace("{source_language}", source);
    prompt = prompt.replace("{target_language}", target_language);
    prompt = prompt.replace("{text}", text);
    prompt
}

fn next_ai_operation_id(state: &Arc<Mutex<SharedAppState>>) -> u64 {
    let mut state_guard = lock_arc_mutex(state);
    state_guard.ai_request_seq = state_guard.ai_request_seq.wrapping_add(1);
    state_guard.ai_request_seq
}

#[derive(Clone)]
enum AiStreamKind {
    Translation,
    Explanation,
    CustomPrompt(String),
}

impl AiStreamKind {
    fn kind_name(&self) -> String {
        match self {
            Self::Translation => "translation".to_string(),
            Self::Explanation => "explanation".to_string(),
            Self::CustomPrompt(_) => "custom_prompt".to_string(),
        }
    }

    fn window_title(&self) -> String {
        match self {
            Self::Translation => "翻译结果".to_string(),
            Self::Explanation => "解释结果".to_string(),
            Self::CustomPrompt(name) => format!("{} 结果", name),
        }
    }

    fn display_name(&self) -> String {
        match self {
            Self::Translation => "翻译".to_string(),
            Self::Explanation => "解释".to_string(),
            Self::CustomPrompt(name) => name.clone(),
        }
    }
}

fn set_active_operation(state: &Arc<Mutex<SharedAppState>>, kind: &AiStreamKind, operation_id: u64) {
    let mut state_guard = lock_arc_mutex(state);
    match kind {
        AiStreamKind::Translation => state_guard.active_translation_op_id = operation_id,
        AiStreamKind::Explanation => state_guard.active_explanation_op_id = operation_id,
        AiStreamKind::CustomPrompt(_) => state_guard.active_custom_prompt_op_id = operation_id,
    }
}

fn is_operation_active(
    state: &Arc<Mutex<SharedAppState>>,
    kind: &AiStreamKind,
    operation_id: u64,
) -> bool {
    let state_guard = lock_arc_mutex(state);
    match kind {
        AiStreamKind::Translation => state_guard.active_translation_op_id == operation_id,
        AiStreamKind::Explanation => state_guard.active_explanation_op_id == operation_id,
        AiStreamKind::CustomPrompt(_) => state_guard.active_custom_prompt_op_id == operation_id,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamTranslateRequest {
    pub text: String,
    pub source_language: String,
    pub target_language: String,
    #[serde(default)]
    pub scene_hint: Option<String>,
    #[serde(default)]
    pub op_id: Option<u64>,
    #[serde(default)]
    pub window_label: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamExplainRequest {
    pub text: String,
    pub target_language: String,
    #[serde(default)]
    pub scene_hint: Option<String>,
    #[serde(default)]
    pub op_id: Option<u64>,
    #[serde(default)]
    pub window_label: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamCustomPromptRequest {
    pub text: String,
    pub prompt_name: String,
    #[serde(default)]
    pub target_language: Option<String>,
    #[serde(default)]
    pub scene_hint: Option<String>,
    #[serde(default)]
    pub op_id: Option<u64>,
    #[serde(default)]
    pub window_label: Option<String>,
}

struct StreamExecutionRequest {
    text: String,
    source_language: Option<String>,
    target_language: String,
    scene_hint: Option<String>,
    op_id: Option<u64>,
    window_label: Option<String>,
}

async fn execute_stream_request(
    kind: AiStreamKind,
    request: StreamExecutionRequest,
    app: AppHandle,
    state_arc: Arc<Mutex<SharedAppState>>,
) -> Result<(), AppError> {
    let started_at = Instant::now();
    let text = request.text.trim().to_string();
    if text.is_empty() {
        return Err(AppErrorKind::AiTextEmpty.to_app_error());
    }

    let configured_prompt = {
        let state_guard = lock_arc_mutex(&state_arc);
        match &kind {
            AiStreamKind::Translation => state_guard.settings.translation_prompt_template.clone(),
            AiStreamKind::Explanation => state_guard.settings.explanation_prompt_template.clone(),
            AiStreamKind::CustomPrompt(name) => {
                state_guard.settings.selection_custom_prompts.iter().find(|p| &p.name == name).map(|p| p.prompt.clone()).unwrap_or_default()
            }
        }
    };

    let operation_id = request
        .op_id
        .unwrap_or_else(|| next_ai_operation_id(&state_arc));
    set_active_operation(&state_arc, &kind, operation_id);
    log::info!("[AI流式] 开始获取AI客户端...");
    let client = get_or_create_ai_client(state_arc.clone()).await?;
    log::info!("[AI流式] AI客户端就绪，开始创建结果窗口...");

    // 显示结果窗口并获取窗口标签
    let window_label = show_result_window(
        kind.window_title().to_string(),
        "".to_string(),
        kind.kind_name().to_string(),
        text.clone(),
        request.target_language.clone(),
        app.clone(),
        request.window_label.clone(),
    )
    .await
        .map_err(|e| AppErrorKind::InternalError.to_app_error_with_details(e))?;

    hide_selection_toolbar_impl(app.clone());

    let source_language_name = request
        .source_language
        .unwrap_or_default()
        .trim()
        .to_string();
    let prompt_template = if configured_prompt.trim().is_empty() {
        match kind {
            AiStreamKind::Translation => default_translation_prompt_template(),
            AiStreamKind::Explanation => default_explanation_prompt_template(),
            AiStreamKind::CustomPrompt(_) => "{text}".to_string(),
        }
    } else {
        configured_prompt
    };

    let text_for_prompt = if let Some(scene_hint) = request.scene_hint {
        let hint = scene_hint.trim();
        if hint.is_empty() {
            text.clone()
        } else {
            format!("{}\n\n附加要求：\n{}", text, hint)
        }
    } else {
        text.clone()
    };

    let messages = fill_prompt_template(
        &prompt_template,
        &text_for_prompt,
        if source_language_name.is_empty() {
            None
        } else {
            Some(source_language_name.as_str())
        },
        &request.target_language,
    );

    // 发送清理事件到新创建的窗口
    if let Some(window) = app.clone().get_webview_window(&window_label) {
        if let Err(e) = window.emit(
            "result-clean",
            serde_json::json!({
                "type": kind.kind_name(),
                "opId": operation_id,
                "windowLabel": window_label
            }),
        ) {
            log::warn!("发送结果清理事件失败: {}", e);
        }
    }

    let state_for_stream = state_arc.clone();
    let first_chunk_recorded = AtomicBool::new(false);
    let result = client
        .generate_text_stream(messages.as_str(), Some(1000), |content_chunk| {
            if !content_chunk.is_empty() && !first_chunk_recorded.swap(true, Ordering::Relaxed) {
                record_perf_metric(
                    match kind {
                        AiStreamKind::Translation => "ai.translation.first_chunk",
                        AiStreamKind::Explanation => "ai.explanation.first_chunk",
                        AiStreamKind::CustomPrompt(_) => "ai.custom_prompt.first_chunk",
                    },
                    match kind {
                        AiStreamKind::Translation => "AI翻译首字返回",
                        AiStreamKind::Explanation => "AI解释首字返回",
                        AiStreamKind::CustomPrompt(_) => "AI自定义首字返回",
                    },
                    started_at.elapsed().as_millis() as u64,
                    true,
                    None,
                );
            }
            if !is_operation_active(&state_for_stream, &kind, operation_id) {
                log::info!(
                    "{}流已被新请求接管，停止旧流: op_id={}",
                    kind.display_name(),
                    operation_id
                );
                return false;
            }
            // 使用新创建的窗口标签发送更新事件
            if let Some(window) = app.get_webview_window(&window_label) {
                let payload = serde_json::json!({
                    "type": kind.kind_name(),
                    "content": content_chunk,
                    "windowLabel": window_label
                });
                if let Err(e) = window.emit("result-update", payload) {
                    log::error!("更新{}结果窗口失败: {}", kind.display_name(), e);
                }
            } else {
                log::error!("{}窗口不存在: {}", kind.kind_name(), window_label);
            }
            true
        })
        .await;

    match result {
        Ok(()) => {
            record_perf_metric(
                match kind {
                    AiStreamKind::Translation => "ai.translation.total",
                    AiStreamKind::Explanation => "ai.explanation.total",
                    AiStreamKind::CustomPrompt(_) => "ai.custom_prompt.total",
                },
                match kind {
                    AiStreamKind::Translation => "AI翻译总耗时",
                    AiStreamKind::Explanation => "AI解释总耗时",
                    AiStreamKind::CustomPrompt(_) => "AI自定义总耗时",
                },
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            if is_operation_active(&state_arc, &kind, operation_id) {
                log::info!("{}完成: op_id={}", kind.display_name(), operation_id);
            } else {
                log::info!(
                    "{}请求已过期并结束: op_id={}",
                    kind.display_name(),
                    operation_id
                );
            }
        }
        Err(e) => {
            let error_message = e.to_string();
            if !first_chunk_recorded.load(Ordering::Relaxed) {
                record_perf_metric(
                    match kind {
                        AiStreamKind::Translation => "ai.translation.first_chunk",
                        AiStreamKind::Explanation => "ai.explanation.first_chunk",
                        AiStreamKind::CustomPrompt(_) => "ai.custom_prompt.first_chunk",
                    },
                    match kind {
                        AiStreamKind::Translation => "AI翻译首字返回",
                        AiStreamKind::Explanation => "AI解释首字返回",
                        AiStreamKind::CustomPrompt(_) => "AI自定义首字返回",
                    },
                    started_at.elapsed().as_millis() as u64,
                    false,
                    Some(error_message.clone()),
                );
            }
            record_perf_metric(
                match kind {
                    AiStreamKind::Translation => "ai.translation.total",
                    AiStreamKind::Explanation => "ai.explanation.total",
                    AiStreamKind::CustomPrompt(_) => "ai.custom_prompt.total",
                },
                match kind {
                    AiStreamKind::Translation => "AI翻译总耗时",
                    AiStreamKind::Explanation => "AI解释总耗时",
                    AiStreamKind::CustomPrompt(_) => "AI自定义总耗时",
                },
                started_at.elapsed().as_millis() as u64,
                false,
                Some(error_message.clone()),
            );
            if !is_operation_active(&state_arc, &kind, operation_id) {
                log::info!(
                    "忽略过期{}错误: op_id={}, error={}",
                    kind.display_name(),
                    operation_id,
                    e
                );
                return Ok(());
            }
            let error_msg = if let Some(ref details) = e.details {
                format!("{}失败: {} | {}", kind.display_name(), e.message, details)
            } else {
                format!("{}失败: {}", kind.display_name(), e.message)
            };
            // 发送错误信息到新创建的窗口
            if let Some(window) = app.get_webview_window(&window_label) {
                if let Err(e) = window.emit(
                    "result-update",
                    serde_json::json!({
                        "type": kind.kind_name(),
                        "content": error_msg.clone(),
                        "windowLabel": window_label
                    }),
                ) {
                    log::warn!("发送AI错误结果事件失败: {}", e);
                }
            }
            log::error!("{}", error_msg);
        }
    }

    Ok(())
}

/// 流式翻译文本
#[tauri::command]
pub async fn stream_translate_text(
    request: StreamTranslateRequest,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), AppError> {
    execute_stream_request(
        AiStreamKind::Translation,
        StreamExecutionRequest {
            text: request.text,
            source_language: Some(request.source_language),
            target_language: request.target_language,
            scene_hint: request.scene_hint,
            op_id: request.op_id,
            window_label: request.window_label,
        },
        app,
        state.inner().clone(),
    )
    .await
}

/// 流式解释文本
#[tauri::command]
pub async fn stream_explain_text(
    request: StreamExplainRequest,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), AppError> {
    execute_stream_request(
        AiStreamKind::Explanation,
        StreamExecutionRequest {
            text: request.text,
            source_language: None,
            target_language: request.target_language,
            scene_hint: request.scene_hint,
            op_id: request.op_id,
            window_label: request.window_label,
        },
        app,
        state.inner().clone(),
    )
    .await
}

/// 流式执行自定义 Prompt
#[tauri::command]
pub async fn stream_custom_prompt_text(
    request: StreamCustomPromptRequest,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), AppError> {
    execute_stream_request(
        AiStreamKind::CustomPrompt(request.prompt_name),
        StreamExecutionRequest {
            text: request.text,
            source_language: None,
            target_language: request.target_language.unwrap_or_else(|| "中文".to_string()),
            scene_hint: request.scene_hint,
            op_id: request.op_id,
            window_label: request.window_label,
        },
        app,
        state.inner().clone(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::app_state::AppState;

    #[test]
    fn test_fill_prompt_template_all_placeholders() {
        let template = "请把{source_language}翻译成{target_language}：{text}";
        let out = fill_prompt_template(template, "你好", Some("中文"), "英文");
        assert_eq!(out, "请把中文翻译成英文：你好");
    }

    #[test]
    fn test_fill_prompt_template_default_source() {
        let template = "源语言:{source_language}";
        let out = fill_prompt_template(template, "x", None, "en");
        assert_eq!(out, "源语言:自动识别");
    }

    #[test]
    fn test_fill_prompt_template_missing_placeholder_untouched() {
        let out = fill_prompt_template("没有占位符", "文本", None, "en");
        assert_eq!(out, "没有占位符");
    }

    #[test]
    fn test_ai_stream_kind_names() {
        assert_eq!(AiStreamKind::Translation.kind_name(), "translation");
        assert_eq!(AiStreamKind::Explanation.kind_name(), "explanation");
        assert_eq!(
            AiStreamKind::CustomPrompt("总结".to_string()).kind_name(),
            "custom_prompt"
        );
    }

    #[test]
    fn test_ai_stream_kind_titles() {
        assert_eq!(AiStreamKind::Translation.window_title(), "翻译结果");
        assert_eq!(AiStreamKind::Explanation.window_title(), "解释结果");
        assert_eq!(
            AiStreamKind::CustomPrompt("总结".to_string()).window_title(),
            "总结 结果"
        );
    }

    #[test]
    fn test_ai_stream_kind_display_names() {
        assert_eq!(AiStreamKind::Translation.display_name(), "翻译");
        assert_eq!(AiStreamKind::Explanation.display_name(), "解释");
        assert_eq!(AiStreamKind::CustomPrompt("润色".to_string()).display_name(), "润色");
    }

    #[test]
    fn test_next_and_active_operation() {
        let state = Arc::new(Mutex::new(AppState::default()));
        let id = next_ai_operation_id(&state);
        assert_eq!(id, 1);
        let id2 = next_ai_operation_id(&state);
        assert_eq!(id2, 2);

        set_active_operation(&state, &AiStreamKind::Translation, id);
        assert!(is_operation_active(&state, &AiStreamKind::Translation, id));
        assert!(!is_operation_active(&state, &AiStreamKind::Translation, id2));
        assert!(!is_operation_active(&state, &AiStreamKind::Explanation, id));
    }

    #[test]
    fn test_next_ai_operation_id_wraps() {
        let state = Arc::new(Mutex::new(AppState::default()));
        {
            let mut g = lock_arc_mutex(&state);
            g.ai_request_seq = u64::MAX;
        }
        let id = next_ai_operation_id(&state);
        assert_eq!(id, 0); // wrapping_add
    }
}
