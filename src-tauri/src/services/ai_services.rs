use crate::core::app_state::AppState as SharedAppState;
use crate::core::error::{AppError, AppResult, ErrorCode};
use crate::services::ai_client::{AIClient, AIConfig};
use crate::ui::window_manager::{hide_selection_toolbar_impl, show_result_window, update_result_window};
use crate::utils::utils_helpers::{
    default_explanation_prompt_template, default_translation_prompt_template,
};
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

/// 流式翻译文本
#[tauri::command]
pub async fn stream_translate_text(
    text: String,
    source_language: String,
    target_language: String,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), AppError> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::new(ErrorCode::ValidationError, "文本为空，无法翻译"));
    }
    let configured_translation_prompt = {
        let state_guard = state.lock().unwrap();
        state_guard.settings.translation_prompt_template.clone()
    };
    let client: AIClient = get_or_create_ai_client(state.inner().clone()).await?;

    show_result_window(
        "翻译结果".to_string(),
        "".to_string(),
        "translation".to_string(),
        text.clone(),
        app.clone(),
    )
        .await
        .map_err(|e| AppError::new(ErrorCode::SystemError, e))?;
    hide_selection_toolbar_impl(app.clone());
    let source_language_name = source_language.trim().to_string();
    let target_language_name = target_language;
    let prompt_template = if configured_translation_prompt.trim().is_empty() {
        default_translation_prompt_template()
    } else {
        configured_translation_prompt
    };
    let messages = fill_prompt_template(
        &prompt_template,
        &text,
        if source_language_name.is_empty() {
            None
        } else {
            Some(source_language_name.as_str())
        },
        &target_language_name,
    );
    if let Some(window) = app.clone().get_webview_window("result_translation") {
        let _ = window.emit("result-clean", serde_json::json!({
            "type": "translation"
        }));
    }
    let result = client
        .generate_text_stream(messages.as_str(), Some(1000), |content_chunk| {
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    update_result_window(content_chunk, "translation".to_string(), app_clone).await
                {
                    log::error!("发送数据失败:{}", e);
                }
            });
        })
        .await;
    match result {
        Ok(()) => {
            log::info!("翻译完成");
        }
        Err(e) => {
            let error_msg = format!("翻译失败: {}", e);
            update_result_window(error_msg.clone(), "translation".to_string(), app).await
                .map_err(|e| AppError::new(ErrorCode::SystemError, e))?;
            log::error!("翻译失败: {}", error_msg);
        }
    }
    Ok(())
}

/// 流式解释文本
#[tauri::command]
pub async fn stream_explain_text(
    text: String,
    target_language: String,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), AppError> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::new(ErrorCode::ValidationError, "文本为空，无法解释"));
    }
    let configured_explanation_prompt = {
        let state_guard = state.lock().unwrap();
        state_guard.settings.explanation_prompt_template.clone()
    };
    let client: AIClient = get_or_create_ai_client(state.inner().clone()).await?;
    show_result_window(
        "解释结果".to_string(),
        "".to_string(),
        "explanation".to_string(),
        text.clone(),
        app.clone(),
    )
        .await
        .map_err(|e| AppError::new(ErrorCode::SystemError, e))?;
    hide_selection_toolbar_impl(app.clone());
    let target_language_name = target_language;

    let prompt_template = if configured_explanation_prompt.trim().is_empty() {
        default_explanation_prompt_template()
    } else {
        configured_explanation_prompt
    };
    let messages = fill_prompt_template(&prompt_template, &text, None, &target_language_name);
    if let Some(window) = app.clone().get_webview_window("result_explanation") {
        let _ = window.emit("result-clean", serde_json::json!({
            "type": "explanation"
        }));
    }
    let result = client
        .generate_text_stream(messages.as_str(), Some(1000), |content_chunk| {
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    update_result_window(content_chunk, "explanation".to_string(), app_clone).await
                {
                    log::error!("更新解释结果窗口失败: {}", e);
                }
            });
        })
        .await;
    match result {
        Ok(()) => {
            log::info!("解释完成");
        }
        Err(e) => {
            let error_msg = format!("解释失败: {}", e);
            update_result_window(error_msg, "explanation".to_string(), app).await
                .map_err(|e| AppError::new(ErrorCode::SystemError, e))?;
        }
    }
    Ok(())
}
