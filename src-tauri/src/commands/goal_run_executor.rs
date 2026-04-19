use crate::agent::{self, runner};
use crate::commands::repair_prompt::build_repair_prompt;
use crate::commands::{cto_action_engine, runtime_commands};
use crate::db::{Database, PieceUpdate};
use crate::llm::{self, LlmConfig, Message};
use crate::models::{
    CtoDecisionExecution, CtoDecisionRecordInput, CtoDecisionReview, CtoDecisionStatus,
    CtoRepairContext, GoalRun, GoalRunEventKind, GoalRunPhase, GoalRunStatus, GoalRunUpdate,
    OutputMode, Phase, PhaseFailureContext, VerificationResult, WorkPlan,
};
use crate::AppState;
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// Status values for `phase-progress` events. Kept in sync with the TS
/// `PhaseProgressEvent` union — see `src/api/goalRunApi.ts`.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PhaseProgressStatus {
    Started,
    Step,
    Completed,
    #[allow(dead_code)]
    Failed,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PhaseProgressEvent {
    pub goal_run_id: String,
    pub phase: String,
    pub status: PhaseProgressStatus,
    pub message: String,
    pub piece_id: Option<String>,
    pub piece_name: Option<String>,
    pub step_index: Option<u32>,
    pub step_total: Option<u32>,
}

/// Emit a `phase-progress` breadcrumb to the frontend. Best-effort — a dropped
/// event is never fatal, so all call sites just `let _ =`.
pub(crate) fn emit_phase_progress<R: tauri::Runtime>(
    app_handle: &AppHandle<R>,
    goal_run_id: &str,
    phase: GoalRunPhase,
    status: PhaseProgressStatus,
    message: impl Into<String>,
) {
    let phase_str = serde_json::to_string(&phase)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();
    let _ = app_handle.emit(
        "phase-progress",
        PhaseProgressEvent {
            goal_run_id: goal_run_id.to_string(),
            phase: phase_str,
            status,
            message: message.into(),
            piece_id: None,
            piece_name: None,
            step_index: None,
            step_total: None,
        },
    );
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RepairAttemptContext {
    goal_run_id: String,
    project_id: String,
    phase: GoalRunPhase,
    retry_count: i64,
    failure_summary: String,
    failed_check_count: usize,
    passed_check_count: usize,
    provider_name: String,
    model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
    prompt_preview: String,
    prompt_length: usize,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum RepairSkipReason {
    NoApiKey,
    NoActions,
    OnlyBlockedActions,
    NoExecutableActions,
    RetryBudgetExhausted,
    RepeatedFailure,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum RepairFailureStage {
    Chat,
    Parse,
    Execute,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum RepairOutcome {
    Skipped {
        reason: RepairSkipReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decision_id: Option<String>,
    },
    Executed {
        decision_id: String,
        executed_actions: i64,
        errors: Vec<String>,
    },
    Failed {
        stage: RepairFailureStage,
        error: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decision_id: Option<String>,
    },
}

fn repair_event_payload(
    context: &RepairAttemptContext,
    outcome: Option<&RepairOutcome>,
) -> serde_json::Value {
    match outcome {
        Some(RepairOutcome::Skipped {
            reason,
            decision_id,
        }) => json!({
            "context": context,
            "outcome": {
                "kind": "skipped",
                "reason": reason,
                "decisionId": decision_id,
            }
        }),
        Some(RepairOutcome::Executed {
            decision_id,
            executed_actions,
            errors,
        }) => json!({
            "context": context,
            "outcome": {
                "kind": "executed",
                "decisionId": decision_id,
                "executedActions": executed_actions,
                "errors": errors,
            }
        }),
        Some(RepairOutcome::Failed {
            stage,
            error,
            decision_id,
        }) => json!({
            "context": context,
            "outcome": {
                "kind": "failed",
                "stage": stage,
                "error": error,
                "decisionId": decision_id,
            }
        }),
        None => json!({
            "context": context,
        }),
    }
}

fn repair_decision_review(
    mut review: CtoDecisionReview,
    context: &RepairAttemptContext,
) -> CtoDecisionReview {
    review.repair_context = Some(CtoRepairContext {
        goal_run_id: context.goal_run_id.clone(),
        phase: context.phase.clone(),
        retry_count: context.retry_count,
        provider_name: context.provider_name.clone(),
        model: context.model.clone(),
        base_url: context.base_url.clone(),
        failure_summary: context.failure_summary.clone(),
        failed_check_count: context.failed_check_count,
        passed_check_count: context.passed_check_count,
        prompt_preview: context.prompt_preview.clone(),
        prompt_length: context.prompt_length,
    });
    review
}

fn preview_text(text: &str, max_chars: usize) -> String {
    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.trim().to_string();
    }

    let mut preview: String = text.chars().take(max_chars).collect();
    preview.push_str(&format!(
        "…[truncated {} more chars]",
        total_chars - max_chars
    ));
    preview
}

fn repair_summary(phase: &GoalRunPhase) -> String {
    format!(
        "Autonomous CTO repair during {}",
        serde_json::to_string(phase)
            .unwrap_or_default()
            .trim_matches('"')
    )
}

fn repair_skip_reason_label(reason: RepairSkipReason) -> &'static str {
    match reason {
        RepairSkipReason::NoApiKey => "no API key available",
        RepairSkipReason::NoActions => "repair agent returned no actions",
        RepairSkipReason::OnlyBlockedActions => "repair agent only proposed executor actions",
        RepairSkipReason::NoExecutableActions => "repair agent produced no executable actions",
        RepairSkipReason::RetryBudgetExhausted => "automatic repair budget exhausted",
        RepairSkipReason::RepeatedFailure => "same failure repeated",
    }
}

fn repair_outcome_blocker(outcome: &RepairOutcome) -> Option<String> {
    match outcome {
        RepairOutcome::Skipped { reason, .. } => Some(format!(
            "CTO repair skipped: {}",
            repair_skip_reason_label(*reason)
        )),
        RepairOutcome::Failed { stage, error, .. } => {
            Some(format!("CTO repair failed during {stage:?}: {error}"))
        }
        RepairOutcome::Executed { .. } => None,
    }
}

fn log_repair_not_attempted(
    db: &Mutex<Database>,
    goal_run_id: &str,
    phase: GoalRunPhase,
    error: &str,
    fingerprint: &str,
    retry_count: i64,
    reason: RepairSkipReason,
) {
    log_event(
        db,
        goal_run_id,
        phase,
        GoalRunEventKind::RepairSkipped,
        &format!("CTO repair skipped: {}", repair_skip_reason_label(reason)),
        Some(json!({
            "outcome": {
                "kind": "skipped",
                "reason": reason,
            },
            "failure": {
                "summary": error,
                "fingerprint": fingerprint,
                "retryCount": retry_count,
                "maxRepairRetries": MAX_REPAIR_RETRIES,
            }
        })),
    );
}

fn persist_repair_decision(
    db: &Mutex<Database>,
    project_id: &str,
    review: CtoDecisionReview,
    execution: Option<CtoDecisionExecution>,
    status: CtoDecisionStatus,
    summary: &str,
) -> Result<crate::models::CtoDecision, String> {
    let decision = CtoDecisionRecordInput {
        summary: summary.to_string(),
        review,
        execution,
        status,
    };
    let db = db.lock().map_err(|e| e.to_string())?;
    db.insert_cto_decision(project_id, &decision)
}

fn failure_fingerprint(phase: GoalRunPhase, message: &str) -> String {
    let normalized = message
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .take(12)
        .collect::<Vec<_>>()
        .join("-");
    format!(
        "{}:{}",
        serde_json::to_string(&phase)
            .unwrap_or_default()
            .trim_matches('"'),
        normalized
    )
}

fn update_goal_run_state(
    db: &Mutex<Database>,
    goal_run_id: &str,
    updates: GoalRunUpdate,
) -> Result<GoalRun, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.update_goal_run(goal_run_id, &updates)
}

fn log_event(
    db: &Mutex<Database>,
    goal_run_id: &str,
    phase: GoalRunPhase,
    kind: GoalRunEventKind,
    summary: &str,
    payload: Option<serde_json::Value>,
) {
    if let Ok(db) = db.lock() {
        let payload_str = payload.map(|value| value.to_string());
        let _ = db.append_goal_run_event(goal_run_id, phase, kind, summary, payload_str.as_deref());
    }
}

fn stop_requested(db: &Mutex<Database>, goal_run_id: &str) -> Result<bool, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    Ok(db.get_goal_run(goal_run_id)?.stop_requested)
}

