use crate::core::error::{AppError, AppResult, ErrorCode};
use async_openai::{
    types::chat::{
        ChatCompletionRequestAssistantMessageArgs,
        ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs,
    },
    Client,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: Option<String>,
    pub choices: Vec<Choice>,
    pub created: Option<u64>,
    pub model: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub index: Option<u64>,
    pub message: Message,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct AIConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct AIClient {
    pub client: Client<async_openai::config::OpenAIConfig>,
    pub config: AIConfig,
}

impl AIClient {
    /// 创建AI客户端
    pub fn new(config: AIConfig) -> AppResult<Self> {
        let openai_config = async_openai::config::OpenAIConfig::new()
            .with_api_key(&config.api_key)
            .with_api_base(&config.base_url);

        let client = Client::with_config(openai_config);

        Ok(AIClient { client, config })
    }

    /// 将内部消息格式转换为OpenAI消息格式
    fn convert_messages(&self, messages: &[Message]) -> AppResult<Vec<ChatCompletionRequestMessage>> {
        messages
            .iter()
            .map(|msg| {
                match msg.role.to_lowercase().as_str() {
                    "assistant" => ChatCompletionRequestAssistantMessageArgs::default()
                        .content(msg.content.clone())
                        .build()
                        .map(ChatCompletionRequestMessage::Assistant)
                        .map_err(|e| AppError::new(ErrorCode::ValidationError, "构建assistant消息失败").with_details(e.to_string())),
                    "system" => ChatCompletionRequestSystemMessageArgs::default()
                        .content(msg.content.clone())
                        .build()
                        .map(ChatCompletionRequestMessage::System)
                        .map_err(|e| AppError::new(ErrorCode::ValidationError, "构建system消息失败").with_details(e.to_string())),
                    _ => ChatCompletionRequestUserMessageArgs::default()
                        .content(msg.content.clone())
                        .build()
                        .map(ChatCompletionRequestMessage::User)
                        .map_err(|e| AppError::new(ErrorCode::ValidationError, "构建user消息失败").with_details(e.to_string())),
                }
            })
            .collect()
    }

    /// 构建OpenAI聊天完成请求
    fn build_chat_request(
        &self,
        request: &ChatCompletionRequest,
        stream: bool,
    ) -> AppResult<async_openai::types::chat::CreateChatCompletionRequest> {
        let messages = self.convert_messages(&request.messages)?;

        let mut binding = CreateChatCompletionRequestArgs::default();
        let mut builder = binding
            .model(&request.model)
            .messages(messages)
            .temperature(request.temperature.unwrap_or(0.7))
            .max_tokens(request.max_tokens.unwrap_or(1000))
            .top_p(request.top_p.unwrap_or(1.0))
            .frequency_penalty(request.frequency_penalty.unwrap_or(0.0))
            .presence_penalty(request.presence_penalty.unwrap_or(0.0));

        if stream {
            builder = builder.stream(true);
        }

        builder
            .build()
            .map_err(|e| AppError::new(ErrorCode::ValidationError, "构建请求失败").with_details(e.to_string()))
    }

    /// 发送聊天完成请求
    pub async fn chat_completion(
        &self,
        request: &ChatCompletionRequest,
    ) -> AppResult<ChatCompletionResponse> {
        let openai_request = self.build_chat_request(request, false)?;

        let response = self
            .client
            .chat()
            .create(openai_request)
            .await
            .map_err(|e| AppError::new(ErrorCode::NetworkError, "请求发送失败").with_details(e.to_string()))?;

        let chat_response = ChatCompletionResponse {
            id: Some(response.id.clone()),
            choices: response
                .choices
                .into_iter()
                .map(|choice| Choice {
                    index: Some(choice.index as u64),
                    message: Message {
                        role: "assistant".to_string(),
                        content: choice.message.content.unwrap_or_default(),
                    },
                    finish_reason: choice.finish_reason.map(|fr| format!("{:?}", fr)),
                })
                .collect(),
            created: Some(response.created as u64),
            model: Some(response.model),
            usage: response.usage.map(|usage| Usage {
                prompt_tokens: Some(usage.prompt_tokens),
                completion_tokens: Some(usage.completion_tokens),
                total_tokens: Some(usage.total_tokens),
            }),
        };

        Ok(chat_response)
    }

    /// 流式发送聊天完成请求
    pub async fn chat_completion_stream<F>(
        &self,
        request: &ChatCompletionRequest,
        mut callback: F,
    ) -> AppResult<()>
    where
        F: FnMut(String) -> bool,
    {
        let openai_request = self.build_chat_request(request, true)?;

        let mut stream = self
            .client
            .chat()
            .create_stream(openai_request)
            .await
            .map_err(|e| AppError::new(ErrorCode::NetworkError, "请求发送失败").with_details(e.to_string()))?;

        use futures_util::StreamExt;
        while let Some(result) = stream.next().await {
            match result {
                Ok(response) => {
                    for choice in response.choices {
                        if let Some(content) = choice.delta.content {
                            if !content.is_empty() && !callback(content) {
                                return Ok(());
                            }
                        }
                        if let Some(finish_reason) = choice.finish_reason {
                            if format!("{:?}", finish_reason) == "Stop" {
                                return Ok(());
                            }
                        }
                    }
                }
                Err(e) => {
                    return Err(AppError::new(ErrorCode::NetworkError, "流式响应错误").with_details(e.to_string()));
                }
            }
        }

        Ok(())
    }

    /// 简单的文本生成
    pub async fn generate_text(
        &self,
        prompt: &str,
        max_tokens: Option<u32>,
    ) -> AppResult<String> {
        let messages = vec![Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        }];

        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages,
            temperature: Some(0.7),
            max_tokens,
            max_completion_tokens: max_tokens,
            top_p: Some(1.0),
            frequency_penalty: Some(0.0),
            presence_penalty: Some(0.0),
            stream: Some(false),
        };

        let response = self.chat_completion(&request).await?;

        if let Some(choice) = response.choices.first() {
            Ok(choice.message.content.clone())
        } else {
            Err(AppError::new(ErrorCode::NetworkError, "API返回空结果"))
        }
    }

    /// 流式文本生成
    pub async fn generate_text_stream<F>(
        &self,
        prompt: &str,
        max_tokens: Option<u32>,
        callback: F,
    ) -> AppResult<()>
    where
        F: FnMut(String) -> bool,
    {
        let messages = vec![Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        }];

        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages,
            temperature: Some(0.7),
            max_tokens,
            max_completion_tokens: max_tokens,
            top_p: Some(1.0),
            frequency_penalty: Some(0.0),
            presence_penalty: Some(0.0),
            stream: Some(true),
        };
        self.chat_completion_stream(&request, callback).await
    }

    /// 测试连接
    pub async fn test_connection(&self) -> AppResult<bool> {
        let messages = vec![Message {
            role: "user".to_string(),
            content: "Hi".to_string(),
        }];

        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages,
            temperature: Some(0.0),
            max_tokens: Some(1),
            max_completion_tokens: Some(1),
            top_p: Some(1.0),
            frequency_penalty: Some(0.0),
            presence_penalty: Some(0.0),
            stream: Some(false),
        };

        match self.chat_completion(&request).await {
            Ok(response) => {
                if !response.choices.is_empty() {
                    Ok(true)
                } else {
                    log::warn!("AI连接测试返回空选项，但网络连接正常");
                    Ok(true)
                }
            },
            Err(e) => {
                log::error!("AI连接测试失败: {}", e);
                let raw = e
                    .details
                    .clone()
                    .unwrap_or_else(|| e.message.clone())
                    .to_lowercase();
                if raw.contains("401") {
                    Err(AppError::new(ErrorCode::ValidationError, "鉴权失败：API Key 无效"))
                } else if raw.contains("404") {
                    Err(AppError::new(ErrorCode::ValidationError, "请求失败：模型不存在或 API 地址错误"))
                } else if raw.contains("timeout") || raw.contains("timed out") {
                    Err(AppError::new(ErrorCode::NetworkError, "连接超时：请检查网络设置"))
                } else {
                    Err(e)
                }
            }
        }
    }
}
