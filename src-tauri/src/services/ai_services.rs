use crate::core::app_state::AppState as SharedAppState;
use crate::core::error::{AppError, AppResult, ErrorCode};
use crate::services::ai_client::{AIClient, AIConfig};
use crate::ui::window_manager::{hide_selection_toolbar_impl, show_result_window, update_result_window};
use crate::utils::utils_helpers::{
    default_explanation_prompt_template, default_translation_prompt_template,
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

/// 验证AI提供商配置
fn validate_provider_config(state: &Arc<Mutex<SharedAppState>>) -> AppResult<()> {
    let state_guard = state.lock().unwrap();
    let settings = &state_guard.settings;

    if settings.ai_provider.is_empty() {
        return Err(AppError::new(ErrorCode::ConfigError, "未配置AI提供商，请在设置中选择提供商"));
    }

    if !settings.provider_configs.contains_key(&settings.ai_provider) {
        return Err(AppError::new(ErrorCode::ConfigError, format!("未找到提供商 '{}' 的配置，请在设置中配置API信息", settings.ai_provider)));
    }

    let provider_config = settings.get_current_provider_config()
        .ok_or_else(|| AppError::new(ErrorCode::ConfigError, format!("未找到提供商 '{}' 的配置，请在设置中配置API信息", settings.ai_provider)))?;

    if provider_config.api_url.is_empty() {
        return Err(AppError::new(ErrorCode::ConfigError, "API地址不能为空，请在设置中填写正确的API地址"));
    }

    if provider_config.model_name.is_empty() {
        return Err(AppError::new(ErrorCode::ConfigError, "模型名称不能为空，请在设置中填写正确的模型名称"));
    }

    log::info!("正在验证提供商 {} 的配置", settings.ai_provider);
    let api_key = settings.get_provider_api_key(&settings.ai_provider)
        .map_err(|e| {
            log::error!("读取密钥库失败: {}", e);
            AppError::new(ErrorCode::SystemError, format!("读取密钥库失败: {}", e))
        })?;

    if api_key.is_empty() {
        log::warn!("提供商 {} 的API密钥为空", settings.ai_provider);
        return Err(AppError::new(ErrorCode::ConfigError, "API密钥未配置或无效，请在设置中填写正确的API密钥"));
    }
    log::info!("提供商 {} 配置验证通过", settings.ai_provider);

    if !provider_config.api_url.starts_with("http://") && !provider_config.api_url.starts_with("https://") {
        return Err(AppError::new(ErrorCode::ConfigError, "API地址格式不正确，请确保以 http:// 或 https:// 开头"));
    }

    Ok(())
}

/// 获取或创建AI客户端
pub async fn get_or_create_ai_client(state: Arc<Mutex<SharedAppState>>) -> AppResult<AIClient> {
    validate_provider_config(&state)?;
    
    let current_config = {
        let state_guard = state.lock().unwrap();
        let api_key = state_guard
            .settings
            .get_provider_api_key(&state_guard.settings.ai_provider)
            .map_err(|e| AppError::new(ErrorCode::SystemError, format!("获取API密钥失败: {}", e)))?;
        if api_key.is_empty() {
            return Err(AppError::new(ErrorCode::ConfigError, "API密钥为空，无法创建客户端"));
        }
        let provider_config = state_guard.settings.get_current_provider_config()
            .ok_or(AppError::new(ErrorCode::ConfigError, "获取当前提供商配置失败"))?;
        AIConfig {
            api_key,
            base_url: provider_config.api_url.clone(),
            model: provider_config.model_name.clone(),
        }
    };
    let client = AIClient::new(current_config).map_err(|e| AppError::new(ErrorCode::SystemError, format!("客户端初始化失败: {}", e)))?;
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
    let mut state_guard = state.lock().unwrap();
    state_guard.ai_request_seq = state_guard.ai_request_seq.wrapping_add(1);
    state_guard.ai_request_seq
}

#[derive(Clone, Copy)]
enum AiStreamKind {
    Translation,
    Explanation,
}

impl AiStreamKind {
    fn kind_name(self) -> &'static str {
        match self {
            Self::Translation => "translation",
            Self::Explanation => "explanation",
        }
    }

    fn window_label(self) -> &'static str {
        match self {
            Self::Translation => "result_translation",
            Self::Explanation => "result_explanation",
        }
    }

    fn window_title(self) -> &'static str {
        match self {
            Self::Translation => "翻译结果",
            Self::Explanation => "解释结果",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Translation => "翻译",
            Self::Explanation => "解释",
        }
    }
}

fn set_active_operation(state: &Arc<Mutex<SharedAppState>>, kind: AiStreamKind, operation_id: u64) {
    let mut state_guard = state.lock().unwrap();
    match kind {
        AiStreamKind::Translation => state_guard.active_translation_op_id = operation_id,
        AiStreamKind::Explanation => state_guard.active_explanation_op_id = operation_id,
    }
}

fn is_operation_active(state: &Arc<Mutex<SharedAppState>>, kind: AiStreamKind, operation_id: u64) -> bool {
    let state_guard = state.lock().unwrap();
    match kind {
        AiStreamKind::Translation => state_guard.active_translation_op_id == operation_id,
        AiStreamKind::Explanation => state_guard.active_explanation_op_id == operation_id,
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
    let text = request.text.trim().to_string();
    if text.is_empty() {
        let msg = match kind {
            AiStreamKind::Translation => "文本为空，无法翻译",
            AiStreamKind::Explanation => "文本为空，无法解释",
        };
        return Err(AppError::new(ErrorCode::ValidationError, msg));
    }

    let configured_prompt = {
        let state_guard = state_arc.lock().unwrap();
        match kind {
            AiStreamKind::Translation => state_guard.settings.translation_prompt_template.clone(),
            AiStreamKind::Explanation => state_guard.settings.explanation_prompt_template.clone(),
        }
    };

    let operation_id = request.op_id.unwrap_or_else(|| next_ai_operation_id(&state_arc));
    set_active_operation(&state_arc, kind, operation_id);
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
    let result = client
        .generate_text_stream(messages.as_str(), Some(1000), |content_chunk| {
            if !is_operation_active(&state_for_stream, kind, operation_id) {
                log::info!(
                    "{}流已被新请求接管，停止旧流: op_id={}",
                    kind.display_name(),
                    operation_id
                );
                return false;
            }
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    update_result_window(content_chunk, kind.kind_name().to_string(), app_clone).await
                {
                    log::error!("更新{}结果窗口失败: {}", kind.display_name(), e);
                }
            });
            true
        })
        .await;

    match result {
        Ok(()) => {
            if is_operation_active(&state_arc, kind, operation_id) {
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
            if !is_operation_active(&state_arc, kind, operation_id) {
                log::info!(
                    "忽略过期{}错误: op_id={}, error={}",
                    kind.display_name(),
                    operation_id,
                    e
                );
                return Ok(());
            }
            let error_msg = format!("{}失败: {}", kind.display_name(), e);
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