fn maybe_scaffold_implementation_piece(
    db: &Mutex<Database>,
    goal_run: &GoalRun,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let existing = db.list_pieces(&goal_run.project_id)?;
    if !existing.is_empty() {
        return Ok(());
    }

    let piece = db.create_piece(&goal_run.project_id, None, "Implementation", 0.0, 0.0)?;
    db.update_piece(
        &piece.id,
        &PieceUpdate {
            responsibilities: Some(goal_run.prompt.clone()),
            agent_prompt: Some(goal_run.prompt.clone()),
            output_mode: Some(OutputMode::CodeOnly),
            phase: Some(Phase::Approved),
            ..Default::default()
        },
    )?;
    Ok(())
}

const MAX_REPAIR_RETRIES: i64 = 3;

/// Attempt an autonomous CTO repair after a phase failure.
///
/// Calls the CTO LLM with the failure context, parses any action blocks,
/// executes fix-up actions (excluding runPiece / runAllTasks / retryGoalStep,
/// which the executor handles itself by re-running the phase), and returns a
/// structured outcome describing whether the repair was skipped, executed, or
/// failed.
async fn attempt_cto_repair<R: tauri::Runtime>(
    app_handle: &AppHandle<R>,
    db: &Mutex<Database>,
    goal_run: &GoalRun,
    phase: GoalRunPhase,
    failure: &PhaseFailureContext,
) -> Result<RepairOutcome, String> {
    let state = app_handle.state::<AppState>();

    // Resolve LLM config
    let (messages, context, api_key) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let project = db.get_project(&goal_run.project_id)?;
        let (provider_name, api_key, model, base_url) =
            runner::resolve_llm_config(&project.settings);

        // Build CTO system prompt (full project context)
        let mut messages: Vec<Message> = agent::build_cto_prompt(&db, &goal_run.project_id);

        // Append the repair user turn built from structured failure context.
        let repair_prompt = build_repair_prompt(phase.clone(), failure);
        messages.push(Message {
            role: "user".to_string(),
            content: repair_prompt.clone(),
        });

        let context = RepairAttemptContext {
            goal_run_id: goal_run.id.clone(),
            project_id: goal_run.project_id.clone(),
            phase: phase.clone(),
            retry_count: goal_run.retry_count,
            failure_summary: failure.summary.trim().to_string(),
            failed_check_count: failure.failed_checks.len(),
            passed_check_count: failure.passed_checks.len(),
            provider_name,
            model: model.clone(),
            base_url,
            prompt_preview: preview_text(&repair_prompt, 900),
            prompt_length: repair_prompt.chars().count(),
        };

        (messages, context, api_key)
    };

    if api_key.is_empty() {
        let outcome = RepairOutcome::Skipped {
            reason: RepairSkipReason::NoApiKey,
            decision_id: None,
        };
        warn!(
            goal_run_id = %goal_run.id,
            provider = %context.provider_name,
            "No API key available for CTO repair agent — skipping repair"
        );
        log_event(
            &state.db,
            goal_run.id.as_str(),
            phase.clone(),
            GoalRunEventKind::RepairSkipped,
            "CTO repair agent skipped: no API key available",
            Some(repair_event_payload(&context, Some(&outcome))),
        );
        return Ok(outcome);
    }

    log_event(
        &state.db,
        goal_run.id.as_str(),
        phase.clone(),
        GoalRunEventKind::RepairStarted,
        "CTO repair agent started",
        Some(repair_event_payload(&context, None)),
    );

    let provider = llm::create_provider(&context.provider_name);
    let config = LlmConfig {
        api_key,
        model: context.model.clone(),
        base_url: context.base_url.clone(),
        max_tokens: 4096,
    };

    info!(
        goal_run_id = %goal_run.id,
        phase = ?phase,
        failed_check_count = context.failed_check_count,
        passed_check_count = context.passed_check_count,
        "Calling CTO repair agent"
    );

    let response = match provider.chat(&messages, &config).await {
        Ok(response) => response,
        Err(error) => {
            let outcome = RepairOutcome::Failed {
                stage: RepairFailureStage::Chat,
                error: error.to_string(),
                decision_id: None,
            };
            log_event(
                &state.db,
                goal_run.id.as_str(),
                phase.clone(),
                GoalRunEventKind::RepairFailed,
                "CTO repair agent failed while calling the model",
                Some(repair_event_payload(&context, Some(&outcome))),
            );
            return Ok(outcome);
        }
    };
    let assistant_text = response.content;

    // Parse action blocks from the CTO response
    let review = match cto_action_engine::review_cto_actions_impl(&assistant_text) {
        Ok(review) => review,
        Err(error) => {
            let outcome = RepairOutcome::Failed {
                stage: RepairFailureStage::Parse,
                error: error.to_string(),
                decision_id: None,
            };
            log_event(
                &state.db,
                goal_run.id.as_str(),
                phase.clone(),
                GoalRunEventKind::RepairFailed,
                "CTO repair agent failed while parsing the model response",
                Some(repair_event_payload(&context, Some(&outcome))),
            );
            return Ok(outcome);
        }
    };
    let review_with_context = repair_decision_review(review.clone(), &context);

    if review.actions.is_empty() {
        let decision = persist_repair_decision(
            db,
            &goal_run.project_id,
            review_with_context,
            None,
            CtoDecisionStatus::Rejected,
            &repair_summary(&phase),
        )?;
        let outcome = RepairOutcome::Skipped {
            reason: RepairSkipReason::NoActions,
            decision_id: Some(decision.id.clone()),
        };
        info!(
            goal_run_id = %goal_run.id,
            "CTO repair agent returned no actions"
        );
        log_event(
            &state.db,
            goal_run.id.as_str(),
            phase.clone(),
            GoalRunEventKind::RepairSkipped,
            "CTO repair agent returned no actions",
            Some(repair_event_payload(&context, Some(&outcome))),
        );
        return Ok(outcome);
    }

    // Filter out actions that conflict with the executor's own control flow
    let blocked_actions: &[&str] = &["runPiece", "runAllTasks", "retryGoalStep"];
    let _blocked_action_count = review
        .actions
        .iter()
        .filter(|action| {
            let name = action.get("action").and_then(Value::as_str).unwrap_or("");
            blocked_actions.contains(&name)
        })
        .count();
    let filtered_actions: Vec<Value> = review
        .actions
        .into_iter()
        .filter(|action| {
            let name = action.get("action").and_then(Value::as_str).unwrap_or("");
            !blocked_actions.contains(&name)
        })
        .collect();

    if filtered_actions.is_empty() {
        let decision = persist_repair_decision(
            db,
            &goal_run.project_id,
            review_with_context,
            None,
            CtoDecisionStatus::Rejected,
            &repair_summary(&phase),
        )?;
        let reason = if review.validation_errors.is_empty() {
            RepairSkipReason::OnlyBlockedActions
        } else {
            RepairSkipReason::NoExecutableActions
        };
        let outcome = RepairOutcome::Skipped {
            reason,
            decision_id: Some(decision.id.clone()),
        };
        info!(
            goal_run_id = %goal_run.id,
            "CTO repair agent only proposed execution actions — retrying phase without changes"
        );
        log_event(
            &state.db,
            goal_run.id.as_str(),
            phase.clone(),
            GoalRunEventKind::RepairSkipped,
            "CTO repair agent only proposed execution actions",
            Some(repair_event_payload(&context, Some(&outcome))),
        );
        return Ok(outcome);
    }

    let filtered_review = CtoDecisionReview {
        assistant_text: review_with_context.assistant_text.clone(),
        cleaned_content: review_with_context.cleaned_content.clone(),
        actions: filtered_actions,
        validation_errors: review_with_context.validation_errors.clone(),
        repair_context: review_with_context.repair_context.clone(),
    };

    let result = match cto_action_engine::execute_cto_actions_impl(
        &state.db,
        app_handle,
        goal_run.project_id.clone(),
        filtered_review.clone(),
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            let decision = persist_repair_decision(
                db,
                &goal_run.project_id,
                filtered_review.clone(),
                None,
                CtoDecisionStatus::Failed,
                &repair_summary(&phase),
            )?;
            let outcome = RepairOutcome::Failed {
                stage: RepairFailureStage::Execute,
                error: error.clone(),
                decision_id: Some(decision.id.clone()),
            };
            log_event(
                &state.db,
                goal_run.id.as_str(),
                phase.clone(),
                GoalRunEventKind::RepairFailed,
                "CTO repair agent failed while executing actions",
                Some(repair_event_payload(&context, Some(&outcome))),
            );
            return Ok(outcome);
        }
    };

    let decision_status = if result.executed == 0 && result.errors.is_empty() {
        CtoDecisionStatus::Rejected
    } else if result.errors.is_empty() {
        CtoDecisionStatus::Executed
    } else {
        CtoDecisionStatus::Failed
    };
    let decision = persist_repair_decision(
        db,
        &goal_run.project_id,
        filtered_review.clone(),
        Some(result.clone()),
        decision_status,
        &repair_summary(&phase),
    )?;

    if result.executed == 0 && result.errors.is_empty() {
        let outcome = RepairOutcome::Skipped {
            reason: RepairSkipReason::NoExecutableActions,
            decision_id: Some(decision.id.clone()),
        };
        log_event(
            &state.db,
            goal_run.id.as_str(),
            phase.clone(),
            GoalRunEventKind::RepairSkipped,
            "CTO repair agent executed no actions",
            Some(repair_event_payload(&context, Some(&outcome))),
        );
        return Ok(outcome);
    }

    let outcome = RepairOutcome::Executed {
        decision_id: decision.id.clone(),
        executed_actions: result.executed,
        errors: result.errors.clone(),
    };

    info!(
        goal_run_id = %goal_run.id,
        executed = result.executed,
        errors = result.errors.len(),
        "CTO repair agent executed actions"
    );

    log_event(
        &state.db,
        goal_run.id.as_str(),
        phase,
        GoalRunEventKind::RepairExecuted,
        "CTO repair agent executed actions",
        Some(repair_event_payload(&context, Some(&outcome))),
    );

    Ok(outcome)
}

