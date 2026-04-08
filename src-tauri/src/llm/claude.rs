use super::{LlmConfig, LlmProvider, LlmResponse, Message, TokenUsage};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::{debug, trace, error};

pub struct ClaudeProvider;

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";

#[async_trait]
impl LlmProvider for ClaudeProvider {
    async fn chat(&self, messages: &[Message], config: &LlmConfig) -> Result<LlmResponse, String> {
        let client = Client::new();
        let (system, user_msgs) = split_system(messages);
            debug!(model = %config.model, msg_count = messages.len(), max_tokens = config.max_tokens, "Claude API chat call");
            trace!(system_prompt = ?system, user_messages = ?user_msgs.len(), "Claude chat request messages");

        let mut body = json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "messages": user_msgs.iter().map(|m| json!({
                "role": m.role,
                "content": m.content,
            })).collect::<Vec<_>>(),
        });
        if let Some(sys) = &system {
            body["system"] = json!(sys);
        }

        let resp = client
            .post(CLAUDE_API_URL)
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            error!(status = %status, body = %text, "Claude API error");
            return Err(format!("Claude API error ({status}): {text}"));
        }

        let data: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let content = data["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let input_tokens = data["usage"]["input_tokens"].as_u64().unwrap_or(0);
        let output_tokens = data["usage"]["output_tokens"].as_u64().unwrap_or(0);

            debug!(input_tokens, output_tokens, "Claude chat complete");
            trace!(response_content = %content, "Claude chat response");

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
        let (system, user_msgs) = split_system(messages);
            debug!(model = %config.model, msg_count = messages.len(), max_tokens = config.max_tokens, "Claude API streaming call");
            trace!(system_prompt = ?system, "Claude stream request");

        let mut body = json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "stream": true,
            "messages": user_msgs.iter().map(|m| json!({
                "role": m.role,
                "content": m.content,
            })).collect::<Vec<_>>(),
        });
        if let Some(sys) = &system {
            body["system"] = json!(sys);
        }

        let resp = client
            .post(CLAUDE_API_URL)
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.map_err(|e| e.to_string())?;
            error!(body = %text, "Claude streaming API error");
            return Err(format!("Claude API error: {text}"));
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
                    let event_type = event["type"].as_str().unwrap_or("");
                    match event_type {
                        "content_block_delta" => {
                            if let Some(text) = event["delta"]["text"].as_str() {
                                let _ = sender.send(text.to_string()).await;
                            }
                        }
                        "message_delta" => {
                            if let Some(out) = event["usage"]["output_tokens"].as_u64() {
                                usage.output = out;
                            }
                        }
                        "message_start" => {
                            if let Some(inp) = event["message"]["usage"]["input_tokens"].as_u64() {
                                usage.input = inp;
                            }
                        }
                        other => {
                            trace!(event_type = ?other, "Claude SSE event (ignored)");
                        }
                    }
                }
            }
        }

        debug!(input_tokens = usage.input, output_tokens = usage.output, "Claude stream complete");

        Ok(usage)
    }
}

/// Split system message from user/assistant messages
fn split_system(messages: &[Message]) -> (Option<String>, Vec<&Message>) {
    let mut system = None;
    let mut others = Vec::new();
    for msg in messages {
        if msg.role == "system" {
            system = Some(msg.content.clone());
        } else {
            others.push(msg);
        }
    }
    (system, others)
}
