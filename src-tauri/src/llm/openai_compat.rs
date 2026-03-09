use super::{LlmConfig, LlmProvider, LlmResponse, Message, TokenUsage};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub struct OpenAICompatProvider;

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

#[async_trait]
impl LlmProvider for OpenAICompatProvider {
    async fn chat(&self, messages: &[Message], config: &LlmConfig) -> Result<LlmResponse, String> {
        let client = Client::new();
        let base_url = config
            .base_url
            .as_deref()
            .unwrap_or(DEFAULT_BASE_URL)
            .trim_end_matches('/');

        let body = json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "messages": messages.iter().map(|m| json!({
                "role": m.role,
                "content": m.content,
            })).collect::<Vec<_>>(),
        });

        let resp = client
            .post(format!("{base_url}/chat/completions"))
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("OpenAI API error ({status}): {text}"));
        }

        let data: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let content = data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let input_tokens = data["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
        let output_tokens = data["usage"]["completion_tokens"].as_u64().unwrap_or(0);

        Ok(LlmResponse {
            content,
            tokens_used: TokenUsage {
                input: input_tokens,
                output: output_tokens,
            },
        })
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        config: &LlmConfig,
        sender: mpsc::Sender<String>,
    ) -> Result<TokenUsage, String> {
        let client = Client::new();
        let base_url = config
            .base_url
            .as_deref()
            .unwrap_or(DEFAULT_BASE_URL)
            .trim_end_matches('/');

        let body = json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "stream": true,
            "stream_options": { "include_usage": true },
            "messages": messages.iter().map(|m| json!({
                "role": m.role,
                "content": m.content,
            })).collect::<Vec<_>>(),
        });

        let resp = client
            .post(format!("{base_url}/chat/completions"))
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.map_err(|e| e.to_string())?;
            return Err(format!("OpenAI API error: {text}"));
        }

        let mut stream = resp.bytes_stream();
        let mut usage = TokenUsage::default();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| e.to_string())?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if !line.starts_with("data: ") {
                    continue;
                }
                let json_str = &line[6..];
                if json_str == "[DONE]" {
                    continue;
                }

                if let Ok(event) = serde_json::from_str::<Value>(json_str) {
                    // Stream content delta
                    if let Some(text) = event["choices"][0]["delta"]["content"].as_str() {
                        let _ = sender.send(text.to_string()).await;
                    }
                    // Usage info (sent at end if stream_options.include_usage is true)
                    if let Some(u) = event.get("usage") {
                        usage.input = u["prompt_tokens"].as_u64().unwrap_or(0);
                        usage.output = u["completion_tokens"].as_u64().unwrap_or(0);
                    }
                }
            }
        }

        Ok(usage)
    }
}