/// RAII guard that owns the executor's slot in `running_goal_runs` and
/// `goal_run_cancels`. Its `Drop` fires the cancel token (so the heartbeat
/// task exits) and removes the map entries — unconditionally, whether the
/// executor future returns `Ok`, `Err`, or unwinds due to a panic. Without
/// this, a panic inside `advance_goal_run` would leave stale entries that
/// permanently block respawn of the same goal run.
struct GoalRunSlot<R: tauri::Runtime> {
    app_handle: AppHandle<R>,
    goal_run_id: String,
    cancel: CancellationToken,
}

impl<R: tauri::Runtime> Drop for GoalRunSlot<R> {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(state) = self.app_handle.try_state::<AppState>() {
            if let Ok(mut guard) = state.running_goal_runs.lock() {
                guard.remove(&self.goal_run_id);
            }
            if let Ok(mut map) = state.goal_run_cancels.lock() {
                map.remove(&self.goal_run_id);
            }
        }
    }
}

pub fn spawn_goal_run_executor<R: tauri::Runtime>(app_handle: AppHandle<R>, goal_run_id: String) {
    let state = app_handle.state::<AppState>();

    // Atomically guard against double-spawn via the running-set.
    {
        let mut running = match state.running_goal_runs.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        if !running.insert(goal_run_id.clone()) {
            return;
        }
    }

    // Install a cancellation token keyed by goal_run_id so pause/cancel commands
    // can unwind the executor (and its heartbeat + external CLI) promptly.
    let cancel = CancellationToken::new();
    if let Ok(mut map) = state.goal_run_cancels.lock() {
        map.insert(goal_run_id.clone(), cancel.clone());
    }

    // Heartbeat: bump last_heartbeat_at every 5s while the executor is alive.
    // The task co-dies with the executor via the shared token.
    let heartbeat_cancel = cancel.clone();
    let heartbeat_id = goal_run_id.clone();
    let heartbeat_handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let state = match heartbeat_handle.try_state::<AppState>() {
            Some(s) => s,
            None => return,
        };
        loop {
            tokio::select! {
                _ = heartbeat_cancel.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    if let Ok(db) = state.db.lock() {
                        let _ = db.update_heartbeat(&heartbeat_id);
                    }
                }
            }
        }
    });

    let exec_cancel = cancel.clone();
    tauri::async_runtime::spawn(async move {
        // The slot's Drop cleans up running_goal_runs + goal_run_cancels even
        // if `advance_goal_run` panics. Keep it on the stack for the whole task.
        let _slot = GoalRunSlot {
            app_handle: app_handle.clone(),
            goal_run_id: goal_run_id.clone(),
            cancel: exec_cancel.clone(),
        };
        let run_result = advance_goal_run(&app_handle, &goal_run_id, exec_cancel).await;
        if let Err(error) = run_result {
            error!(goal_run_id = %goal_run_id, error = %error, "Goal run executor failed");
        }
    });
}

