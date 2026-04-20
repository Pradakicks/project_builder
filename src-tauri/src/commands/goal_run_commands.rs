use crate::commands::goal_run_executor::spawn_goal_run_executor;
use crate::commands::runtime_commands::{self, RuntimeSessions};
#[cfg(test)]
use crate::db::AgentHistoryMetadata;
use crate::db::Database;
use crate::models::{
    parse_verification_result, GoalRun, GoalRunCodeEvidence, GoalRunDeliverySnapshot, GoalRunEvent,
    GoalRunEventKind, GoalRunPhase, GoalRunRetryState, GoalRunStatus, GoalRunUpdate, LiveActivity,
    PlanTask, TaskStatus, VerificationResult, WorkPlan,
};
use crate::AppState;
#[cfg(test)]
use std::collections::HashMap;
use tauri::{AppHandle, State};
use tokio_util::sync::CancellationToken;

pub(crate) fn create_goal_run_impl(
    db: &std::sync::Mutex<Database>,
    project_id: String,
    prompt: String,
) -> Result<GoalRun, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.create_goal_run(&project_id, &prompt)
}

pub(crate) fn update_goal_run_impl(
    db: &std::sync::Mutex<Database>,
    goal_run_id: String,
    updates: GoalRunUpdate,
) -> Result<GoalRun, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.update_goal_run(&goal_run_id, &updates)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn create_goal_run(
    state: State<'_, AppState>,
    project_id: String,
    prompt: String,
) -> Result<GoalRun, String> {
    create_goal_run_impl(&state.db, project_id, prompt)
}

