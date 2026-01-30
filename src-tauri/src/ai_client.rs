use reqwest;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct AIConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

pub struct AIClient {
    client: reqwest::Client,
    config: AIConfig,
}

impl AIClient {
    pub fn new(config: AIConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()?;

        Ok(AIClient { client, config })
    }

    /// 发送聊天完成请求
    pub async fn chat_completion(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse, reqwest::Error> {
        let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Authorization", format!("Bearer {}", self.config.api_key).parse().unwrap());
        headers.insert("Content-Type", "application/json".parse().unwrap());

        let response = self.client
            .post(&url)
            .headers(headers)
            .json(request)
            .send()
            .await?;

        let response: ChatCompletionResponse = response.json().await?;
        Ok(response)
    }

    /// 简单的文本生成
    pub async fn generate_text(&self, prompt: &str, max_tokens: Option<u32>) -> Result<String, Box<dyn std::error::Error>> {
        let messages = vec![Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        }];

        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages,
            temperature: Some(0.7),
            max_tokens,
            top_p: Some(1.0),
            frequency_penalty: Some(0.0),
            presence_penalty: Some(0.0),
        };

        let response = self.chat_completion(&request).await?;
        
        if let Some(choice) = response.choices.first() {
            Ok(choice.message.content.clone())
        } else {
            Err("No choices in response".into())
        }
    }

    /// 测试连接
    pub async fn test_connection(&self) -> Result<bool, Box<dyn std::error::Error>> {
        let test_prompt = "请输出：连接成功";
        match self.generate_text(test_prompt, Some(50)).await {
            Ok(result) => Ok(result.contains("成功") || result.contains("Success") || result.to_lowercase().contains("connected")),
            Err(e) => {
                log::error!("AI连接测试失败: {}", e);
                Err(e)
            }
        }
    }
}

// 自定义错误类型
#[derive(Debug)]
pub struct AIClientError {
    pub message: String,
}

impl std::fmt::Display for AIClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AIClientError {}

// 为 String 实现 From trait
impl From<String> for AIClientError {
    fn from(msg: String) -> Self {
        AIClientError { message: msg }
    }
}

impl From<&str> for AIClientError {
    fn from(msg: &str) -> Self {
        AIClientError { message: msg.to_string() }
    }
}