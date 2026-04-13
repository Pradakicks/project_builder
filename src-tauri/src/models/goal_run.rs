use serde::{Deserialize, Serialize};

use super::{Artifact, Piece, PlanTask, ProjectRuntimeStatus, WorkPlan};

/// Discriminates what kind of check produced a `VerificationCheck`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckKind {
    Shell,
    Http,
    TcpPort,
    Skipped,
}

/// One concrete check run during the Verification phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationCheck {
    /// Human-readable label, e.g. "verify command", "http readiness".
    pub name: String,
    pub kind: CheckKind,
    pub passed: bool,
    /// Concise detail: exit code text, HTTP status, error message, etc.
    pub detail: String,
    pub duration_ms: i64,
}

/// Structured result of the Verification phase, stored as JSON in the
/// `verification_summary` TEXT column (no schema migration required).
/// Legacy rows that stored a plain string are handled by `parse_verification_result`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationResult {
    pub passed: bool,
    pub checks: Vec<VerificationCheck>,
    pub started_at: String,  // RFC-3339
    pub finished_at: String, // RFC-3339
    /// One-line human summary suitable for `blocker_reason` / banners.
    pub message: String,
}

/// Tolerant deserializer: accepts a JSON `VerificationResult` or falls back to
/// treating the raw string as a legacy plain-text summary (passed=true, no checks).
pub fn parse_verification_result(raw: &str) -> VerificationResult {
    if let Ok(result) = serde_json::from_str::<VerificationResult>(raw) {
        return result;
    }
    // Legacy plain string
    let now = chrono::Utc::now().to_rfc3339();
    VerificationResult {
        passed: true,
        checks: vec![],
        started_at: now.clone(),
        finished_at: now,
        message: raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_verification_result_handles_structured_json() {
        let result = VerificationResult {
            passed: true,
            checks: vec![VerificationCheck {
                name: "verify command".to_string(),
                kind: CheckKind::Shell,
                passed: true,
                detail: "exited 0".to_string(),
                duration_ms: 100,
            }],
            started_at: "2024-01-01T00:00:00Z".to_string(),
            finished_at: "2024-01-01T00:00:01Z".to_string(),
            message: "1/1 checks passed".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed = parse_verification_result(&json);
        assert!(parsed.passed);
        assert_eq!(parsed.checks.len(), 1);
        assert_eq!(parsed.message, "1/1 checks passed");
    }

    #[test]
    fn parse_verification_result_handles_legacy_plain_string() {
        let raw = "Verification passed via `npm test`";
        let parsed = parse_verification_result(raw);
        assert!(parsed.passed, "legacy strings default to passed=true");
        assert!(parsed.checks.is_empty(), "legacy strings produce no checks");
        assert_eq!(parsed.message, raw);
    }

    #[test]
    fn parse_verification_result_handles_failed_json() {
        let result = VerificationResult {
            passed: false,
            checks: vec![VerificationCheck {
                name: "http readiness".to_string(),
                kind: CheckKind::Http,
                passed: false,
                detail: "connection refused".to_string(),
                duration_ms: 60_000,
            }],
            started_at: "2024-01-01T00:00:00Z".to_string(),
            finished_at: "2024-01-01T00:01:00Z".to_string(),
            message: "HTTP readiness check failed: connection refused".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed = parse_verification_result(&json);
        assert!(!parsed.passed);
        assert_eq!(parsed.checks[0].kind, CheckKind::Http);
    }
}

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
    /// Parsed structured verification result, if a Verification phase has run.
    pub verification_result: Option<VerificationResult>,
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
