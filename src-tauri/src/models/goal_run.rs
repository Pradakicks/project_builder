use serde::{Deserialize, Serialize};

use super::{AgentRole, Artifact, Piece, PlanTask, ProjectRuntimeStatus, WorkPlan};

/// Discriminates what kind of check produced a `VerificationCheck`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckKind {
    Shell,
    Http,
    TcpPort,
    LogScan,
    Skipped,
}

/// One concrete check run during the Verification phase.
///
/// `expected` / `actual` are the structured human-readable contract for a
/// check — e.g. `expected = "status in 200..=399"`, `actual = "status 500"`.
/// Older rows and simpler check kinds may leave both `None`; rendering code
/// falls back to the free-form `detail` string in that case.
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
    #[serde(default)]
    pub expected: Option<String>,
    #[serde(default)]
    pub actual: Option<String>,
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

/// Input to the CTO repair prompt assembler. Carries what the repair agent
/// needs to diagnose a failure: the one-line summary plus, when available,
/// the structured per-check breakdown from Verification.
///
/// Non-Verification phases (Implementation, RuntimeExecution) don't yet emit
/// structured failure data, so they construct this via `from_summary` and
/// the downstream prompt renderer degrades gracefully to the minimal shape.
#[derive(Debug, Clone)]
pub struct PhaseFailureContext {
    pub summary: String,
    pub failed_checks: Vec<VerificationCheck>,
    pub passed_checks: Vec<VerificationCheck>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub failing_role: Option<AgentRole>,
}

impl PhaseFailureContext {
    pub fn from_summary(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            failed_checks: vec![],
            passed_checks: vec![],
            started_at: None,
            finished_at: None,
            failing_role: None,
        }
    }

    pub fn from_verification(result: &VerificationResult) -> Self {
        let (failed, passed): (Vec<_>, Vec<_>) = result
            .checks
            .iter()
            .cloned()
            .partition(|check| !check.passed);
        Self {
            summary: result.message.clone(),
            failed_checks: failed,
            passed_checks: passed,
            started_at: Some(result.started_at.clone()),
            finished_at: Some(result.finished_at.clone()),
            failing_role: None,
        }
    }

    pub fn with_failing_role(mut self, role: AgentRole) -> Self {
        self.failing_role = Some(role);
        self
    }
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
    fn goal_run_phase_ordinal_is_strictly_increasing() {
        let phases = [
            GoalRunPhase::PromptReceived,
            GoalRunPhase::Planning,
            GoalRunPhase::Implementation,
            GoalRunPhase::Merging,
            GoalRunPhase::RuntimeConfiguration,
            GoalRunPhase::RuntimeExecution,
            GoalRunPhase::Verification,
        ];
        for window in phases.windows(2) {
            assert!(
                window[0].ordinal() < window[1].ordinal(),
                "{:?} ordinal should be less than {:?} ordinal",
                window[0],
                window[1]
            );
        }
    }

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
                expected: None,
                actual: None,
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
    fn parse_verification_result_accepts_rows_without_expected_actual_fields() {
        // Legacy rows produced before `expected`/`actual` were added must still
        // deserialize cleanly via the `#[serde(default)]` fallback.
        let raw = r#"{
            "passed": true,
            "checks": [{
                "name": "verify command",
                "kind": "shell",
                "passed": true,
                "detail": "exited 0",
                "durationMs": 42
            }],
            "startedAt": "2024-01-01T00:00:00Z",
            "finishedAt": "2024-01-01T00:00:01Z",
            "message": "1/1 checks passed"
        }"#;
        let parsed = parse_verification_result(raw);
        assert_eq!(parsed.checks.len(), 1);
        assert!(parsed.checks[0].expected.is_none());
        assert!(parsed.checks[0].actual.is_none());
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
    fn phase_failure_context_from_summary_is_empty_except_for_summary() {
        let ctx = PhaseFailureContext::from_summary("boom");
        assert_eq!(ctx.summary, "boom");
        assert!(ctx.failed_checks.is_empty());
        assert!(ctx.passed_checks.is_empty());
        assert!(ctx.started_at.is_none());
        assert!(ctx.finished_at.is_none());
        assert!(ctx.failing_role.is_none());
    }

    #[test]
    fn phase_failure_context_records_failing_role() {
        let ctx = PhaseFailureContext::from_summary("x").with_failing_role(AgentRole::Testing);
        assert_eq!(ctx.failing_role, Some(AgentRole::Testing));
    }

    #[test]
    fn phase_failure_context_from_verification_partitions_checks() {
        let result = VerificationResult {
            passed: false,
            checks: vec![
                VerificationCheck {
                    name: "verify command".to_string(),
                    kind: CheckKind::Shell,
                    passed: true,
                    detail: "exited 0".to_string(),
                    duration_ms: 10,
                    expected: None,
                    actual: None,
                },
                VerificationCheck {
                    name: "http readiness".to_string(),
                    kind: CheckKind::Http,
                    passed: false,
                    detail: "connection refused".to_string(),
                    duration_ms: 60_000,
                    expected: Some("HTTP 200".to_string()),
                    actual: Some("connection refused".to_string()),
                },
            ],
            started_at: "2024-01-01T00:00:00Z".to_string(),
            finished_at: "2024-01-01T00:01:00Z".to_string(),
            message: "HTTP readiness check failed".to_string(),
        };

        let ctx = PhaseFailureContext::from_verification(&result);
        assert_eq!(ctx.summary, "HTTP readiness check failed");
        assert_eq!(ctx.failed_checks.len(), 1);
        assert_eq!(ctx.failed_checks[0].name, "http readiness");
        assert_eq!(ctx.passed_checks.len(), 1);
        assert_eq!(ctx.passed_checks[0].name, "verify command");
        assert_eq!(ctx.started_at.as_deref(), Some("2024-01-01T00:00:00Z"));
        assert_eq!(ctx.finished_at.as_deref(), Some("2024-01-01T00:01:00Z"));
        assert!(ctx.failing_role.is_none());
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
                expected: None,
                actual: None,
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

impl GoalRunPhase {
    pub fn ordinal(&self) -> u8 {
        match self {
            GoalRunPhase::PromptReceived => 0,
            GoalRunPhase::Planning => 1,
            GoalRunPhase::Implementation => 2,
            GoalRunPhase::Merging => 3,
            GoalRunPhase::RuntimeConfiguration => 4,
            GoalRunPhase::RuntimeExecution => 5,
            GoalRunPhase::Verification => 6,
        }
    }
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
    /// Explicitly paused by the operator; resume-able, not a failure
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GoalRunEventKind {
    PhaseStarted,
    PhaseCompleted,
    RetryScheduled,
    RetryResumed,
    RepairRequested,
    RepairStarted,
    RepairSkipped,
    RepairExecuted,
    RepairFailed,
    Blocked,
    Failed,
    Stopped,
    Note,
    Paused,
    Resumed,
    CancelledMidPhase,
    HeartbeatStale,
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
    #[serde(default)]
    pub last_heartbeat_at: Option<String>,
    #[serde(default)]
    pub operator_repair_requested: bool,
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
    #[serde(default)]
    pub operator_repair_requested: bool,
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
    pub last_heartbeat_at: Option<Option<String>>,
    pub operator_repair_requested: Option<bool>,
}
