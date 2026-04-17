use crate::agent::{self, runner};
use crate::commands::{cto_action_engine, runtime_commands};
use crate::db::{Database, PieceUpdate};
use crate::llm::{self, LlmConfig, Message};
use crate::models::{
    GoalRun, GoalRunEventKind, GoalRunPhase, GoalRunStatus, GoalRunUpdate, OutputMode, Phase,
    VerificationResult, WorkPlan,
};
use crate::AppState;
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

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
        let _ = db.append_goal_run_event(
            goal_run_id,
            phase,
            kind,
            summary,
            payload_str.as_deref(),
        );
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
/// which the executor handles itself by re-running the phase), and returns
/// `true` when at least one action was executed.
async fn attempt_cto_repair<R: tauri::Runtime>(
    app_handle: &AppHandle<R>,
    db: &Mutex<Database>,
    goal_run: &GoalRun,
    phase: GoalRunPhase,
    failure_summary: &str,
) -> Result<bool, String> {
    let state = app_handle.state::<AppState>();

    // Resolve LLM config
    let (messages, provider_name, api_key, model, base_url) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let project = db.get_project(&goal_run.project_id)?;
        let (provider_name, api_key, model, base_url) =
            runner::resolve_llm_config(&project.settings);

        // Build CTO system prompt (full project context)
        let mut messages: Vec<Message> = agent::build_cto_prompt(&db, &goal_run.project_id);

        // Append the repair user turn
        let phase_str = serde_json::to_string(&phase)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        let repair_prompt = format!(
            "The goal run has failed during the **{phase_str}** phase.\n\n\
             Error:\n{failure_summary}\n\n\
             Diagnose the failure and propose concrete fixes using action blocks. \
             Focus on updatePiece, createPiece, configureRuntime, generatePlan, or approvePlan — \
             do NOT use runPiece, runAllTasks, or retryGoalStep, as the system retries the phase automatically after your fixes."
        );
        messages.push(Message {
            role: "user".to_string(),
            content: repair_prompt,
        });

        (messages, provider_name, api_key, model, base_url)
    };

    if api_key.is_empty() {
        warn!(
            goal_run_id = %goal_run.id,
            provider = %provider_name,
            "No API key available for CTO repair agent — skipping repair"
        );
        return Ok(false);
    }

    let provider = llm::create_provider(&provider_name);
    let config = LlmConfig {
        api_key,
        model,
        base_url,
        max_tokens: 4096,
    };

    info!(
        goal_run_id = %goal_run.id,
        phase = ?phase,
        "Calling CTO repair agent"
    );

    let response = provider.chat(&messages, &config).await?;
    let assistant_text = response.content;

    // Parse action blocks from the CTO response
    let review = cto_action_engine::review_cto_actions_impl(&assistant_text)?;

    if review.actions.is_empty() {
        info!(
            goal_run_id = %goal_run.id,
            "CTO repair agent returned no actions"
        );
        return Ok(false);
    }

    // Filter out actions that conflict with the executor's own control flow
    let blocked_actions: &[&str] = &["runPiece", "runAllTasks", "retryGoalStep"];
    let filtered_actions: Vec<Value> = review
        .actions
        .into_iter()
        .filter(|action| {
            let name = action.get("action").and_then(Value::as_str).unwrap_or("");
            !blocked_actions.contains(&name)
        })
        .collect();

    if filtered_actions.is_empty() {
        info!(
            goal_run_id = %goal_run.id,
            "CTO repair agent only proposed execution actions — retrying phase without changes"
        );
        return Ok(false);
    }

    let filtered_review = crate::models::CtoDecisionReview {
        assistant_text: review.assistant_text,
        cleaned_content: review.cleaned_content,
        actions: filtered_actions,
        validation_errors: review.validation_errors,
    };

    let result = cto_action_engine::execute_cto_actions_impl(
        &state.db,
        app_handle,
        goal_run.project_id.clone(),
        filtered_review,
    )
    .await?;

    info!(
        goal_run_id = %goal_run.id,
        executed = result.executed,
        errors = result.errors.len(),
        "CTO repair agent executed actions"
    );

    Ok(result.executed > 0)
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
        initial.current_plan_id.as_deref().and_then(|plan_id| {
            state.db.lock().ok()?.get_work_plan(plan_id).ok()
        })
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
            _ => runner::run_leader_agent(&goal_run.project_id, &goal_run.prompt, &state.db, app_handle)
                .await?,
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
                            &format!("Repair attempt {}/{MAX_REPAIR_RETRIES}: {error}", current_retry + 1),
                            Some(json!({ "fingerprint": fingerprint, "retryCount": current_retry + 1 })),
                        );
                        match attempt_cto_repair(
                            app_handle,
                            &state.db,
                            &goal_run,
                            GoalRunPhase::Implementation,
                            &error,
                        )
                        .await
                        {
                            Ok(_) => continue 'implementation,
                            Err(repair_err) => {
                                warn!(goal_run_id, repair_err = %repair_err, "CTO repair agent failed");
                            }
                        }
                    }

                    update_goal_run_state(
                        &state.db,
                        goal_run_id,
                        GoalRunUpdate {
                            phase: Some(GoalRunPhase::Implementation),
                            status: Some(GoalRunStatus::Failed),
                            last_failure_summary: Some(Some(error.clone())),
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
                        &error,
                        Some(json!({ "fingerprint": fingerprint })),
                    );
                    return Err(error);
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
        goal_run = update_goal_run_state(
            &state.db,
            goal_run_id,
            GoalRunUpdate {
                phase: Some(GoalRunPhase::RuntimeConfiguration),
                status: Some(GoalRunStatus::Running),
                ..Default::default()
            },
        )?;

        let runtime_status =
            runtime_commands::get_runtime_status_impl(&state.db, &state.runtime_sessions, goal_run.project_id.clone()).await?;
        if runtime_status.spec.is_none() {
            let mut detected =
                runtime_commands::detect_runtime_impl(&state.db, goal_run.project_id.clone()).await?;
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
            let _ = runtime_commands::get_runtime_status_impl(&state.db, &state.runtime_sessions, goal_run.project_id.clone()).await?;
        }
        log_event(
            &state.db,
            goal_run_id,
            GoalRunPhase::RuntimeConfiguration,
            GoalRunEventKind::PhaseCompleted,
            "Runtime configured",
            None,
        );
    } // end if start_ordinal <= RuntimeConfiguration

    if start_ordinal <= GoalRunPhase::RuntimeExecution.ordinal() {
        // Re-fetch runtime_status here so it's always available regardless of whether
        // RuntimeConfiguration was skipped.
        let runtime_status =
            runtime_commands::get_runtime_status_impl(&state.db, &state.runtime_sessions, goal_run.project_id.clone()).await?;

        log_event(
            &state.db,
            goal_run_id,
            GoalRunPhase::RuntimeExecution,
            GoalRunEventKind::PhaseStarted,
            "Runtime execution started",
            None,
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
                            &format!("Repair attempt {}/{MAX_REPAIR_RETRIES}: {error}", current_retry + 1),
                            Some(json!({ "fingerprint": fingerprint, "retryCount": current_retry + 1 })),
                        );
                        match attempt_cto_repair(
                            app_handle,
                            &state.db,
                            &goal_run,
                            GoalRunPhase::RuntimeExecution,
                            &error,
                        )
                        .await
                        {
                            Ok(_) => continue 'runtime_execution,
                            Err(repair_err) => {
                                warn!(goal_run_id, repair_err = %repair_err, "CTO repair agent failed");
                            }
                        }
                    }

                    update_goal_run_state(
                        &state.db,
                        goal_run_id,
                        GoalRunUpdate {
                            phase: Some(GoalRunPhase::RuntimeExecution),
                            status: Some(GoalRunStatus::Blocked),
                            blocker_reason: Some(Some(error.clone())),
                            last_failure_summary: Some(Some(error.clone())),
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
                        &error,
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

    'verification: loop {
        let verification_result =
            runtime_commands::verify_runtime_impl(&state.db, &state.runtime_sessions, goal_run.project_id.clone()).await;

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
            break 'verification;
        }

        // Verification check failure — attempt CTO repair (same pattern as RuntimeExecution).
        if cancel.is_cancelled() {
            return Ok(());
        }
        let error = verification_result.message.clone();
        let fingerprint = failure_fingerprint(GoalRunPhase::Verification, &error);
        let current_retry = {
            let db = state.db.lock().map_err(|e| e.to_string())?;
            db.get_goal_run(goal_run_id)?.retry_count
        };
        let same_failure = goal_run
            .last_failure_fingerprint
            .as_deref()
            .map(|prev| prev == fingerprint)
            .unwrap_or(false);

        if current_retry < MAX_REPAIR_RETRIES && !same_failure {
            goal_run = update_goal_run_state(
                &state.db,
                goal_run_id,
                GoalRunUpdate {
                    phase: Some(GoalRunPhase::Verification),
                    status: Some(GoalRunStatus::Retrying),
                    retry_count: Some(current_retry + 1),
                    last_failure_summary: Some(Some(error.clone())),
                    last_failure_fingerprint: Some(Some(fingerprint.clone())),
                    verification_summary: Some(Some(result_json.clone())),
                    blocker_reason: Some(None),
                    attention_required: Some(false),
                    ..Default::default()
                },
            )?;
            log_event(
                &state.db,
                goal_run_id,
                GoalRunPhase::Verification,
                GoalRunEventKind::RetryScheduled,
                &format!("Repair attempt {}/{MAX_REPAIR_RETRIES}: {error}", current_retry + 1),
                Some(json!({ "fingerprint": fingerprint, "retryCount": current_retry + 1 })),
            );
            match attempt_cto_repair(
                app_handle,
                &state.db,
                &goal_run,
                GoalRunPhase::Verification,
                &error,
            )
            .await
            {
                Ok(_) => continue 'verification,
                Err(repair_err) => {
                    warn!(goal_run_id, repair_err = %repair_err, "CTO repair agent failed during verification");
                }
            }
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