#[tracing::instrument(skip(state, app_handle))]
#[tauri::command]
pub fn start_goal_run(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    project_id: String,
    prompt: String,
) -> Result<GoalRun, String> {
    let goal_run = create_goal_run_impl(&state.db, project_id, prompt)?;
    spawn_goal_run_executor(app_handle, goal_run.id.clone());
    Ok(goal_run)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn get_goal_run(state: State<'_, AppState>, goal_run_id: String) -> Result<GoalRun, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_goal_run(&goal_run_id)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn list_goal_runs(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<GoalRun>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_goal_runs(&project_id)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn update_goal_run(
    state: State<'_, AppState>,
    goal_run_id: String,
    updates: GoalRunUpdate,
) -> Result<GoalRun, String> {
    update_goal_run_impl(&state.db, goal_run_id, updates)
}

/// Re-enters the executor at the goal run's current stored phase.
/// Accepts Paused, Blocked, Interrupted, Failed, and Retrying rows.
/// Only failure metadata is cleared; `retry_count` and `phase` are preserved.
/// Pairs with `advance_goal_run`'s phase-ordinal skip guards.
#[tracing::instrument(skip(state, app_handle))]
#[tauri::command]
pub fn resume_goal_run(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    goal_run_id: String,
) -> Result<GoalRun, String> {
    let updated = update_goal_run_impl(
        &state.db,
        goal_run_id.clone(),
        GoalRunUpdate {
            stop_requested: Some(false),
            status: Some(GoalRunStatus::Running),
            blocker_reason: Some(None),
            last_failure_summary: Some(None),
            last_failure_fingerprint: Some(None),
            attention_required: Some(false),
            ..Default::default()
        },
    )?;
    append_event(
        &state.db,
        &goal_run_id,
        &updated.phase,
        GoalRunEventKind::Resumed,
        "Resumed by operator",
    );
    spawn_goal_run_executor(app_handle, goal_run_id);
    Ok(updated)
}

/// Re-enters the executor and requests one operator-forced repair attempt.
/// Unlike automatic retries, this path is allowed to run even when the retry
/// budget is exhausted, but the executor consumes the flag before attempting
/// repair so a single click cannot loop indefinitely.
#[tracing::instrument(skip(state, app_handle))]
#[tauri::command]
pub fn resume_goal_run_with_repair(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    goal_run_id: String,
) -> Result<GoalRun, String> {
    let updated = update_goal_run_impl(
        &state.db,
        goal_run_id.clone(),
        GoalRunUpdate {
            stop_requested: Some(false),
            status: Some(GoalRunStatus::Running),
            blocker_reason: Some(None),
            last_failure_summary: Some(None),
            last_failure_fingerprint: Some(None),
            attention_required: Some(false),
            operator_repair_requested: Some(true),
            ..Default::default()
        },
    )?;
    append_event(
        &state.db,
        &goal_run_id,
        &updated.phase,
        GoalRunEventKind::RepairRequested,
        "Repair requested by operator",
    );
    spawn_goal_run_executor(app_handle, goal_run_id);
    Ok(updated)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn stop_goal_run(state: State<'_, AppState>, goal_run_id: String) -> Result<GoalRun, String> {
    fire_cancel_token(&state, &goal_run_id);
    let run = update_goal_run_impl(
        &state.db,
        goal_run_id.clone(),
        GoalRunUpdate {
            stop_requested: Some(true),
            status: Some(GoalRunStatus::Blocked),
            blocker_reason: Some(Some("Stopped by operator".to_string())),
            ..Default::default()
        },
    )?;
    append_event(
        &state.db,
        &goal_run_id,
        &run.phase,
        GoalRunEventKind::Stopped,
        "Stopped by operator",
    );
    Ok(run)
}

/// Re-enter the Verification phase without invoking the CTO repair agent.
///
/// Intended for use after a blocked verification run where the operator has
/// made a manual fix and wants to re-run the acceptance suite to confirm the
/// fix holds. Force-sets phase=Verification, status=Running; preserves
/// retry_count (so this doesn't count against the MAX_REPAIR_RETRIES budget)
/// and keeps current_plan_id / current_piece_id intact.
#[tracing::instrument(skip(state))]
#[tauri::command]
pub async fn rerun_verification(
    state: State<'_, AppState>,
    goal_run_id: String,
) -> Result<GoalRun, String> {
    let updated = update_goal_run_impl(
        &state.db,
        goal_run_id.clone(),
        GoalRunUpdate {
            phase: Some(GoalRunPhase::Verification),
            status: Some(GoalRunStatus::Running),
            stop_requested: Some(false),
            blocker_reason: Some(None),
            last_failure_summary: Some(None),
            last_failure_fingerprint: Some(None),
            attention_required: Some(false),
            ..Default::default()
        },
    )?;
    append_event(
        &state.db,
        &goal_run_id,
        &GoalRunPhase::Verification,
        GoalRunEventKind::PhaseStarted,
        "Rerun verification requested by operator",
    );

    match runtime_commands::verify_runtime_impl(
        &state.db,
        &state.runtime_sessions,
        updated.project_id.clone(),
        CancellationToken::new(),
    )
    .await
    {
        Ok(result) => {
            let result_json =
                serde_json::to_string(&result).unwrap_or_else(|_| result.message.clone());
            if result.passed {
                let completed = update_goal_run_impl(
                    &state.db,
                    goal_run_id.clone(),
                    GoalRunUpdate {
                        phase: Some(GoalRunPhase::Verification),
                        status: Some(GoalRunStatus::Completed),
                        blocker_reason: Some(None),
                        last_failure_summary: Some(None),
                        last_failure_fingerprint: Some(None),
                        verification_summary: Some(Some(result_json)),
                        attention_required: Some(false),
                        ..Default::default()
                    },
                )?;
                append_event(
                    &state.db,
                    &goal_run_id,
                    &GoalRunPhase::Verification,
                    GoalRunEventKind::PhaseCompleted,
                    "Verification completed",
                );
                Ok(completed)
            } else {
                let blocked = update_goal_run_impl(
                    &state.db,
                    goal_run_id.clone(),
                    GoalRunUpdate {
                        phase: Some(GoalRunPhase::Verification),
                        status: Some(GoalRunStatus::Blocked),
                        blocker_reason: Some(Some(result.message.clone())),
                        last_failure_summary: Some(Some(result.message.clone())),
                        last_failure_fingerprint: Some(None),
                        verification_summary: Some(Some(result_json)),
                        attention_required: Some(true),
                        ..Default::default()
                    },
                )?;
                append_event(
                    &state.db,
                    &goal_run_id,
                    &GoalRunPhase::Verification,
                    GoalRunEventKind::Blocked,
                    &result.message,
                );
                Ok(blocked)
            }
        }
        Err(error) => {
            let blocked = update_goal_run_impl(
                &state.db,
                goal_run_id.clone(),
                GoalRunUpdate {
                    phase: Some(GoalRunPhase::Verification),
                    status: Some(GoalRunStatus::Blocked),
                    blocker_reason: Some(Some(error.clone())),
                    last_failure_summary: Some(Some(error.clone())),
                    last_failure_fingerprint: Some(None),
                    attention_required: Some(true),
                    ..Default::default()
                },
            )?;
            append_event(
                &state.db,
                &goal_run_id,
                &GoalRunPhase::Verification,
                GoalRunEventKind::Blocked,
                &error,
            );
            Ok(blocked)
        }
    }
}

/// Soft-pause: mark Paused, fire the cancellation token to unwind external CLI
/// children + the heartbeat, but preserve current piece/task so Resume picks up
/// the same phase. Does not clear retry counters or failure metadata.
#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn pause_goal_run(state: State<'_, AppState>, goal_run_id: String) -> Result<GoalRun, String> {
    fire_cancel_token(&state, &goal_run_id);
    let run = update_goal_run_impl(
        &state.db,
        goal_run_id.clone(),
        GoalRunUpdate {
            stop_requested: Some(true),
            status: Some(GoalRunStatus::Paused),
            blocker_reason: Some(Some("Paused by operator".to_string())),
            ..Default::default()
        },
    )?;
    append_event(
        &state.db,
        &goal_run_id,
        &run.phase,
        GoalRunEventKind::Paused,
        "Paused by operator",
    );
    Ok(run)
}

/// Hard cancel: fires the token and marks the run Failed. Unlike pause, this is
/// a terminal state and is not meant to be resumed.
#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn cancel_goal_run(state: State<'_, AppState>, goal_run_id: String) -> Result<GoalRun, String> {
    fire_cancel_token(&state, &goal_run_id);
    let run = update_goal_run_impl(
        &state.db,
        goal_run_id.clone(),
        GoalRunUpdate {
            stop_requested: Some(true),
            status: Some(GoalRunStatus::Failed),
            blocker_reason: Some(Some("Cancelled by operator".to_string())),
            ..Default::default()
        },
    )?;
    append_event(
        &state.db,
        &goal_run_id,
        &run.phase,
        GoalRunEventKind::CancelledMidPhase,
        "Cancelled by operator",
    );
    Ok(run)
}

