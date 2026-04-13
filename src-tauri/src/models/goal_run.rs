use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GoalRunPhase {
    PromptReceived,
    Planning,
    Implementation,
    Merging,
    RuntimeConfiguration,
    RuntimeExecution,
    Verification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GoalRunStatus {
    Running,
    Blocked,
    Completed,
    Failed,
    /// Was running when the app closed; can be resumed
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GoalRunEventKind {
    PhaseStarted,
    PhaseCompleted,
    RetryScheduled,
    RetryResumed,
    Blocked,
    Failed,
    Stopped,
    Note,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRun {
    pub id: String,
    pub project_id: String,
    pub prompt: String,
    pub phase: GoalRunPhase,
    pub status: GoalRunStatus,
    pub blocker_reason: Option<String>,
    pub current_plan_id: Option<String>,
    pub runtime_status_summary: Option<String>,
    pub verification_summary: Option<String>,
    pub retry_count: i64,
    pub last_failure_summary: Option<String>,
    #[serde(default)]
    pub stop_requested: bool,
    pub current_piece_id: Option<String>,
    pub current_task_id: Option<String>,
    pub retry_backoff_until: Option<String>,
    pub last_failure_fingerprint: Option<String>,
    #[serde(default)]
    pub attention_required: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRunEvent {
    pub id: String,
    pub goal_run_id: String,
    pub phase: GoalRunPhase,
    pub kind: GoalRunEventKind,
    pub summary: String,
    pub payload_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRunUpdate {
    pub prompt: Option<String>,
    pub phase: Option<GoalRunPhase>,
    pub status: Option<GoalRunStatus>,
    pub blocker_reason: Option<Option<String>>,
    pub current_plan_id: Option<Option<String>>,
    pub runtime_status_summary: Option<Option<String>>,
    pub verification_summary: Option<Option<String>>,
    pub retry_count: Option<i64>,
    pub last_failure_summary: Option<Option<String>>,
    pub stop_requested: Option<bool>,
    pub current_piece_id: Option<Option<String>>,
    pub current_task_id: Option<Option<String>>,
    pub retry_backoff_until: Option<Option<String>>,
    pub last_failure_fingerprint: Option<Option<String>>,
    pub attention_required: Option<bool>,
}
