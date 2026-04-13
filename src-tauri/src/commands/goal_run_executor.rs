use crate::agent::runner;
use crate::commands::runtime_commands;
use crate::db::{Database, PieceUpdate};
use crate::models::{
    GoalRun, GoalRunEventKind, GoalRunPhase, GoalRunStatus, GoalRunUpdate, OutputMode, Phase,
};
use crate::AppState;
use serde_json::json;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tracing::{error, info};

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

pub fn spawn_goal_run_executor<R: tauri::Runtime>(app_handle: AppHandle<R>, goal_run_id: String) {
    let state = app_handle.state::<AppState>();
    {
        let mut running = match state.running_goal_runs.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        if !running.insert(goal_run_id.clone()) {
            return;
        }
    }

    tauri::async_runtime::spawn(async move {
        let run_result = advance_goal_run(&app_handle, &goal_run_id).await;
        if let Err(error) = run_result {
            error!(goal_run_id = %goal_run_id, error = %error, "Goal run executor failed");
        }
        if let Some(state) = app_handle.try_state::<AppState>() {
            if let Ok(mut guard) = state.running_goal_runs.lock() {
                guard.remove(&goal_run_id);
            }
        }
    });
}

async fn advance_goal_run<R: tauri::Runtime>(
    app_handle: &AppHandle<R>,
    goal_run_id: &str,
) -> Result<(), String> {
    let state = app_handle.state::<AppState>();
    let initial = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.get_goal_run(goal_run_id)?
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

    if let Err(error) = runner::run_all_plan_tasks(
        &plan.id,
        &state.db,
        &state.running_pieces,
        app_handle,
    )
    .await
    {
        let fingerprint = failure_fingerprint(GoalRunPhase::Implementation, &error);
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

    let mut runtime_status =
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
        runtime_status =
            runtime_commands::get_runtime_status_impl(&state.db, &state.runtime_sessions, goal_run.project_id.clone()).await?;
    }

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

    if let Err(error) = runtime_commands::start_runtime_impl(
        &state.db,
        &state.runtime_sessions,
        goal_run.project_id.clone(),
    )
    .await
    {
        let fingerprint = failure_fingerprint(GoalRunPhase::RuntimeExecution, &error);
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

    goal_run = update_goal_run_state(
        &state.db,
        goal_run_id,
        GoalRunUpdate {
            phase: Some(GoalRunPhase::Verification),
            status: Some(GoalRunStatus::Running),
            ..Default::default()
        },
    )?;
    let verification =
        runtime_commands::verify_runtime_impl(&state.db, &state.runtime_sessions, goal_run.project_id.clone()).await;
    match verification {
        Ok(summary) => {
            update_goal_run_state(
                &state.db,
                goal_run_id,
                GoalRunUpdate {
                    phase: Some(GoalRunPhase::Verification),
                    status: Some(GoalRunStatus::Completed),
                    blocker_reason: Some(None),
                    last_failure_summary: Some(None),
                    verification_summary: Some(Some(summary.clone())),
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
                Some(json!({ "summary": summary })),
            );
        }
        Err(error) => {
            let fingerprint = failure_fingerprint(GoalRunPhase::Verification, &error);
            update_goal_run_state(
                &state.db,
                goal_run_id,
                GoalRunUpdate {
                    phase: Some(GoalRunPhase::Verification),
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
                GoalRunPhase::Verification,
                GoalRunEventKind::Blocked,
                &error,
                Some(json!({ "fingerprint": fingerprint })),
            );
        }
    }

    info!(goal_run_id = %goal_run_id, "Goal run executor finished");
    Ok(())
}