fn fire_cancel_token(state: &State<'_, AppState>, goal_run_id: &str) {
    if let Ok(map) = state.goal_run_cancels.lock() {
        if let Some(token) = map.get(goal_run_id) {
            token.cancel();
        }
    }
}

fn append_event(
    db: &std::sync::Mutex<Database>,
    goal_run_id: &str,
    phase: &GoalRunPhase,
    kind: GoalRunEventKind,
    summary: &str,
) {
    if let Ok(db) = db.lock() {
        let _ = db.append_goal_run_event(goal_run_id, phase.clone(), kind, summary, None);
    }
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn get_goal_run_events(
    state: State<'_, AppState>,
    goal_run_id: String,
) -> Result<Vec<GoalRunEvent>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_goal_run_events(&goal_run_id)
}

/// List runs marked Interrupted — used by the startup banner to offer a
/// one-click resume of runs that were caught mid-execution by a crash/force-quit.
#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn list_interrupted_runs(state: State<'_, AppState>) -> Result<Vec<GoalRun>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_interrupted_runs()
}

fn latest_git_evidence(
    history: &[crate::db::AgentHistoryEntry],
) -> (Option<String>, Option<String>, Option<String>) {
    let mut git_branch = None;
    let mut git_commit_sha = None;
    let mut git_diff_stat = None;

    for entry in history {
        let metadata = &entry.metadata;
        if git_branch.is_none() {
            git_branch = metadata.git_branch.clone();
        }
        if git_commit_sha.is_none() {
            git_commit_sha = metadata.git_commit_sha.clone();
        }
        if git_diff_stat.is_none() {
            git_diff_stat = metadata.git_diff_stat.clone();
        }

        if git_branch.is_some() && git_commit_sha.is_some() && git_diff_stat.is_some() {
            break;
        }
    }

    (git_branch, git_commit_sha, git_diff_stat)
}

