use crate::core::app_state::AppState as SharedAppState;
use crate::core::error::{AppError, AppResult, ErrorCode};
use crate::core::perf_metrics::record_perf_metric;
use crate::services::ai_client::{AIClient, AIConfig};
use crate::sync::Mutex;
use crate::ui::window_manager::{
    hide_selection_toolbar_impl, show_result_window, update_result_window,
};
use crate::utils::utils_helpers::{
    default_explanation_prompt_template, default_translation_prompt_template,
};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};

fn lock_state<'a>(
    state: &'a Arc<Mutex<SharedAppState>>,
) -> crate::sync::MutexGuard<'a, SharedAppState> {
    state.lock().unwrap()
}

fn build_ai_config(state: &Arc<Mutex<SharedAppState>>) -> AppResult<AIConfig> {
    let (settings_snapshot, provider_key, api_url, model_name) = {
        let state_guard = lock_state(state);
        let settings_snapshot = state_guard.settings.clone();

        if settings_snapshot.ai_provider.is_empty() {
            return Err(AppError::new(
                ErrorCode::ConfigError,
                "未配置AI提供商，请在设置中选择提供商",
            ));
        }

        if !settings_snapshot
            .provider_configs
            .contains_key(&settings_snapshot.ai_provider)
        {
            return Err(AppError::new(
                ErrorCode::ConfigError,
                format!(
                    "未找到提供商 '{}' 的配置，请在设置中配置API信息",
                    settings_snapshot.ai_provider
                ),
            ));
        }

        let provider_key = settings_snapshot.ai_provider.clone();
        let provider_config = settings_snapshot
            .get_current_provider_config()
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::ConfigError,
                    format!(
                        "未找到提供商 '{}' 的配置，请在设置中配置API信息",
                        provider_key
                    ),
                )
            })?;

        if provider_config.api_url.is_empty() {
            return Err(AppError::new(
                ErrorCode::ConfigError,
                "API地址不能为空，请在设置中填写正确的API地址",
            ));
        }

        if provider_config.model_name.is_empty() {
            return Err(AppError::new(
                ErrorCode::ConfigError,
                "模型名称不能为空，请在设置中填写正确的模型名称",
            ));
        }

        let api_url = provider_config.api_url.clone();
        let model_name = provider_config.model_name.clone();

        (settings_snapshot, provider_key, api_url, model_name)
    };

    if !api_url.starts_with("https://") {
        return Err(AppError::new(
            ErrorCode::ConfigError,
            "API地址格式不正确，请确保以 https:// 开头",
        ));
    }

    log::info!("正在验证提供商 {} 的配置", provider_key);
    let api_key = settings_snapshot
        .get_provider_api_key(&provider_key)
        .map_err(|e| {
            log::error!("读取密钥库失败: {}", e);
            AppError::new(ErrorCode::SystemError, format!("读取密钥库失败: {}", e))
        })?;

    if api_key.is_empty() {
        log::warn!("提供商 {} 的API密钥为空", provider_key);
        return Err(AppError::new(
            ErrorCode::ConfigError,
            "API密钥未配置或无效，请在设置中填写正确的API密钥",
        ));
    }
    log::info!("提供商 {} 配置验证通过", provider_key);

    Ok(AIConfig {
        api_key,
        base_url: api_url,
        model: model_name,
    })
}

/// 获取或创建AI客户端
pub async fn get_or_create_ai_client(state: Arc<Mutex<SharedAppState>>) -> AppResult<AIClient> {
    let current_config = build_ai_config(&state)?;
    let client = AIClient::new(current_config).map_err(|e| {
        AppError::new(ErrorCode::SystemError, "客户端初始化失败").with_details(e.to_string())
    })?;
    Ok(client)
}

fn fill_prompt_template(
    template: &str,
    text: &str,
    source_language: Option<&str>,
    target_language: &str,
) -> String {
    let mut prompt = template.replace("{text}", text);
    let source = source_language.unwrap_or("自动识别");
    prompt = prompt.replace("{source_language}", source);
    prompt.replace("{target_language}", target_language)
}

fn next_ai_operation_id(state: &Arc<Mutex<SharedAppState>>) -> u64 {
    let mut state_guard = lock_state(state);
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

    fn window_label(&self) -> String {
        match self {
            Self::Translation => "result_translation".to_string(),
            Self::Explanation => "result_explanation".to_string(),
            Self::CustomPrompt(_) => "result_custom_prompt".to_string(),
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
    let mut state_guard = lock_state(state);
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
    let state_guard = lock_state(state);
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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamCustomPromptRequest {
    pub text: String,
    pub prompt_name: String,
    #[serde(default)]
    pub scene_hint: Option<String>,
    #[serde(default)]
    pub op_id: Option<u64>,
}

struct StreamExecutionRequest {
    text: String,
    source_language: Option<String>,
    target_language: String,
    scene_hint: Option<String>,
    op_id: Option<u64>,
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
        let msg = match kind {
            AiStreamKind::Translation => "文本为空，无法翻译",
            AiStreamKind::Explanation => "文本为空，无法解释",
            AiStreamKind::CustomPrompt(_) => "文本为空，无法执行",
        };
        return Err(AppError::new(ErrorCode::ValidationError, msg));
    }

    let configured_prompt = {
        let state_guard = lock_state(&state_arc);
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
    let client: AIClient = get_or_create_ai_client(state_arc.clone()).await?;

    show_result_window(
        kind.window_title().to_string(),
        "".to_string(),
        kind.kind_name().to_string(),
        text.clone(),
        request.target_language.clone(),
        app.clone(),
    )
    .await
    .map_err(|e| AppError::new(ErrorCode::SystemError, e))?;

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

    if let Some(window) = app.clone().get_webview_window(kind.window_label()) {
        let _ = window.emit(
            "result-clean",
            serde_json::json!({
                "type": kind.kind_name(),
                "opId": operation_id
            }),
        );
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
            if let Some(window) = app.get_webview_window(kind.window_label()) {
                let payload = serde_json::json!({
                    "type": kind.kind_name(),
                    "content": content_chunk
                });
                if let Err(e) = window.emit("result-update", payload) {
                    log::error!("更新{}结果窗口失败: {}", kind.display_name(), e);
                }
            } else {
                log::error!("{}窗口不存在", kind.kind_name());
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
            let error_msg = format!("{}失败: {}", kind.display_name(), e.message);
            update_result_window(error_msg.clone(), kind.kind_name().to_string(), app)
                .await
                .map_err(|e| AppError::new(ErrorCode::SystemError, e))?;
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
            target_language: "中文".to_string(), // 或者由前端传过来，目前简化处理
            scene_hint: request.scene_hint,
            op_id: request.op_id,
        },
        app,
        state.inner().clone(),
    )
    .await
}
