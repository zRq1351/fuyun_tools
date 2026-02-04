use crate::core::app_state::AppState as SharedAppState;
use crate::services::ai_client::{AIClient, AIConfig};
use crate::ui::window_manager::{hide_selection_toolbar_impl, show_result_window, update_result_window};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

/// 获取或创建AI客户端
pub async fn get_or_create_ai_client(state: Arc<Mutex<SharedAppState>>) -> Result<AIClient, String> {
    let current_config = {
        let state_guard = state.lock().unwrap();
        let api_key = state_guard
            .settings
            .decrypt_provider_api_key(&state_guard.settings.ai_provider)
            .unwrap_or_default();
        let provider_config = state_guard.settings.get_current_provider_config().unwrap();
        AIConfig {
            api_key,
            base_url: provider_config.api_url.clone(),
            model: provider_config.model_name.clone(),
        }
    };
    let client = AIClient::new(current_config).map_err(|e| format!("客户端初始化失败: {}", e))?;
    Ok(client)
}

#[tauri::command]
pub async fn stream_translate_text(
    text: String,
    source_language: String,
    target_language: String,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let client: AIClient = get_or_create_ai_client(state.inner().clone()).await?;

    show_result_window(
        "翻译结果".to_string(),
        "正在翻译...".to_string(),
        "translation".to_string(),
        text.clone(),
        app.clone(),
    )
        .await?;
    hide_selection_toolbar_impl(app.clone());
    // 直接使用传入的中文语言名称
    let source_language_name = source_language;
    let target_language_name = target_language;

    let messages = format!(
        "直接翻译下面的文字由{}翻译为:{}，不要有任何额外的内容输出文字输出。需要翻译内容为：\n\n{}",
        source_language_name, target_language_name, text
    );
    if let Some(window) = app.clone().get_webview_window("result_translation") {
        let _ = window.emit("result-clean", "");
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
            update_result_window(error_msg.clone(), "translation".to_string(), app).await?;
            log::error!("翻译失败: {}", error_msg);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn stream_explain_text(
    text: String,
    target_language: String,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let client: AIClient = get_or_create_ai_client(state.inner().clone()).await?;
    show_result_window(
        "解释结果".to_string(),
        "正在解释...".to_string(),
        "explanation".to_string(),
        text.clone(),
        app.clone(),
    )
        .await?;
    hide_selection_toolbar_impl(app.clone());
    let target_language_name = target_language;

    let messages = format!(
        "请用{}200字内解释这段话：\n\n{}",
        target_language_name, text
    );
    if let Some(window) = app.clone().get_webview_window("result_explanation") {
        let _ = window.emit("result-clean", "");
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
            update_result_window(error_msg, "explanation".to_string(), app).await?;
        }
    }
    Ok(())
}