pub mod claude;
pub mod openai_compat;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[cfg(test)]
use std::sync::{Mutex, OnceLock};

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
    #[cfg(test)]
    if mock_responses().is_some() {
        return Box::new(MockProvider);
    }

    match provider.to_lowercase().as_str() {
        "claude" => Box::new(claude::ClaudeProvider),
        _ => Box::new(openai_compat::OpenAICompatProvider),
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct TestLlmResponses {
    pub leader_plan: String,
    pub integration_review: String,
    pub summary: String,
}

#[cfg(test)]
static TEST_LLM_RESPONSES: OnceLock<Mutex<Option<TestLlmResponses>>> = OnceLock::new();

#[cfg(test)]
pub fn set_test_llm_responses(responses: TestLlmResponses) {
    let slot = TEST_LLM_RESPONSES.get_or_init(|| Mutex::new(None));
    *slot.lock().expect("lock test llm responses") = Some(responses);
}

#[cfg(test)]
fn mock_responses() -> Option<TestLlmResponses> {
    TEST_LLM_RESPONSES
        .get()
        .and_then(|slot| slot.lock().ok().and_then(|responses| responses.clone()))
}

#[cfg(test)]
struct MockProvider;

#[cfg(test)]
#[async_trait]
impl LlmProvider for MockProvider {
    async fn chat(&self, messages: &[Message], _config: &LlmConfig) -> Result<LlmResponse, String> {
        let responses = mock_responses().unwrap_or_default();
        let content = choose_mock_response(messages, &responses);
        Ok(LlmResponse {
            content,
            tokens_used: TokenUsage::default(),
        })
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        _config: &LlmConfig,
        sender: mpsc::Sender<String>,
    ) -> Result<TokenUsage, String> {
        let responses = mock_responses().unwrap_or_default();
        let content = choose_mock_response(messages, &responses);
        if !content.is_empty() {
            let _ = sender.send(content).await;
        }
        Ok(TokenUsage { input: 1, output: 1 })
    }
}

#[cfg(test)]
fn choose_mock_response(messages: &[Message], responses: &TestLlmResponses) -> String {
    let combined = messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    if combined.contains("Leader Agent") {
        responses.leader_plan.clone()
    } else if combined.contains("integration reviewer") {
        responses.integration_review.clone()
    } else if combined.contains("technical summarizer") {
        responses.summary.clone()
    } else {
        String::new()
    }
}