async fn advance_goal_run<R: tauri::Runtime>(
    app_handle: &AppHandle<R>,
    goal_run_id: &str,
    cancel: CancellationToken,
) -> Result<(), String> {
    let state = app_handle.state::<AppState>();
    let initial = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.get_goal_run(goal_run_id)?
    };

    let start_ordinal = initial.phase.ordinal();

    // If resuming past Planning, load the existing plan from DB now so later phases have it.
    let rehydrated_plan: Option<WorkPlan> = if start_ordinal > GoalRunPhase::Planning.ordinal() {
        initial
            .current_plan_id
            .as_deref()
            .and_then(|plan_id| state.db.lock().ok()?.get_work_plan(plan_id).ok())
    } else {
        None
    };

    let mut goal_run = update_goal_run_state(
        &state.db,
        goal_run_id,
        GoalRunUpdate {
            status: Some(GoalRunStatus::Running),
            blocker_reason: Some(None),
            stop_requested: Some(false),
            ..Default::default()
        },
    )?;

    log_event(
        &state.db,
        goal_run_id,
        goal_run.phase.clone(),
        GoalRunEventKind::Note,
        "Goal run executor started",
        Some(json!({ "status": initial.status, "phase": initial.phase })),
    );

    if stop_requested(&state.db, goal_run_id)? {
        return Ok(());
    }

    let mut plan_holder: Option<WorkPlan> = None;

    if start_ordinal <= GoalRunPhase::Planning.ordinal() {
        maybe_scaffold_implementation_piece(&state.db, &goal_run)?;

        log_event(
            &state.db,
            goal_run_id,
            GoalRunPhase::Planning,
            GoalRunEventKind::PhaseStarted,
            "Planning started",
            None,
        );
        emit_phase_progress(
            app_handle,
            goal_run_id,
            GoalRunPhase::Planning,
            PhaseProgressStatus::Started,
            "Planning started",
        );
        goal_run = update_goal_run_state(
            &state.db,
            goal_run_id,
            GoalRunUpdate {
                phase: Some(GoalRunPhase::Planning),
                status: Some(GoalRunStatus::Running),
                ..Default::default()
            },
        )?;

        let plan = {
            let db = state.db.lock().map_err(|e| e.to_string())?;
            let plans = db.list_work_plans(&goal_run.project_id)?;
            plans
                .into_iter()
                .find(|item| item.status == crate::models::PlanStatus::Approved)
                .or_else(|| {
                    db.list_work_plans(&goal_run.project_id)
                        .ok()
                        .and_then(|plans| plans.into_iter().next())
                })
        };

        let mut plan = match plan {
            Some(plan)
                if !matches!(
                    plan.status,
                    crate::models::PlanStatus::Rejected | crate::models::PlanStatus::Superseded
                ) =>
            {
                plan
            }
            _ => {
                runner::run_leader_agent(
                    &goal_run.project_id,
                    &goal_run.prompt,
                    &state.db,
                    app_handle,
                )
                .await?
            }
        };

        if plan.status == crate::models::PlanStatus::Draft {
            let db = state.db.lock().map_err(|e| e.to_string())?;
            plan = db.update_work_plan(
                &plan.id,
                &crate::models::WorkPlanUpdate {
                    status: Some(crate::models::PlanStatus::Approved),
                    ..Default::default()
                },
            )?;
        }

        log_event(
            &state.db,
            goal_run_id,
            GoalRunPhase::Planning,
            GoalRunEventKind::PhaseCompleted,
            "Planning completed",
            Some(json!({ "planId": plan.id })),
        );
        emit_phase_progress(
            app_handle,
            goal_run_id,
            GoalRunPhase::Planning,
            PhaseProgressStatus::Completed,
            "Planning completed",
        );
        plan_holder = Some(plan);
    }

    // Use the plan produced by Planning, or the rehydrated one if we skipped Planning.
    let plan = plan_holder
        .or(rehydrated_plan)
        .ok_or_else(|| "Cannot resume past Planning: no plan found in DB".to_string())?;

    if start_ordinal <= GoalRunPhase::Implementation.ordinal() {
        goal_run = update_goal_run_state(
            &state.db,
            goal_run_id,
            GoalRunUpdate {
                phase: Some(GoalRunPhase::Implementation),
                status: Some(GoalRunStatus::Running),
                current_plan_id: Some(Some(plan.id.clone())),
                ..Default::default()
            },
        )?;
        log_event(
            &state.db,
            goal_run_id,
            GoalRunPhase::Implementation,
            GoalRunEventKind::PhaseStarted,
            "Implementation started",
            Some(json!({ "planId": plan.id })),
        );
        emit_phase_progress(
            app_handle,
            goal_run_id,
            GoalRunPhase::Implementation,
            PhaseProgressStatus::Started,
            "Implementation started",
        );

        if stop_requested(&state.db, goal_run_id)? {
            update_goal_run_state(
                &state.db,
                goal_run_id,
                GoalRunUpdate {
                    status: Some(GoalRunStatus::Blocked),
                    blocker_reason: Some(Some("Stopped by operator".to_string())),
                    ..Default::default()
                },
            )?;
            log_event(
                &state.db,
                goal_run_id,
                goal_run.phase.clone(),
                GoalRunEventKind::Stopped,
                "Goal run stopped by operator",
                None,
            );
            return Ok(());
        }

        'implementation: loop {
            match runner::run_all_plan_tasks(
                &plan.id,
                Some(goal_run_id),
                &state.db,
                &state.running_pieces,
                app_handle,
                Some(cancel.clone()),
            )
            .await
            {
                Ok(()) => {
                    log_event(
                        &state.db,
                        goal_run_id,
                        GoalRunPhase::Implementation,
                        GoalRunEventKind::PhaseCompleted,
                        "Implementation completed",
                        None,
                    );
                    emit_phase_progress(
                        app_handle,
                        goal_run_id,
                        GoalRunPhase::Implementation,
                        PhaseProgressStatus::Completed,
                        "Implementation completed",
                    );
                    break 'implementation;
                }
                Err(error) => {
                    // If the token fired, a pause/cancel command already set the final status.
                    if cancel.is_cancelled() {
                        return Ok(());
                    }
                    let fingerprint = failure_fingerprint(GoalRunPhase::Implementation, &error);
                    let current_retry = {
                        let db = state.db.lock().map_err(|e| e.to_string())?;
                        db.get_goal_run(goal_run_id)?.retry_count
                    };
                    let same_failure = goal_run
                        .last_failure_fingerprint
                        .as_deref()
                        .map(|prev| prev == fingerprint)
                        .unwrap_or(false);

                    let mut repair_blocker: Option<String> = None;
                    if current_retry < MAX_REPAIR_RETRIES && !same_failure {
                        goal_run = update_goal_run_state(
                            &state.db,
                            goal_run_id,
                            GoalRunUpdate {
                                phase: Some(GoalRunPhase::Implementation),
                                status: Some(GoalRunStatus::Retrying),
                                retry_count: Some(current_retry + 1),
                                last_failure_summary: Some(Some(error.clone())),
                                last_failure_fingerprint: Some(Some(fingerprint.clone())),
                                blocker_reason: Some(None),
                                attention_required: Some(false),
                                ..Default::default()
                            },
                        )?;
                        log_event(
                            &state.db,
                            goal_run_id,
                            GoalRunPhase::Implementation,
                            GoalRunEventKind::RetryScheduled,
                            &format!(
                                "Repair attempt {}/{MAX_REPAIR_RETRIES}: {error}",
                                current_retry + 1
                            ),
                            Some(
                                json!({ "fingerprint": fingerprint, "retryCount": current_retry + 1 }),
                            ),
                        );
                        emit_phase_progress(
                            app_handle,
                            goal_run_id,
                            GoalRunPhase::Implementation,
                            PhaseProgressStatus::Step,
                            format!("Repair attempt {}/{MAX_REPAIR_RETRIES}", current_retry + 1),
                        );
                        match attempt_cto_repair(
                            app_handle,
                            &state.db,
                            &goal_run,
                            GoalRunPhase::Implementation,
                            &PhaseFailureContext::from_summary(&error),
                        )
                        .await
                        {
                            Ok(RepairOutcome::Executed { .. }) => continue 'implementation,
                            Ok(outcome @ RepairOutcome::Skipped { .. }) => {
                                repair_blocker = repair_outcome_blocker(&outcome);
                            }
                            Ok(RepairOutcome::Failed {
                                stage,
                                error,
                                decision_id,
                            }) => {
                                warn!(
                                    goal_run_id,
                                    ?stage,
                                    decision_id = decision_id.as_deref().unwrap_or(""),
                                    repair_error = %error,
                                    "CTO repair agent failed"
                                );
                                repair_blocker =
                                    Some(format!("CTO repair failed during {stage:?}: {error}"));
                            }
                            Err(repair_err) => {
                                warn!(goal_run_id, repair_err = %repair_err, "CTO repair agent failed");
                            }
                        }
                    }

                    let final_error = repair_blocker.unwrap_or_else(|| error.clone());
                    update_goal_run_state(
                        &state.db,
                        goal_run_id,
                        GoalRunUpdate {
                            phase: Some(GoalRunPhase::Implementation),
                            status: Some(GoalRunStatus::Failed),
                            last_failure_summary: Some(Some(final_error.clone())),
                            last_failure_fingerprint: Some(Some(fingerprint.clone())),
                            attention_required: Some(true),
                            ..Default::default()
                        },
                    )?;
                    log_event(
                        &state.db,
                        goal_run_id,
                        GoalRunPhase::Implementation,
                        GoalRunEventKind::Failed,
                        &final_error,
                        Some(json!({ "fingerprint": fingerprint })),
                    );
                    return Err(final_error);
                }
            }
        }
    } // end if start_ordinal <= Implementation

    if start_ordinal <= GoalRunPhase::RuntimeConfiguration.ordinal() {
        log_event(
            &state.db,
            goal_run_id,
            GoalRunPhase::RuntimeConfiguration,
            GoalRunEventKind::PhaseStarted,
            "Runtime configuration started",
            None,
        );
        emit_phase_progress(
            app_handle,
            goal_run_id,
            GoalRunPhase::RuntimeConfiguration,
            PhaseProgressStatus::Started,
            "Runtime configuration started",
        );
        goal_run = update_goal_run_state(
            &state.db,
            goal_run_id,
            GoalRunUpdate {
                phase: Some(GoalRunPhase::RuntimeConfiguration),
                status: Some(GoalRunStatus::Running),
                ..Default::default()
            },
        )?;

        let runtime_status = runtime_commands::get_runtime_status_impl(
            &state.db,
            &state.runtime_sessions,
            goal_run.project_id.clone(),
        )
        .await?;
        if runtime_status.spec.is_none() {
            let mut detected =
                runtime_commands::detect_runtime_impl(&state.db, goal_run.project_id.clone())
                    .await?;
            if detected.is_none() {
                detected = runtime_commands::detect_runtime_with_agent_impl(
                    &state.db,
                    app_handle,
                    goal_run.project_id.clone(),
                )
                .await?;
            }

            let Some(detected) = detected else {
                let message =
                    "Automatic runtime detection failed. Review or configure the run command."
                        .to_string();
                update_goal_run_state(
                    &state.db,
                    goal_run_id,
                    GoalRunUpdate {
                        phase: Some(GoalRunPhase::RuntimeConfiguration),
                        status: Some(GoalRunStatus::Blocked),
                        blocker_reason: Some(Some(message.clone())),
                        last_failure_summary: Some(Some(message.clone())),
                        attention_required: Some(true),
                        runtime_status_summary: Some(Some("runtime not configured".to_string())),
                        ..Default::default()
                    },
                )?;
                log_event(
                    &state.db,
                    goal_run_id,
                    GoalRunPhase::RuntimeConfiguration,
                    GoalRunEventKind::Blocked,
                    &message,
                    None,
                );
                return Ok(());
            };

            runtime_commands::configure_runtime_impl(
                &state.db,
                &state.runtime_sessions,
                goal_run.project_id.clone(),
                detected,
            )
            .await?;
            let _ = runtime_commands::get_runtime_status_impl(
                &state.db,
                &state.runtime_sessions,
                goal_run.project_id.clone(),
            )
            .await?;
        }
        log_event(
            &state.db,
            goal_run_id,
            GoalRunPhase::RuntimeConfiguration,
            GoalRunEventKind::PhaseCompleted,
            "Runtime configured",
            None,
        );
        emit_phase_progress(
            app_handle,
            goal_run_id,
            GoalRunPhase::RuntimeConfiguration,
            PhaseProgressStatus::Completed,
            "Runtime configured",
        );
    } // end if start_ordinal <= RuntimeConfiguration

    if start_ordinal <= GoalRunPhase::RuntimeExecution.ordinal() {
        // Re-fetch runtime_status here so it's always available regardless of whether
        // RuntimeConfiguration was skipped.
        let runtime_status = runtime_commands::get_runtime_status_impl(
            &state.db,
            &state.runtime_sessions,
            goal_run.project_id.clone(),
        )
        .await?;

        log_event(
            &state.db,
            goal_run_id,
            GoalRunPhase::RuntimeExecution,
            GoalRunEventKind::PhaseStarted,
            "Runtime execution started",
            None,
        );
        emit_phase_progress(
            app_handle,
            goal_run_id,
            GoalRunPhase::RuntimeExecution,
            PhaseProgressStatus::Started,
            "Runtime execution started",
        );
        goal_run = update_goal_run_state(
            &state.db,
            goal_run_id,
            GoalRunUpdate {
                phase: Some(GoalRunPhase::RuntimeExecution),
                status: Some(GoalRunStatus::Running),
                runtime_status_summary: Some(Some(
                    runtime_status
                        .spec
                        .as_ref()
                        .map(|spec| spec.run_command.clone())
                        .unwrap_or_else(|| "runtime configured".to_string()),
                )),
                ..Default::default()
            },
        )?;

        'runtime_execution: loop {
            match runtime_commands::start_runtime_impl(
                &state.db,
                &state.runtime_sessions,
                goal_run.project_id.clone(),
            )
            .await
            {
                Ok(_) => {
                    log_event(
                        &state.db,
                        goal_run_id,
                        GoalRunPhase::RuntimeExecution,
                        GoalRunEventKind::PhaseCompleted,
                        "Runtime started",
                        None,
                    );
                    emit_phase_progress(
                        app_handle,
                        goal_run_id,
                        GoalRunPhase::RuntimeExecution,
                        PhaseProgressStatus::Completed,
                        "Runtime started",
                    );
                    break 'runtime_execution;
                }
                Err(error) => {
                    if cancel.is_cancelled() {
                        return Ok(());
                    }
                    let fingerprint = failure_fingerprint(GoalRunPhase::RuntimeExecution, &error);
                    let current_retry = {
                        let db = state.db.lock().map_err(|e| e.to_string())?;
                        db.get_goal_run(goal_run_id)?.retry_count
                    };
                    let same_failure = goal_run
                        .last_failure_fingerprint
                        .as_deref()
                        .map(|prev| prev == fingerprint)
                        .unwrap_or(false);

                    let mut repair_blocker: Option<String> = None;
                    if current_retry < MAX_REPAIR_RETRIES && !same_failure {
                        goal_run = update_goal_run_state(
                            &state.db,
                            goal_run_id,
                            GoalRunUpdate {
                                phase: Some(GoalRunPhase::RuntimeExecution),
                                status: Some(GoalRunStatus::Retrying),
                                retry_count: Some(current_retry + 1),
                                last_failure_summary: Some(Some(error.clone())),
                                last_failure_fingerprint: Some(Some(fingerprint.clone())),
                                blocker_reason: Some(None),
                                attention_required: Some(false),
                                ..Default::default()
                            },
                        )?;
                        log_event(
                            &state.db,
                            goal_run_id,
                            GoalRunPhase::RuntimeExecution,
                            GoalRunEventKind::RetryScheduled,
                            &format!(
                                "Repair attempt {}/{MAX_REPAIR_RETRIES}: {error}",
                                current_retry + 1
                            ),
                            Some(
                                json!({ "fingerprint": fingerprint, "retryCount": current_retry + 1 }),
                            ),
                        );
                        emit_phase_progress(
                            app_handle,
                            goal_run_id,
                            GoalRunPhase::RuntimeExecution,
                            PhaseProgressStatus::Step,
                            format!("Repair attempt {}/{MAX_REPAIR_RETRIES}", current_retry + 1),
                        );
                        match attempt_cto_repair(
                            app_handle,
                            &state.db,
                            &goal_run,
                            GoalRunPhase::RuntimeExecution,
                            &PhaseFailureContext::from_summary(&error),
                        )
                        .await
                        {
                            Ok(RepairOutcome::Executed { .. }) => continue 'runtime_execution,
                            Ok(outcome @ RepairOutcome::Skipped { .. }) => {
                                repair_blocker = repair_outcome_blocker(&outcome);
                            }
                            Ok(RepairOutcome::Failed {
                                stage,
                                error,
                                decision_id,
                            }) => {
                                warn!(
                                    goal_run_id,
                                    ?stage,
                                    decision_id = decision_id.as_deref().unwrap_or(""),
                                    repair_error = %error,
                                    "CTO repair agent failed"
                                );
                                repair_blocker =
                                    Some(format!("CTO repair failed during {stage:?}: {error}"));
                            }
                            Err(repair_err) => {
                                warn!(goal_run_id, repair_err = %repair_err, "CTO repair agent failed");
                            }
                        }
                    }

                    let final_error = repair_blocker.unwrap_or_else(|| error.clone());
                    update_goal_run_state(
                        &state.db,
                        goal_run_id,
                        GoalRunUpdate {
                            phase: Some(GoalRunPhase::RuntimeExecution),
                            status: Some(GoalRunStatus::Blocked),
                            blocker_reason: Some(Some(final_error.clone())),
                            last_failure_summary: Some(Some(final_error.clone())),
                            last_failure_fingerprint: Some(Some(fingerprint.clone())),
                            attention_required: Some(true),
                            ..Default::default()
                        },
                    )?;
                    log_event(
                        &state.db,
                        goal_run_id,
                        GoalRunPhase::RuntimeExecution,
                        GoalRunEventKind::Blocked,
                        &final_error,
                        Some(json!({ "fingerprint": fingerprint })),
                    );
                    return Ok(());
                }
            }
        }
    } // end if start_ordinal <= RuntimeExecution

    if start_ordinal <= GoalRunPhase::Verification.ordinal() {
        goal_run = update_goal_run_state(
            &state.db,
            goal_run_id,
            GoalRunUpdate {
                phase: Some(GoalRunPhase::Verification),
                status: Some(GoalRunStatus::Running),
                ..Default::default()
            },
        )?;
        emit_phase_progress(
            app_handle,
            goal_run_id,
            GoalRunPhase::Verification,
            PhaseProgressStatus::Started,
            "Verification started",
        );

        'verification: loop {
            let verification_result = runtime_commands::verify_runtime_impl(
                &state.db,
                &state.runtime_sessions,
                goal_run.project_id.clone(),
                cancel.clone(),
            )
            .await;

            let verification_result: VerificationResult = match verification_result {
                Ok(result) => result,
                Err(infra_error) => {
                    if cancel.is_cancelled() {
                        return Ok(());
                    }
                    // Infrastructure error (runtime not running, spec missing) — blocked, no repair.
                    let fingerprint = failure_fingerprint(GoalRunPhase::Verification, &infra_error);
                    update_goal_run_state(
                        &state.db,
                        goal_run_id,
                        GoalRunUpdate {
                            phase: Some(GoalRunPhase::Verification),
                            status: Some(GoalRunStatus::Blocked),
                            blocker_reason: Some(Some(infra_error.clone())),
                            last_failure_summary: Some(Some(infra_error.clone())),
                            last_failure_fingerprint: Some(Some(fingerprint.clone())),
                            attention_required: Some(true),
                            ..Default::default()
                        },
                    )?;
                    log_event(
                        &state.db,
                        goal_run_id,
                        GoalRunPhase::Verification,
                        GoalRunEventKind::Blocked,
                        &infra_error,
                        Some(json!({ "fingerprint": fingerprint })),
                    );
                    return Ok(());
                }
            };

            let result_json = serde_json::to_string(&verification_result)
                .unwrap_or_else(|_| verification_result.message.clone());

            if verification_result.passed {
                update_goal_run_state(
                    &state.db,
                    goal_run_id,
                    GoalRunUpdate {
                        phase: Some(GoalRunPhase::Verification),
                        status: Some(GoalRunStatus::Completed),
                        blocker_reason: Some(None),
                        last_failure_summary: Some(None),
                        verification_summary: Some(Some(result_json)),
                        attention_required: Some(false),
                        ..Default::default()
                    },
                )?;
                log_event(
                    &state.db,
                    goal_run_id,
                    GoalRunPhase::Verification,
                    GoalRunEventKind::PhaseCompleted,
                    "Verification completed",
                    Some(json!({ "message": verification_result.message })),
                );
                emit_phase_progress(
                    app_handle,
                    goal_run_id,
                    GoalRunPhase::Verification,
                    PhaseProgressStatus::Completed,
                    "Verification completed",
                );
                break 'verification;
            }

            // Verification check failure — attempt CTO repair (same pattern as RuntimeExecution).
            if cancel.is_cancelled() {
                return Ok(());
            }
            let error = verification_result.message.clone();
            let fingerprint = failure_fingerprint(GoalRunPhase::Verification, &error);
            let gate_run = {
                let db = state.db.lock().map_err(|e| e.to_string())?;
                db.get_goal_run(goal_run_id)?
            };
            let current_retry = gate_run.retry_count;
            let operator_repair_requested = gate_run.operator_repair_requested;
            let same_failure = goal_run
                .last_failure_fingerprint
                .as_deref()
                .map(|prev| prev == fingerprint)
                .unwrap_or(false);

            if operator_repair_requested || (current_retry < MAX_REPAIR_RETRIES && !same_failure) {
                let next_retry_count = if operator_repair_requested {
                    current_retry
                } else {
                    current_retry + 1
                };
                goal_run = update_goal_run_state(
                    &state.db,
                    goal_run_id,
                    GoalRunUpdate {
                        phase: Some(GoalRunPhase::Verification),
                        status: Some(GoalRunStatus::Retrying),
                        retry_count: Some(next_retry_count),
                        last_failure_summary: Some(Some(error.clone())),
                        last_failure_fingerprint: Some(Some(fingerprint.clone())),
                        verification_summary: Some(Some(result_json.clone())),
                        blocker_reason: Some(None),
                        attention_required: Some(false),
                        operator_repair_requested: Some(false),
                        ..Default::default()
                    },
                )?;
                if operator_repair_requested {
                    emit_phase_progress(
                        app_handle,
                        goal_run_id,
                        GoalRunPhase::Verification,
                        PhaseProgressStatus::Step,
                        "Operator repair attempt",
                    );
                } else {
                    log_event(
                        &state.db,
                        goal_run_id,
                        GoalRunPhase::Verification,
                        GoalRunEventKind::RetryScheduled,
                        &format!(
                            "Repair attempt {}/{MAX_REPAIR_RETRIES}: {error}",
                            current_retry + 1
                        ),
                        Some(
                            json!({ "fingerprint": fingerprint, "retryCount": current_retry + 1 }),
                        ),
                    );
                    emit_phase_progress(
                        app_handle,
                        goal_run_id,
                        GoalRunPhase::Verification,
                        PhaseProgressStatus::Step,
                        format!("Repair attempt {}/{MAX_REPAIR_RETRIES}", current_retry + 1),
                    );
                }
                match attempt_cto_repair(
                    app_handle,
                    &state.db,
                    &goal_run,
                    GoalRunPhase::Verification,
                    &PhaseFailureContext::from_verification(&verification_result),
                )
                .await
                {
                    Ok(RepairOutcome::Executed { .. }) => continue 'verification,
                    Ok(outcome @ RepairOutcome::Skipped { .. }) => {
                        let repair_blocker = repair_outcome_blocker(&outcome)
                            .unwrap_or_else(|| "CTO repair skipped".to_string());
                        update_goal_run_state(
                            &state.db,
                            goal_run_id,
                            GoalRunUpdate {
                                phase: Some(GoalRunPhase::Verification),
                                status: Some(GoalRunStatus::Blocked),
                                blocker_reason: Some(Some(repair_blocker.clone())),
                                last_failure_summary: Some(Some(error.clone())),
                                last_failure_fingerprint: Some(Some(fingerprint.clone())),
                                verification_summary: Some(Some(result_json)),
                                attention_required: Some(true),
                                operator_repair_requested: Some(false),
                                ..Default::default()
                            },
                        )?;
                        log_event(
                            &state.db,
                            goal_run_id,
                            GoalRunPhase::Verification,
                            GoalRunEventKind::Blocked,
                            &repair_blocker,
                            Some(json!({ "fingerprint": fingerprint })),
                        );
                        return Ok(());
                    }
                    Ok(RepairOutcome::Failed {
                        stage,
                        error: repair_error,
                        decision_id,
                    }) => {
                        warn!(
                            goal_run_id,
                            ?stage,
                            decision_id = decision_id.as_deref().unwrap_or(""),
                            repair_error = %repair_error,
                            "CTO repair agent failed during verification"
                        );
                        let repair_blocker =
                            format!("CTO repair failed during {stage:?}: {repair_error}");
                        update_goal_run_state(
                            &state.db,
                            goal_run_id,
                            GoalRunUpdate {
                                phase: Some(GoalRunPhase::Verification),
                                status: Some(GoalRunStatus::Blocked),
                                blocker_reason: Some(Some(repair_blocker.clone())),
                                last_failure_summary: Some(Some(error.clone())),
                                last_failure_fingerprint: Some(Some(fingerprint.clone())),
                                verification_summary: Some(Some(result_json)),
                                attention_required: Some(true),
                                operator_repair_requested: Some(false),
                                ..Default::default()
                            },
                        )?;
                        log_event(
                            &state.db,
                            goal_run_id,
                            GoalRunPhase::Verification,
                            GoalRunEventKind::Blocked,
                            &repair_blocker,
                            Some(json!({ "fingerprint": fingerprint })),
                        );
                        return Ok(());
                    }
                    Err(repair_err) => {
                        warn!(goal_run_id, repair_err = %repair_err, "CTO repair agent failed during verification");
                    }
                }
            } else {
                let reason = if current_retry >= MAX_REPAIR_RETRIES {
                    RepairSkipReason::RetryBudgetExhausted
                } else {
                    RepairSkipReason::RepeatedFailure
                };
                log_repair_not_attempted(
                    &state.db,
                    goal_run_id,
                    GoalRunPhase::Verification,
                    &error,
                    &fingerprint,
                    current_retry,
                    reason,
                );
            }

            update_goal_run_state(
                &state.db,
                goal_run_id,
                GoalRunUpdate {
                    phase: Some(GoalRunPhase::Verification),
                    status: Some(GoalRunStatus::Blocked),
                    blocker_reason: Some(Some(error.clone())),
                    last_failure_summary: Some(Some(error.clone())),
                    last_failure_fingerprint: Some(Some(fingerprint.clone())),
                    verification_summary: Some(Some(result_json)),
                    attention_required: Some(true),
                    ..Default::default()
                },
            )?;
            log_event(
                &state.db,
                goal_run_id,
                GoalRunPhase::Verification,
                GoalRunEventKind::Blocked,
                &error,
                Some(json!({ "fingerprint": fingerprint })),
            );
            return Ok(());
        }
    } // end if start_ordinal <= Verification

    info!(goal_run_id = %goal_run_id, "Goal run executor finished");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn preview_text_truncates_long_strings() {
        let text = "abcdefghijklmnopqrstuvwxyz";
        let preview = preview_text(text, 10);
        assert!(preview.starts_with("abcdefghij"));
        assert!(preview.contains("truncated 16 more chars"));
    }

    #[test]
    fn repair_decision_review_attaches_sanitized_context() {
        let review = CtoDecisionReview {
            assistant_text: "assistant".to_string(),
            cleaned_content: "clean".to_string(),
            actions: vec![],
            validation_errors: vec![],
            repair_context: None,
        };
        let context = RepairAttemptContext {
            goal_run_id: "goal-run-1".to_string(),
            project_id: "project-1".to_string(),
            phase: GoalRunPhase::Verification,
            retry_count: 2,
            failure_summary: "verification failed".to_string(),
            failed_check_count: 1,
            passed_check_count: 0,
            provider_name: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            base_url: Some("https://example.invalid".to_string()),
            prompt_preview: "prompt preview".to_string(),
            prompt_length: 14,
        };

        let reviewed = repair_decision_review(review, &context);
        let repair_context = reviewed.repair_context.expect("repair context");
        assert_eq!(repair_context.goal_run_id, "goal-run-1");
        assert_eq!(repair_context.phase, GoalRunPhase::Verification);
        assert_eq!(repair_context.retry_count, 2);
        assert_eq!(repair_context.failed_check_count, 1);
        assert_eq!(repair_context.prompt_preview, "prompt preview");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn repair_with_no_actions_persists_skipped_event() {
        std::env::set_var("OPENAI_API_KEY", "test-openai-key");
        crate::llm::set_test_llm_responses(crate::llm::TestLlmResponses::default());

        let workspace = std::env::temp_dir().join(format!(
            "project-builder-repair-skip-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workspace).expect("create workspace");
        let db_path = workspace.join("data.db");
        let db = Database::new_at_path(&db_path).expect("open db");
        let state_db = Mutex::new(db);

        let (project_id, goal_run) = {
            let db = state_db.lock().expect("lock db");
            let project = db
                .create_project_with_settings(
                    "Repair Skip",
                    "No-actions repair path",
                    crate::models::ProjectSettings {
                        working_directory: Some(workspace.to_string_lossy().to_string()),
                        ..Default::default()
                    },
                )
                .expect("create project");
            let goal_run = db
                .create_goal_run(&project.id, "trigger repair")
                .expect("create goal run");
            (project.id, goal_run)
        };

        let app = tauri::test::mock_app();
        let app_handle = app.handle().clone();
        app.manage(AppState {
            db: state_db,
            running_pieces: Mutex::new(HashSet::new()),
            running_goal_runs: Mutex::new(HashSet::new()),
            goal_run_cancels: Mutex::new(HashMap::new()),
            runtime_sessions: Mutex::new(HashMap::new()),
        });

        let state = app_handle.state::<AppState>();
        let outcome = attempt_cto_repair(
            &app_handle,
            &state.db,
            &goal_run,
            GoalRunPhase::Verification,
            &PhaseFailureContext::from_summary("forced failure"),
        )
        .await
        .expect("repair outcome");

        match outcome {
            RepairOutcome::Skipped {
                reason: RepairSkipReason::NoActions,
                decision_id: Some(_),
            } => {}
            other => panic!("expected no-actions skip, got {other:?}"),
        }

        let events = {
            let db = state.db.lock().expect("lock db");
            db.list_goal_run_events(&goal_run.id)
                .expect("list goal run events")
        };
        let skipped = events
            .iter()
            .find(|event| event.kind == GoalRunEventKind::RepairSkipped)
            .expect("repair skipped event");
        assert!(skipped.summary.contains("no actions"));
        assert!(skipped
            .payload_json
            .as_deref()
            .unwrap_or("")
            .contains(&project_id));

        let _ = std::fs::remove_dir_all(&workspace);
    }
}
