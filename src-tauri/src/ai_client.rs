use reqwest;
use serde::{Deserialize, Serialize};
use std::time::Duration;

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

pub struct AIClient {
    client: reqwest::Client,
    config: AIConfig,
}

impl AIClient {
    pub fn new(config: AIConfig) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| format!("HTTP客户端创建失败: {}", e))?;

        Ok(AIClient { client, config })
    }

    /// 发送聊天完成请求
    pub async fn chat_completion(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse, String> {
        let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("api-key", self.config.api_key.parse().unwrap()); // 使用api-key而不是Authorization
        headers.insert("Content-Type", "application/json".parse().unwrap());

        let response = self.client
            .post(&url)
            .headers(headers)
            .json(request)
            .send()
            .await
            .map_err(|e| format!("请求发送失败: {}", e))?;

        let response: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| format!("响应解析失败: {}", e))?;
        Ok(response)
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
        let mut stream_request = request.clone();
        stream_request.stream = Some(true); // 启用流式响应
        // 优先使用max_completion_tokens，如果未设置则使用max_tokens
        if stream_request.max_completion_tokens.is_none() {
            stream_request.max_completion_tokens = stream_request.max_tokens;
        }

        let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("api-key", self.config.api_key.parse().unwrap());
        headers.insert("Content-Type", "application/json".parse().unwrap());

        let response = self.client
            .post(&url)
            .headers(headers)
            .json(&stream_request)
            .send()
            .await
            .map_err(|e| format!("请求发送失败: {}", e))?;

        let mut stream = response.bytes_stream();

        use futures_util::StreamExt;
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| format!("读取数据块失败: {}", e))?;
            let text = String::from_utf8_lossy(&chunk);

            // 处理服务器发送的事件流数据
            for line in text.lines() {
                if line.starts_with("data: ") {
                    let data = &line[6..]; // 移除 "data: " 前缀
                    if data == "[DONE]" {
                        break;
                    }

                    // 尝试解析JSON数据
                    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(data) {
                        // 检查是否有错误
                        if json_value.get("error").is_some() {
                            if let Ok(error_msg) = serde_json::to_string(&json_value) {
                                return Err(format!("API错误: {}", error_msg));
                            }
                        }
                        
                        // 提取内容部分
                        if let Some(choices) = json_value.get("choices").and_then(|c| c.as_array()) {
                            for choice in choices {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                        if !content.is_empty() {
                                            callback(content.to_string());
                                        }
                                    }
                                } else if let Some(finish_reason) = choice.get("finish_reason").and_then(|fr| fr.as_str()) {
                                    // 处理结束原因
                                    if finish_reason == "stop" {
                                        // 正常结束
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// 简单的文本生成
    pub async fn generate_text(&self, prompt: &str, max_tokens: Option<u32>) -> Result<String, String> {
        let messages = vec![Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        }];

        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages,
            temperature: Some(0.7),
            max_tokens,
            max_completion_tokens: max_tokens, // 使用max_tokens作为max_completion_tokens的值
            top_p: Some(1.0),
            frequency_penalty: Some(0.0),
            presence_penalty: Some(0.0),
            stream: Some(false), // 非流式响应
        };

        let response = self.chat_completion(&request).await?;
        
        if let Some(choice) = response.choices.first() {
            Ok(choice.message.content.clone())
        } else {
            Err("API返回空结果".to_string())
        }
    }

    /// 流式文本生成
    pub async fn generate_text_stream<F>(&self, prompt: &str, max_tokens: Option<u32>, callback: F) -> Result<(), String>
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
            max_completion_tokens: max_tokens, // 使用max_tokens作为max_completion_tokens的值
            top_p: Some(1.0),
            frequency_penalty: Some(0.0),
            presence_penalty: Some(0.0),
            stream: Some(true), // 启用流式响应
        };

        self.chat_completion_stream(&request, callback).await
    }

    /// 测试连接
    pub async fn test_connection(&self) -> Result<bool, String> {
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