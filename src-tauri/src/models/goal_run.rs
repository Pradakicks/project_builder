use serde::{Deserialize, Serialize};

use super::{Artifact, Piece, PlanTask, ProjectRuntimeStatus, WorkPlan};

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
    Retrying,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRunRetryState {
    pub retry_count: i64,
    pub stop_requested: bool,
    pub retry_backoff_until: Option<String>,
    pub last_failure_summary: Option<String>,
    pub last_failure_fingerprint: Option<String>,
    pub attention_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRunCodeEvidence {
    pub piece_id: String,
    pub piece_name: String,
    pub git_branch: Option<String>,
    pub git_commit_sha: Option<String>,
    pub git_diff_stat: Option<String>,
    pub generated_files_artifact: Option<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveActivity {
    pub piece_id: String,
    pub piece_name: String,
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub engine: Option<String>,
    pub current_index: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRunDeliverySnapshot {
    pub goal_run: GoalRun,
    pub current_plan: Option<WorkPlan>,
    pub blocking_piece: Option<Piece>,
    pub blocking_task: Option<PlanTask>,
    pub retry_state: GoalRunRetryState,
    pub code_evidence: Option<GoalRunCodeEvidence>,
    pub runtime_status: Option<ProjectRuntimeStatus>,
    pub recent_events: Vec<GoalRunEvent>,
    pub live_activity: Option<LiveActivity>,
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
