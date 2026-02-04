use async_openai::{
    types::{
        ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs,
        CreateChatCompletionRequestArgs
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
    pub fn new(config: AIConfig) -> Result<Self, String> {
        let openai_config = async_openai::config::OpenAIConfig::new()
            .with_api_key(&config.api_key)
            .with_api_base(&config.base_url);
        
        let client = Client::with_config(openai_config);

        Ok(AIClient { client, config })
    }

    /// 将内部消息格式转换为OpenAI消息格式
    fn convert_messages(&self, messages: &[Message]) -> Vec<ChatCompletionRequestMessage> {
        messages
            .iter()
            .map(|msg| {
                ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(msg.content.clone())
                        .build()
                        .unwrap()
                )
            })
            .collect()
    }

    /// 构建OpenAI聊天完成请求
    fn build_chat_request(
        &self,
        request: &ChatCompletionRequest,
        stream: bool,
    ) -> Result<async_openai::types::CreateChatCompletionRequest, String> {
        let messages = self.convert_messages(&request.messages);
        
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
        
        builder.build().map_err(|e| format!("构建请求失败: {}", e))
    }

    /// 发送聊天完成请求
    pub async fn chat_completion(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, String> {
        let openai_request = self.build_chat_request(request, false)?;

        let response = self
            .client
            .chat()
            .create(openai_request)
            .await
            .map_err(|e| format!("请求发送失败: {}", e))?;

        // 转换为我们的响应格式
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
    ) -> Result<(), String>
    where
        F: FnMut(String) -> (),
    {
        let messages = self.convert_messages(&request.messages);

        let openai_request = CreateChatCompletionRequestArgs::default()
            .model(&request.model)
            .messages(messages)
            .temperature(request.temperature.unwrap_or(0.7))
            .max_tokens(request.max_tokens.unwrap_or(1000))
            .top_p(request.top_p.unwrap_or(1.0))
            .frequency_penalty(request.frequency_penalty.unwrap_or(0.0))
            .presence_penalty(request.presence_penalty.unwrap_or(0.0))
            .stream(true)
            .build()
            .map_err(|e| format!("构建请求失败: {}", e))?;

        let mut stream = self
            .client
            .chat()
            .create_stream(openai_request)
            .await
            .map_err(|e| format!("请求发送失败: {}", e))?;

        use futures_util::StreamExt;
        while let Some(result) = stream.next().await {
            match result {
                Ok(response) => {
                    for choice in response.choices {
                        if let Some(content) = choice.delta.content {
                            if !content.is_empty() {
                                callback(content);
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
                    return Err(format!("流式响应错误: {}", e));
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
    ) -> Result<String, String> {
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
            Err("API返回空结果".to_string())
        }
    }

    /// 流式文本生成
    pub async fn generate_text_stream<F>(
        &self,
        prompt: &str,
        max_tokens: Option<u32>,
        callback: F,
    ) -> Result<(), String>
    where
        F: FnMut(String) -> (),
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
    pub async fn test_connection(&self) -> Result<bool, String> {
        let test_prompt = "请输出：连接成功";
        match self.generate_text(test_prompt, Some(50)).await {
            Ok(result) => Ok(result.contains("成功")
                || result.contains("Success")
                || result.to_lowercase().contains("connected")),
            Err(e) => {
                log::error!("AI连接测试失败: {}", e);
                Err(e)
            }
        }
    }
}