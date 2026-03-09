pub mod claude;
pub mod openai_compat;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmResponse {
    pub content: String,
    pub tokens_used: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
}

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: Option<String>,
    pub max_tokens: u32,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, messages: &[Message], config: &LlmConfig) -> Result<LlmResponse, String>;
    async fn chat_stream(
        &self,
        messages: &[Message],
        config: &LlmConfig,
        sender: mpsc::Sender<String>,
    ) -> Result<TokenUsage, String>;
}

/// Create the appropriate provider based on provider name
pub fn create_provider(provider: &str) -> Box<dyn LlmProvider> {
    match provider.to_lowercase().as_str() {
        "claude" => Box::new(claude::ClaudeProvider),
        _ => Box::new(openai_compat::OpenAICompatProvider),
    }
}