fn select_blocking_task(goal_run: &GoalRun, current_plan: &WorkPlan) -> Option<PlanTask> {
    if let Some(task_id) = goal_run.current_task_id.as_deref() {
        if let Some(task) = current_plan.tasks.iter().find(|task| task.id == task_id) {
            return Some(task.clone());
        }
    }

    if let Some(piece_id) = goal_run.current_piece_id.as_deref() {
        if let Some(task) = current_plan
            .tasks
            .iter()
            .find(|task| task.piece_id == piece_id)
        {
            return Some(task.clone());
        }
    }

    current_plan
        .tasks
        .iter()
        .find(|task| !matches!(task.status, TaskStatus::Complete))
        .cloned()
        .or_else(|| current_plan.tasks.first().cloned())
}

fn select_blocking_piece_id(
    goal_run: &GoalRun,
    blocking_task: Option<&PlanTask>,
) -> Option<String> {
    goal_run
        .current_piece_id
        .clone()
        .or_else(|| blocking_task.map(|task| task.piece_id.clone()))
}

fn tail_lines(contents: &str, limit: usize) -> Vec<String> {
    let lines: Vec<String> = contents
        .lines()
        .rev()
        .take(limit)
        .map(|line| line.to_string())
        .collect();
    lines.into_iter().rev().collect()
}

