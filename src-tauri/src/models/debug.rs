use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugSessionInfo {
    pub enabled: bool,
    pub session_id: Option<String>,
    pub session_dir: Option<String>,
    pub started_at: Option<String>,
    pub log_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugLogTail {
    pub path: Option<String>,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugConversationMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugScenarioRecordInput {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub project_id: String,
    pub project_name: Option<String>,
    pub prompt: String,
    pub conversation: Vec<DebugConversationMessage>,
    pub assistant_text: Option<String>,
    pub cleaned_content: Option<String>,
    pub review: Option<Value>,
    pub decision: Option<Value>,
    pub error: Option<String>,
    pub captured_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugScenarioRecord {
    #[serde(flatten)]
    pub scenario: DebugScenarioRecordInput,
    pub path: Option<String>,
}