pub(crate) async fn build_goal_run_delivery_snapshot_impl(
    db: &std::sync::Mutex<Database>,
    runtime_sessions: &std::sync::Mutex<RuntimeSessions>,
    goal_run_id: &str,
) -> Result<GoalRunDeliverySnapshot, String> {
    let (goal_run, current_plan, recent_events) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let goal_run = db.get_goal_run(goal_run_id)?;
        let current_plan = match goal_run.current_plan_id.as_deref() {
            Some(plan_id) => db
                .get_work_plan(plan_id)
                .ok()
                .or_else(|| db.get_latest_work_plan(&goal_run.project_id).ok().flatten()),
            None => db.get_latest_work_plan(&goal_run.project_id)?,
        };
        let recent_events = db.list_goal_run_events(goal_run_id)?;
        (goal_run, current_plan, recent_events)
    };

    let runtime_status =
        runtime_commands::current_runtime_status(db, runtime_sessions, &goal_run.project_id)
            .await?;
    let runtime_status = if let Some(session) = runtime_status.session.clone() {
        if session.recent_logs.is_empty() {
            if let Some(log_path) = session.log_path.as_deref() {
                if let Ok(contents) = std::fs::read_to_string(log_path) {
                    let recent_logs = tail_lines(&contents, 120);
                    let mut runtime_status = runtime_status.clone();
                    if let Some(session) = runtime_status.session.as_mut() {
                        session.recent_logs = recent_logs;
                    }
                    runtime_status
                } else {
                    runtime_status
                }
            } else {
                runtime_status
            }
        } else {
            runtime_status
        }
    } else {
        runtime_status
    };

    let blocking_task = current_plan
        .as_ref()
        .and_then(|plan| select_blocking_task(&goal_run, plan));

    let blocking_piece = {
        let piece_id = select_blocking_piece_id(&goal_run, blocking_task.as_ref());
        match piece_id {
            Some(piece_id) => {
                let db = db.lock().map_err(|e| e.to_string())?;
                Some(db.get_piece(&piece_id)?)
            }
            None => None,
        }
    };

    let code_evidence = if let Some(piece) = blocking_piece.as_ref() {
        let (git_branch, git_commit_sha, git_diff_stat, generated_files_artifact) = {
            let db = db.lock().map_err(|e| e.to_string())?;
            let history = db.list_agent_history(&piece.id).unwrap_or_default();
            let (git_branch, git_commit_sha, git_diff_stat) = latest_git_evidence(&history);
            let generated_files_artifact = db.get_artifact_by_type(&piece.id, "generated_files")?;
            (
                git_branch,
                git_commit_sha,
                git_diff_stat,
                generated_files_artifact,
            )
        };

        Some(GoalRunCodeEvidence {
            piece_id: piece.id.clone(),
            piece_name: piece.name.clone(),
            git_branch,
            git_commit_sha,
            git_diff_stat,
            generated_files_artifact,
        })
    } else {
        None
    };

    let live_activity = if matches!(goal_run.phase, GoalRunPhase::Implementation)
        && matches!(
            goal_run.status,
            GoalRunStatus::Running | GoalRunStatus::Retrying
        ) {
        goal_run.current_piece_id.as_deref().and_then(|piece_id| {
            // Look up the piece name
            let piece = {
                let db = db.lock().ok()?;
                db.get_piece(piece_id).ok()?
            };

            // Find the task in the current plan
            let (task_id, task_title, current_index, total) = if let Some(plan) = &current_plan {
                let total = plan.tasks.len();
                let current_index = goal_run
                    .current_task_id
                    .as_deref()
                    .and_then(|tid| {
                        plan.tasks.iter().position(|t| t.id == tid).map(|i| i + 1)
                        // 1-based
                    })
                    .unwrap_or(0);
                let task = goal_run
                    .current_task_id
                    .as_deref()
                    .and_then(|tid| plan.tasks.iter().find(|t| t.id == tid));
                (
                    task.map(|t| t.id.clone()),
                    task.map(|t| t.title.clone()),
                    current_index,
                    total,
                )
            } else {
                (None, None, 0, 0)
            };

            Some(LiveActivity {
                piece_id: piece_id.to_string(),
                piece_name: piece.name.clone(),
                task_id,
                task_title,
                engine: piece.agent_config.execution_engine.clone(),
                current_index,
                total,
            })
        })
    } else {
        None
    };

    let verification_result: Option<VerificationResult> = goal_run
        .verification_summary
        .as_deref()
        .map(parse_verification_result);

    Ok(GoalRunDeliverySnapshot {
        goal_run: goal_run.clone(),
        current_plan,
        blocking_piece,
        blocking_task,
        retry_state: GoalRunRetryState {
            retry_count: goal_run.retry_count,
            stop_requested: goal_run.stop_requested,
            retry_backoff_until: goal_run.retry_backoff_until.clone(),
            last_failure_summary: goal_run.last_failure_summary.clone(),
            last_failure_fingerprint: goal_run.last_failure_fingerprint.clone(),
            attention_required: goal_run.attention_required,
            operator_repair_requested: goal_run.operator_repair_requested,
        },
        code_evidence,
        runtime_status: Some(runtime_status),
        recent_events,
        live_activity,
        verification_result,
    })
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub async fn get_goal_run_delivery_snapshot(
    state: State<'_, AppState>,
    goal_run_id: String,
) -> Result<GoalRunDeliverySnapshot, String> {
    build_goal_run_delivery_snapshot_impl(&state.db, &state.runtime_sessions, &goal_run_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::{GoalRunPhase, GoalRunStatus, GoalRunUpdate};
    use std::sync::Mutex;

    #[test]
    fn create_update_and_list_goal_runs_via_command_helpers() {
        let db_path = std::env::temp_dir().join(format!(
            "project-builder-goal-run-command-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&db_path).expect("create temp directory");
        let sqlite_path = db_path.join("data.db");
        let db = Database::new_at_path(&sqlite_path).expect("open db");
        let state = Mutex::new(db);

        let project = {
            let db = state.lock().expect("lock db");
            db.create_project("Command project", "Testing goal run commands")
                .expect("create project")
        };

        let created =
            create_goal_run_impl(&state, project.id.clone(), "Build a todo app".to_string())
                .expect("create goal run");
        assert_eq!(created.project_id, project.id);
        assert_eq!(created.phase, GoalRunPhase::PromptReceived);
        assert_eq!(created.status, GoalRunStatus::Running);

        let updated = update_goal_run_impl(
            &state,
            created.id.clone(),
            GoalRunUpdate {
                phase: Some(GoalRunPhase::Planning),
                status: Some(GoalRunStatus::Blocked),
                blocker_reason: Some(Some("Waiting on plan".to_string())),
                retry_count: Some(1),
                ..Default::default()
            },
        )
        .expect("update goal run");
        assert_eq!(updated.phase, GoalRunPhase::Planning);
        assert_eq!(updated.status, GoalRunStatus::Blocked);
        assert_eq!(updated.retry_count, 1);
        assert_eq!(updated.blocker_reason.as_deref(), Some("Waiting on plan"));

        let listed = {
            let db = state.lock().expect("lock db");
            db.list_goal_runs(&project.id).expect("list goal runs")
        };
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);

        let _ = std::fs::remove_dir_all(&db_path);
    }

    #[test]
    fn delivery_snapshot_includes_blocking_truth_and_evidence() {
        let db_path = std::env::temp_dir().join(format!(
            "project-builder-goal-run-snapshot-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&db_path).expect("create temp directory");
        let sqlite_path = db_path.join("data.db");
        let db = Database::new_at_path(&sqlite_path).expect("open db");
        let state = Mutex::new(db);
        let runtime_sessions: Mutex<RuntimeSessions> = Mutex::new(HashMap::new());
        let rt = tokio::runtime::Runtime::new().expect("runtime");

        rt.block_on(async {
            let project = {
                let db = state.lock().expect("lock db");
                db.create_project("Snapshot project", "Testing delivery snapshot")
                    .expect("create project")
            };

            let piece = {
                let db = state.lock().expect("lock db");
                db.create_piece(&project.id, None, "Implementation", 0.0, 0.0)
                    .expect("create piece")
            };

            let plan = {
                let db = state.lock().expect("lock db");
                db.create_work_plan(&project.id, "Build a todo app")
                    .expect("create work plan")
            };

            let task = PlanTask {
                id: "task-1".to_string(),
                piece_id: piece.id.clone(),
                piece_name: piece.name.clone(),
                title: "Implement todo list".to_string(),
                description: "Create the app".to_string(),
                priority: crate::models::TaskPriority::High,
                suggested_phase: "implementing".to_string(),
                dependencies: vec![],
                status: crate::models::TaskStatus::Pending,
                order: 0,
            };

            {
                let db = state.lock().expect("lock db");
                db.update_work_plan(
                    &plan.id,
                    &crate::models::WorkPlanUpdate {
                        status: Some(crate::models::PlanStatus::Approved),
                        tasks: Some(vec![task.clone()]),
                        ..Default::default()
                    },
                )
                .expect("update work plan");
            }

            let goal_run = {
                let db = state.lock().expect("lock db");
                db.create_goal_run(&project.id, "Create todo app")
                    .expect("create goal run")
            };

            {
                let db = state.lock().expect("lock db");
                db.update_goal_run(
                    &goal_run.id,
                    &GoalRunUpdate {
                        current_plan_id: Some(Some(plan.id.clone())),
                        current_piece_id: Some(Some(piece.id.clone())),
                        current_task_id: Some(Some(task.id.clone())),
                        retry_count: Some(2),
                        stop_requested: Some(false),
                        last_failure_summary: Some(Some("Need to retry".to_string())),
                        last_failure_fingerprint: Some(Some("implementation:retry".to_string())),
                        attention_required: Some(true),
                        ..Default::default()
                    },
                )
                .expect("update goal run");

                db.append_goal_run_event(
                    &goal_run.id,
                    GoalRunPhase::Implementation,
                    crate::models::GoalRunEventKind::Failed,
                    "Implementation failed",
                    Some("{\"fingerprint\":\"implementation:retry\"}"),
                )
                .expect("append event");

                db.upsert_artifact(
                    &piece.id,
                    "generated_files",
                    "Generated files",
                    "- src/main.rs\n- src/app.tsx",
                )
                .expect("upsert artifact");

                db.insert_agent_history(
                    &piece.id,
                    crate::models::AgentRole::Implementation,
                    "run",
                    "build",
                    "ok",
                    Some(&AgentHistoryMetadata {
                        git_branch: Some("feature/todo".to_string()),
                        git_commit_sha: Some("abc123".to_string()),
                        git_diff_stat: Some("1 file changed".to_string()),
                        ..Default::default()
                    }),
                    10,
                )
                .expect("insert agent history");

                let log_path = db_path.join("runtime.log");
                std::fs::write(&log_path, "server ready\n").expect("write runtime log");

                db.upsert_runtime_session(
                    &project.id,
                    Some(&goal_run.id),
                    &crate::models::ProjectRuntimeSession {
                        session_id: "runtime-1".to_string(),
                        status: crate::models::RuntimeSessionStatus::Running,
                        started_at: Some("2024-01-01T00:00:00Z".to_string()),
                        updated_at: "2024-01-01T00:00:01Z".to_string(),
                        ended_at: None,
                        url: Some("http://127.0.0.1:5173".to_string()),
                        port_hint: Some(5173),
                        log_path: Some(log_path.to_string_lossy().into_owned()),
                        recent_logs: vec![],
                        last_error: None,
                        exit_code: None,
                        pid: Some(1234),
                    },
                )
                .expect("upsert runtime session");
            }

            let snapshot =
                build_goal_run_delivery_snapshot_impl(&state, &runtime_sessions, &goal_run.id)
                    .await
                    .expect("build snapshot");

            assert_eq!(snapshot.goal_run.id, goal_run.id);
            assert_eq!(snapshot.retry_state.retry_count, 2);
            assert!(snapshot.retry_state.attention_required);
            assert_eq!(
                snapshot
                    .blocking_piece
                    .as_ref()
                    .map(|piece| piece.id.as_str()),
                Some(piece.id.as_str())
            );
            assert_eq!(
                snapshot.blocking_task.as_ref().map(|task| task.id.as_str()),
                Some(task.id.as_str())
            );
            assert_eq!(
                snapshot
                    .code_evidence
                    .as_ref()
                    .and_then(|e| e.git_branch.as_deref()),
                Some("feature/todo")
            );
            assert_eq!(
                snapshot
                    .code_evidence
                    .as_ref()
                    .and_then(|e| e.generated_files_artifact.as_ref())
                    .map(|artifact| artifact.artifact_type.as_str()),
                Some("generated_files")
            );
            assert_eq!(
                snapshot
                    .runtime_status
                    .as_ref()
                    .and_then(|status| status.session.as_ref())
                    .and_then(|session| session.recent_logs.first())
                    .map(String::as_str),
                Some("server ready")
            );
            assert_eq!(snapshot.recent_events.len(), 1);
        });

        let _ = std::fs::remove_dir_all(&db_path);
    }
}
