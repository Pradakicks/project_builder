use crate::agent::{
    build_agent_prompt, build_external_prompt, build_implementation_prompt, build_leader_prompt,
    build_review_prompt, build_role_external_prompt, build_testing_prompt, next_phase,
    parse_review_verdict, PieceContext, RolePriorOutputs,
};
use crate::models::{AgentRole, AgentState};
use crate::db::{Database, PieceUpdate};
use crate::llm::{self, LlmConfig, Message, TokenUsage};
use crate::models::*;
use crate::AppState;
use serde_json::json;
use std::collections::HashSet;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Load a piece and its context from the database.
fn load_piece_context(
    piece_id: &str,
    db: &Mutex<Database>,
) -> Result<(Piece, PieceContext, ProjectSettings), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let piece = db.get_piece(piece_id)?;

    debug!(piece_id, piece_name = %piece.name, project_id = %piece.project_id, "Loading piece context");

    let all_pieces = db.list_pieces(&piece.project_id)?;
    let connections = db.list_connections(&piece.project_id)?;
    let connected_ids: Vec<String> = connections
        .iter()
        .filter(|c| c.source_piece_id == piece_id || c.target_piece_id == piece_id)
        .map(|c| {
            if c.source_piece_id == piece_id {
                c.target_piece_id.clone()
            } else {
                c.source_piece_id.clone()
            }
        })
        .collect();
    let connected_pieces: Vec<_> = all_pieces
        .into_iter()
        .filter(|p| connected_ids.contains(&p.id))
        .collect();

    let parent = if let Some(ref parent_id) = piece.parent_id {
        db.get_piece(parent_id).ok()
    } else {
        None
    };

    // Load context from connected pieces: prefer context_summary (post-implementation),
    // fall back to design_doc (pre-implementation design decisions)
    let connected_summaries: Vec<(String, String)> = connected_pieces
        .iter()
        .filter_map(|cp| {
            let context_summary = db.get_artifact_by_type(&cp.id, "context_summary")
                .ok()
                .flatten();
            let design_doc = db.get_artifact_by_type(&cp.id, "design_doc")
                .ok()
                .flatten();
            context_summary.or(design_doc).map(|a| (cp.name.clone(), a.content))
        })
        .collect();

    let project = db.get_project(&piece.project_id).ok();
    let settings = project.map(|p| p.settings).unwrap_or_default();

    let context = PieceContext {
        connected_pieces,
        parent,
        connected_summaries,
    };

    Ok((piece, context, settings))
}

/// Result of an inner agent run (before done-event emission).
enum AgentResult {
    Builtin { usage: TokenUsage, output: String },
    External {
        exit_code: i32,
        success: bool,
        git_branch: Option<String>,
        git_commit_sha: Option<String>,
        git_diff_stat: Option<String>,
        validation: Option<crate::db::ValidationResult>,
    },
}

async fn run_validation_command<R: tauri::Runtime>(
    command: &str,
    working_dir: &str,
    piece_id: &str,
    app_handle: &AppHandle<R>,
) -> Result<crate::db::ValidationResult, String> {
    let mut child = if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", command]);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.args(["-lc", command]);
        cmd
    };

    child
        .current_dir(working_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = child
        .spawn()
        .map_err(|e| format!("Failed to start validation command: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture validation stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture validation stderr".to_string())?;

    let piece_id_stdout = piece_id.to_string();
    let app_stdout = app_handle.clone();
    let stdout_handle = tokio::spawn(async move {
        let mut output = String::new();
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            output.push_str(&line);
            output.push('\n');
            let _ = app_stdout.emit(
                "agent-output-chunk",
                json!({
                    "pieceId": piece_id_stdout,
                    "chunk": line + "\n",
                    "streamKind": "validation",
                    "done": false,
                }),
            );
        }
        output
    });

    let piece_id_stderr = piece_id.to_string();
    let app_stderr = app_handle.clone();
    let stderr_handle = tokio::spawn(async move {
        let mut output = String::new();
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            output.push_str(&line);
            output.push('\n');
            let _ = app_stderr.emit(
                "agent-output-chunk",
                json!({
                    "pieceId": piece_id_stderr,
                    "chunk": line + "\n",
                    "streamKind": "validation",
                    "done": false,
                }),
            );
        }
        output
    });

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Validation command failed to run: {e}"))?;
    let stdout_output = stdout_handle.await.unwrap_or_default();
    let stderr_output = stderr_handle.await.unwrap_or_default();

    let mut output = stdout_output;
    if !stderr_output.is_empty() {
        output.push_str(&stderr_output);
    }

    let exit_code = status.code().unwrap_or(-1);
    Ok(crate::db::ValidationResult {
        command: command.to_string(),
        passed: status.success(),
        exit_code,
        output,
    })
}

/// Run a piece's agent: dispatches to built-in LLM or external tool based on config.
/// Emits the unified done event with phase transition fields.
/// Optional `feedback` enables iterative mode: previous output + feedback are injected as context.
pub async fn run_piece_agent<R: tauri::Runtime>(
    piece_id: &str,
    feedback: Option<&str>,
    db: &Mutex<Database>,
    app_handle: &AppHandle<R>,
    cancel: Option<CancellationToken>,
) -> Result<TokenUsage, String> {
    let (piece, context, settings) = load_piece_context(piece_id, db)?;

    info!(piece_id, piece_name = %piece.name, engine = piece.agent_config.execution_engine.as_deref().or(settings.default_execution_engine.as_deref()).unwrap_or("built-in"), phase = ?piece.phase, "Starting piece agent run");

    let engine = piece
        .agent_config
        .execution_engine
        .as_deref()
        .or(settings.default_execution_engine.as_deref())
        .unwrap_or("built-in");

    // Transition the implementation-agent row to Working. Phase 1 only tracks
    // one role; the orchestrator in Phase 3 will drive Testing / Review too.
    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let _ = db_lock.upsert_agent(piece_id, crate::models::AgentRole::Implementation);
        let _ = db_lock.set_agent_state(
            piece_id,
            crate::models::AgentRole::Implementation,
            crate::models::AgentState::Working,
        );
    }

    let impl_prior = RolePriorOutputs::default();
    let result = match engine {
        "built-in" | "" => {
            run_builtin_agent(
                &piece,
                &context,
                &settings,
                piece_id,
                feedback,
                db,
                app_handle,
                cancel.clone(),
                AgentRole::Implementation,
                &impl_prior,
            )
            .await
        }
        name => {
            run_external_agent(
                &piece,
                &context,
                &settings,
                name,
                piece_id,
                feedback,
                db,
                app_handle,
                cancel.clone(),
                AgentRole::Implementation,
                &impl_prior,
            )
            .await
        }
    };

    // Determine if the run was successful
    let success = match &result {
        Ok(AgentResult::Builtin { .. }) => true,
        Ok(AgentResult::External { success, .. }) => *success,
        Err(_) => false,
    };

    // Flip the agent row back to Idle on success, or Error on failure.
    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let _ = db_lock.set_agent_state(
            piece_id,
            crate::models::AgentRole::Implementation,
            if success {
                crate::models::AgentState::Idle
            } else {
                crate::models::AgentState::Error
            },
        );
    }

    // Compute phase transition based on policy (only on success)
    let mut phase_proposal: Option<String> = None;
    let mut phase_changed: Option<String> = None;

    if success {
        if let Some(next) = next_phase(&piece.phase) {
            let next_str = serde_json::to_string(&next)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();

            match settings.phase_control {
                PhaseControlPolicy::FullyAutonomous => {
                    let update = PieceUpdate {
                        phase: Some(next),
                        ..Default::default()
                    };
                    let db_lock = db.lock().map_err(|e| e.to_string())?;
                    let _ = db_lock.update_piece(piece_id, &update);
                    phase_changed = Some(next_str);
                }
                PhaseControlPolicy::GatedAutoAdvance => {
                    phase_proposal = Some(next_str);
                }
                PhaseControlPolicy::Manual => {}
            }
        }
    }

    // Emit unified done event
    let mut done_payload = json!({
        "pieceId": piece_id,
        "chunk": "",
        "done": true,
    });

    match &result {
        Ok(AgentResult::Builtin { usage, .. }) => {
            done_payload["usage"] = json!({"input": usage.input, "output": usage.output});
            done_payload["success"] = json!(true);
        }
        Ok(AgentResult::External { exit_code, success, git_branch, git_commit_sha, git_diff_stat, validation }) => {
            done_payload["usage"] = json!({"input": 0, "output": 0});
            done_payload["exitCode"] = json!(exit_code);
            done_payload["success"] = json!(success);
            if let Some(ref branch) = git_branch {
                done_payload["gitBranch"] = json!(branch);
            }
            if let Some(ref sha) = git_commit_sha {
                done_payload["gitCommitSha"] = json!(sha);
            }
            if let Some(ref stat) = git_diff_stat {
                done_payload["gitDiffStat"] = json!(stat);
            }
            if let Some(ref validation_result) = validation {
                done_payload["validation"] = json!(validation_result);
            }
        }
        Err(e) => {
            done_payload["usage"] = json!({"input": 0, "output": 0});
            done_payload["exitCode"] = json!(-1);
            done_payload["success"] = json!(false);
            done_payload["error"] = json!(e);
        }
    }

    if let Some(ref proposal) = phase_proposal {
        done_payload["phaseProposal"] = json!(proposal);
    }
    if let Some(ref changed) = phase_changed {
        done_payload["phaseChanged"] = json!(changed);
    }
    if let Err(e) = app_handle.emit("agent-output-chunk", done_payload) {
        warn!(error = %e, "Failed to emit agent-output-chunk event");
    }

    // Phase 3 — Specialized agent chain. If the piece's agent_config opts in to
    // Testing and/or Review (non-empty active_agents containing those roles),
    // run them sequentially after a successful Implementation pass. Fails in
    // Testing/Review never abort the pipeline — they record structured
    // outcomes the operator + repair agent can act on. Piece agent_config
    // with an empty active_agents list (legacy default) stays on the
    // single-role flow for backward compat and token-cost safety.
    if success {
        let run_testing = piece
            .agent_config
            .active_agents
            .iter()
            .any(|r| r.eq_ignore_ascii_case("testing"));
        let run_review = piece
            .agent_config
            .active_agents
            .iter()
            .any(|r| r.eq_ignore_ascii_case("review"));

        if run_testing || run_review {
            let impl_output = match &result {
                Ok(AgentResult::Builtin { output, .. }) => output.clone(),
                Ok(AgentResult::External { .. }) => db
                    .lock()
                    .ok()
                    .and_then(|d| {
                        d.list_agent_history_by_role(piece_id, AgentRole::Implementation)
                            .ok()
                    })
                    .and_then(|h| h.into_iter().next().map(|e| e.output_text))
                    .unwrap_or_default(),
                Err(_) => String::new(),
            };
            let impl_diff = match &result {
                Ok(AgentResult::External {
                    git_diff_stat: Some(stat),
                    ..
                }) => Some(stat.clone()),
                _ => None,
            };

            let mut chain_prior = RolePriorOutputs {
                implementation_summary: Some(impl_output.clone()),
                implementation_diff: impl_diff.clone(),
                ..Default::default()
            };

            if run_testing {
                let prior_snapshot = chain_prior.clone();
                if let Err(e) = run_extra_role(
                    &piece,
                    &context,
                    &settings,
                    piece_id,
                    engine,
                    db,
                    app_handle,
                    cancel.clone(),
                    AgentRole::Testing,
                    &prior_snapshot,
                    &mut chain_prior,
                )
                .await
                {
                    warn!(piece_id, error = %e, "Testing role failed; continuing to Review if active");
                }
            }

            if run_review {
                let prior_snapshot = chain_prior.clone();
                if let Err(e) = run_extra_role(
                    &piece,
                    &context,
                    &settings,
                    piece_id,
                    engine,
                    db,
                    app_handle,
                    cancel.clone(),
                    AgentRole::Review,
                    &prior_snapshot,
                    &mut chain_prior,
                )
                .await
                {
                    warn!(piece_id, error = %e, "Review role failed");
                }
            }
        }
    }

    // Fire-and-forget context summarization on success
    if success {
        let agent_output = match &result {
            Ok(AgentResult::Builtin { output, .. }) => output.clone(),
            Ok(AgentResult::External { .. }) => {
                // External output is in agent_history; load it
                db.lock()
                    .ok()
                    .and_then(|db| db.list_agent_history(piece_id).ok())
                    .and_then(|history| history.into_iter().next().map(|h| h.output_text))
                    .unwrap_or_default()
            }
            Err(_) => String::new(),
        };

        let git_info: Option<(String, String)> = match &result {
            Ok(AgentResult::External { git_branch: Some(branch), .. }) => {
                settings.working_directory.clone().map(|wd| (wd, branch.clone()))
            }
            _ => None,
        };

        let piece_id_owned = piece_id.to_string();
        let piece_name = piece.name.clone();
        let settings_clone = settings.clone();
        let app = app_handle.clone();

        if let Some((working_dir, branch)) = git_info.as_ref().map(|(wd, b)| (wd.as_str(), b.as_str()))
        {
            if let Err(e) =
                store_generated_files_artifact(&piece_id_owned, working_dir, branch, db, &app).await
            {
                warn!(
                    piece_id = %piece_id_owned,
                    error = %e,
                    "Generated files artifact update failed"
                );
            }
        }

        let piece_phase = piece.phase.clone();
        let piece_clone = piece.clone();

        tokio::spawn(async move {
            if let Err(e) = generate_context_summary(
                &piece_id_owned,
                &piece_name,
                &agent_output,
                git_info.as_ref().map(|(wd, b)| (wd.as_str(), b.as_str())),
                &settings_clone,
                &app,
            )
            .await
            {
                warn!(piece_id = %piece_id_owned, error = %e, "Context summary generation failed");
            }

            // Design-phase runs also generate a design doc for connected pieces to reference
            if piece_phase == Phase::Design {
                if let Err(e) = generate_design_doc(
                    &piece_id_owned,
                    &piece_clone,
                    &agent_output,
                    &settings_clone,
                    &app,
                )
                .await
                {
                    warn!(piece_id = %piece_id_owned, error = %e, "Design doc generation failed");
                }
            }
        });
    }

    info!(piece_id, success, "Piece agent run complete");

    // Convert result to TokenUsage for return
    match result {
        Ok(AgentResult::Builtin { usage, .. }) => Ok(usage),
        Ok(AgentResult::External { .. }) => Ok(TokenUsage { input: 0, output: 0 }),
        Err(e) => Err(e),
    }
}

fn update_plan_task_status_in_db(
    db: &Mutex<Database>,
    plan_id: &str,
    task_id: &str,
    status: TaskStatus,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let plan = db.get_work_plan(plan_id)?;
    let mut tasks = plan.tasks;
    if let Some(task) = tasks.iter_mut().find(|task| task.id == task_id) {
        task.status = status;
    } else {
        return Err(format!("Task '{}' not found in plan", task_id));
    }
    db.update_work_plan(
        plan_id,
        &WorkPlanUpdate {
            tasks: Some(tasks),
            ..Default::default()
        },
    )?;
    Ok(())
}

/// Run one of the follow-on roles (Testing or Review) after Implementation has
/// succeeded. Handles: upsert_agent + state transitions, dispatch to built-in
/// or external engine with the role-specific prompt, and role-specific
/// post-processing (tests artifact for Testing; review artifact + verdict +
/// reviewStatus flip for Review). Returns Err if the LLM/CLI call itself
/// errors; tests-failing or review-rejecting do NOT produce Err — they record
/// their outcome so the operator / repair agent can act.
async fn run_extra_role<R: tauri::Runtime>(
    piece: &Piece,
    context: &PieceContext,
    settings: &ProjectSettings,
    piece_id: &str,
    engine: &str,
    db: &Mutex<Database>,
    app_handle: &AppHandle<R>,
    cancel: Option<CancellationToken>,
    role: AgentRole,
    prior_in: &RolePriorOutputs,
    prior_out: &mut RolePriorOutputs,
) -> Result<(), String> {
    info!(piece_id, role = role.as_str(), engine, "Starting extra role");

    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let _ = db_lock.upsert_agent(piece_id, role);
        let _ = db_lock.set_agent_state(piece_id, role, AgentState::Working);
    }

    let role_result = match engine {
        "built-in" | "" => {
            run_builtin_agent(
                piece, context, settings, piece_id, None, db, app_handle, cancel, role, prior_in,
            )
            .await
        }
        name => {
            run_external_agent(
                piece, context, settings, name, piece_id, None, db, app_handle, cancel, role,
                prior_in,
            )
            .await
        }
    };

    let role_succeeded = matches!(
        &role_result,
        Ok(AgentResult::Builtin { .. }) | Ok(AgentResult::External { success: true, .. })
    );

    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let _ = db_lock.set_agent_state(
            piece_id,
            role,
            if role_succeeded {
                AgentState::Idle
            } else {
                AgentState::Error
            },
        );
    }

    // Extract role output from the result or from history.
    let role_output = match &role_result {
        Ok(AgentResult::Builtin { output, .. }) => output.clone(),
        Ok(AgentResult::External { .. }) => db
            .lock()
            .ok()
            .and_then(|d| d.list_agent_history_by_role(piece_id, role).ok())
            .and_then(|h| h.into_iter().next().map(|e| e.output_text))
            .unwrap_or_default(),
        Err(_) => String::new(),
    };

    // Role-specific post-processing.
    match role {
        AgentRole::Testing => {
            // Capture the test output for Review's prompt; flag pass/fail if
            // the validation command ran.
            prior_out.tests_summary = Some(role_output.clone());
            prior_out.tests_output_tail = if role_output.chars().count() > 800 {
                Some(
                    role_output
                        .chars()
                        .skip(role_output.chars().count().saturating_sub(800))
                        .collect(),
                )
            } else {
                Some(role_output.clone())
            };
            if let Ok(AgentResult::External {
                validation: Some(v),
                ..
            }) = &role_result
            {
                prior_out.tests_passed = Some(v.passed);
            }
            // Record a lightweight "tests" artifact for the UI timeline.
            if role_succeeded {
                if let Ok(db_lock) = db.lock() {
                    let _ = db_lock.upsert_artifact(
                        piece_id,
                        "tests",
                        "Tests written",
                        &role_output,
                    );
                }
            }
        }
        AgentRole::Review => {
            let (approved, reason) = parse_review_verdict(&role_output);

            // Store the full review prose so operators can read it inline.
            if let Ok(db_lock) = db.lock() {
                let _ = db_lock.upsert_artifact(
                    piece_id,
                    "review",
                    if approved { "Review: APPROVED" } else { "Review: REJECTED" },
                    &role_output,
                );

                // Flip the implementation's generated_files artifact's
                // reviewStatus. Legacy pieces without such an artifact get a
                // silent no-op via the helper.
                let new_status = if approved {
                    crate::models::artifact::ReviewStatus::Approved
                } else {
                    crate::models::artifact::ReviewStatus::Rejected
                };
                let _ = db_lock.set_artifact_review_status(
                    piece_id,
                    "generated_files",
                    new_status,
                );
            }

            if !approved {
                warn!(
                    piece_id,
                    reason = %reason,
                    "Review agent rejected the implementation"
                );
                // Surface via the running event channel so the UI's ActivityFeed
                // / ProjectStatusBar pick it up immediately without a poll.
                let _ = app_handle.emit(
                    "agent-output-chunk",
                    json!({
                        "pieceId": piece_id,
                        "chunk": "",
                        "done": true,
                        "reviewRejected": true,
                        "reviewReason": reason,
                    }),
                );
            }
        }
        _ => {}
    }

    role_result.map(|_| ())
}

pub async fn run_all_plan_tasks<R: tauri::Runtime>(
    plan_id: &str,
    goal_run_id: Option<&str>,
    db: &Mutex<Database>,
    running_pieces: &Mutex<HashSet<String>>,
    app_handle: &AppHandle<R>,
    cancel: Option<CancellationToken>,
) -> Result<(), String> {
    let plan = {
        let db = db.lock().map_err(|e| e.to_string())?;
        db.get_work_plan(plan_id)?
    };

    if !matches!(plan.status, PlanStatus::Approved) {
        return Err("Plan must be approved before running all tasks.".to_string());
    }

    let tasks: Vec<PlanTask> = plan
        .tasks
        .iter()
        .filter(|task| matches!(task.status, TaskStatus::Pending) && !task.piece_id.is_empty())
        .cloned()
        .collect();

    if tasks.is_empty() {
        return Ok(());
    }

    let total_tasks = tasks.len();
    let mut skipped_tasks = 0usize;
    let mut current_index = 0usize;

    for task in tasks {
        // Stop between tasks if the executor is being paused/cancelled.
        if let Some(token) = cancel.as_ref() {
            if token.is_cancelled() {
                return Err("cancelled".to_string());
            }
        }

        // Validate the piece exists before attempting to run — skip tasks that
        // reference non-existent or hallucinated piece IDs.
        let piece_exists = {
            let db = db.lock().map_err(|e| e.to_string())?;
            db.get_piece(&task.piece_id).is_ok()
        };
        if !piece_exists {
            warn!(
                plan_id,
                task_title = %task.title,
                piece_id = %task.piece_id,
                "Skipping task: piece not found"
            );
            skipped_tasks += 1;
            continue;
        }

        current_index += 1;

        // Look up execution engine from the piece
        let engine_name = {
            let db = db.lock().map_err(|e| e.to_string())?;
            let piece = db.get_piece(&task.piece_id).ok();
            piece
                .as_ref()
                .and_then(|p| p.agent_config.execution_engine.as_deref())
                .unwrap_or("built-in")
                .to_string()
        };

        // Write current piece/task to goal run DB row (only when orchestrated by goal run)
        if let Some(gid) = goal_run_id {
            let db = db.lock().map_err(|e| e.to_string())?;
            let _ = db.update_goal_run(gid, &GoalRunUpdate {
                current_piece_id: Some(Some(task.piece_id.clone())),
                current_task_id: Some(Some(task.id.clone())),
                ..Default::default()
            });
        }

        // Emit implementation-progress event (started)
        if let Some(gid) = goal_run_id {
            let _ = app_handle.emit("implementation-progress", serde_json::json!({
                "goalRunId": gid,
                "current": current_index,
                "total": total_tasks,
                "pieceId": task.piece_id,
                "pieceName": task.piece_name,
                "taskId": task.id,
                "taskTitle": task.title,
                "engine": engine_name,
                "status": "started",
            }));
        }

        update_plan_task_status_in_db(db, plan_id, &task.id, TaskStatus::InProgress)?;

        if !task.suggested_phase.is_empty() {
            let phase = match task.suggested_phase.as_str() {
                "design" => Some(Phase::Design),
                "review" => Some(Phase::Review),
                "approved" => Some(Phase::Approved),
                "implementing" => Some(Phase::Implementing),
                _ => None,
            };
            if let Some(phase) = phase {
                let db = db.lock().map_err(|e| e.to_string())?;
                db.update_piece(
                    &task.piece_id,
                    &PieceUpdate {
                        phase: Some(phase),
                        ..Default::default()
                    },
                ).map_err(|e| format!("Failed to update phase for task '{}': {}", task.title, e))?;
            }
        }

        {
            let mut running = running_pieces.lock().map_err(|e| e.to_string())?;
            if !running.insert(task.piece_id.clone()) {
                update_plan_task_status_in_db(db, plan_id, &task.id, TaskStatus::Pending)?;
                return Err(format!(
                    "An agent is already running for piece '{}'.",
                    task.piece_name
                ));
            }
        }

        let result = run_piece_agent(&task.piece_id, None, db, app_handle, cancel.clone()).await;

        {
            let mut running = running_pieces.lock().map_err(|e| e.to_string())?;
            running.remove(&task.piece_id);
        }

        match result {
            Ok(_) => {
                update_plan_task_status_in_db(db, plan_id, &task.id, TaskStatus::Complete)?;
                if let Some(gid) = goal_run_id {
                    let _ = app_handle.emit("implementation-progress", serde_json::json!({
                        "goalRunId": gid,
                        "current": current_index,
                        "total": total_tasks,
                        "pieceId": task.piece_id,
                        "pieceName": task.piece_name,
                        "taskId": task.id,
                        "taskTitle": task.title,
                        "engine": engine_name,
                        "status": "completed",
                    }));
                    // Clear current piece/task
                    let db = db.lock().map_err(|e| e.to_string())?;
                    let _ = db.update_goal_run(gid, &GoalRunUpdate {
                        current_piece_id: Some(None),
                        current_task_id: Some(None),
                        ..Default::default()
                    });
                }
            }
            Err(error) => {
                update_plan_task_status_in_db(db, plan_id, &task.id, TaskStatus::Pending)?;
                if let Some(gid) = goal_run_id {
                    let _ = app_handle.emit("implementation-progress", serde_json::json!({
                        "goalRunId": gid,
                        "current": current_index,
                        "total": total_tasks,
                        "pieceId": task.piece_id,
                        "pieceName": task.piece_name,
                        "taskId": task.id,
                        "taskTitle": task.title,
                        "engine": engine_name,
                        "status": "failed",
                    }));
                }
                return Err(format!("Task '{}' failed: {}", task.title, error));
            }
        }
    }

    // If every task was skipped (all piece IDs were hallucinated or invalid),
    // fail loudly so the autopilot surfaces a blocked state instead of advancing
    // to runtime detection on an empty working directory.
    if skipped_tasks > 0 && skipped_tasks == total_tasks {
        return Err(format!(
            "All {total_tasks} plan tasks were skipped because their pieces do not exist. \
             The plan appears to reference hallucinated piece IDs. Ensure pieces are created \
             before running the plan."
        ));
    }

    let merge_summary = super::merge::merge_plan_branches(plan_id, db, app_handle).await?;
    if merge_summary.conflict.is_none() {
      let _ = super::merge::run_integration_review(plan_id, db, app_handle).await?;
    }

    Ok(())
}

/// Run the built-in LLM agent. Streams chunks but does NOT emit the done event
/// (that's handled by run_piece_agent).
async fn run_builtin_agent<R: tauri::Runtime>(
    piece: &Piece,
    context: &PieceContext,
    settings: &ProjectSettings,
    piece_id: &str,
    feedback: Option<&str>,
    db: &Mutex<Database>,
    app_handle: &AppHandle<R>,
    cancel: Option<CancellationToken>,
    role: AgentRole,
    prior: &RolePriorOutputs,
) -> Result<AgentResult, String> {
    let max_tokens = piece.agent_config.token_budget.unwrap_or(4096) as u32;

    let (provider_name, api_key, model, base_url) =
        if let Some(ref piece_provider) = piece.agent_config.provider {
            let key = resolve_api_key(piece_provider);
            let mdl = piece
                .agent_config
                .model
                .clone()
                .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
            let url = settings
                .llm_configs
                .iter()
                .find(|c| c.provider.to_lowercase() == piece_provider.to_lowercase())
                .and_then(|c| c.base_url.clone());
            (piece_provider.clone(), key, mdl, url)
        } else {
            let (prov, key, mut mdl, url) = resolve_llm_config(settings);
            if let Some(ref piece_model) = piece.agent_config.model {
                mdl = piece_model.clone();
            }
            (prov, key, mdl, url)
        };

    debug!(piece_id, provider = %provider_name, model = %model, max_tokens, feedback = feedback.is_some(), "Built-in agent config resolved");

    if api_key.is_empty() {
        return Err(format!(
            "No API key found for provider '{provider_name}'. Add it in Settings or set the environment variable."
        ));
    }

    // Role-aware prompt: Implementation keeps today's exact shape; Testing and
    // Review layer role-specific directives + prior-role context on top.
    let mut messages = match role {
        AgentRole::Testing => build_testing_prompt(piece, context, prior),
        AgentRole::Review => build_review_prompt(piece, context, prior),
        _ => build_implementation_prompt(piece, context),
    };

    // Iterative mode: inject previous output + feedback as conversation context
    if let Some(fb) = feedback {
        let prev_output = db
            .lock()
            .ok()
            .and_then(|db| db.list_agent_history(piece_id).ok())
            .and_then(|h| h.into_iter().next().map(|e| e.output_text))
            .unwrap_or_default();
        if !prev_output.is_empty() {
            messages.push(Message {
                role: "assistant".into(),
                content: prev_output,
            });
        }
        messages.push(Message {
            role: "user".into(),
            content: fb.to_string(),
        });
    }

    let provider = llm::create_provider(&provider_name);
    let config = LlmConfig {
        api_key,
        model,
        base_url,
        max_tokens,
    };

    let (tx, mut rx) = mpsc::channel::<String>(256);
    let piece_id_owned = piece_id.to_string();
    let app = app_handle.clone();
    let full_output = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
    let full_output_writer = full_output.clone();

    let stream_handle = tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            full_output_writer.lock().await.push_str(&chunk);
            let _ = app.emit(
                "agent-output-chunk",
                json!({
                    "pieceId": piece_id_owned,
                    "chunk": chunk,
                    "done": false,
                }),
            );
        }
    });

    let usage_result = if let Some(ref token) = cancel {
        tokio::select! {
            result = provider.chat_stream(&messages, &config, tx) => result,
            _ = token.cancelled() => Err("cancelled".to_string()),
        }
    } else {
        provider.chat_stream(&messages, &config, tx).await
    };
    // Dropping tx (via select! or completion) closes rx; stream_handle exits naturally.
    let _ = stream_handle.await;
    let usage = usage_result?;

    debug!(piece_id, input_tokens = usage.input, output_tokens = usage.output, "Built-in agent stream complete");

    let output_text = full_output.lock().await.clone();

    {
        let db = db.lock().map_err(|e| e.to_string())?;
        let metadata = crate::db::AgentHistoryMetadata {
            usage: Some(usage.clone()),
            success: Some(true),
            ..Default::default()
        };
        if let Err(e) = db.insert_agent_history(
            piece_id,
            role,
            "run",
            &piece.agent_prompt,
            &output_text,
            Some(&metadata),
            (usage.input + usage.output) as i64,
        ) {
            warn!(piece_id, error = %e, "Failed to insert agent history");
        }
    }

    Ok(AgentResult::Builtin { usage, output: output_text })
}

/// Emit a git-related info line through the agent output stream.
fn emit_git_info<R: tauri::Runtime>(app_handle: &AppHandle<R>, piece_id: &str, message: &str) {
    let _ = app_handle.emit(
        "agent-output-chunk",
        json!({
            "pieceId": piece_id,
            "chunk": format!("[git] {message}\n"),
            "done": false,
        }),
    );
}

/// Run an external tool (Claude Code, Codex, etc.) with git branch/commit lifecycle.
/// Streams chunks but does NOT emit the done event (that's handled by run_piece_agent).
async fn run_external_agent<R: tauri::Runtime>(
    piece: &Piece,
    context: &PieceContext,
    settings: &ProjectSettings,
    engine_name: &str,
    piece_id: &str,
    feedback: Option<&str>,
    db: &Mutex<Database>,
    app_handle: &AppHandle<R>,
    cancel: Option<CancellationToken>,
    role: AgentRole,
    prior: &RolePriorOutputs,
) -> Result<AgentResult, String> {
    use super::git_ops;

    let working_dir = settings
        .working_directory
        .as_deref()
        .ok_or_else(|| {
            "No working directory set. Configure one in Project Settings before using external tools."
                .to_string()
        })?;

    info!(piece_id, engine = engine_name, working_dir, timeout = piece.agent_config.timeout.unwrap_or(300), "Starting external agent run");

    // Role-aware external prompt.
    let (system_prompt, user_prompt_base) = match role {
        AgentRole::Implementation => build_external_prompt(piece, context),
        _ => build_role_external_prompt(piece, context, role, prior),
    };
    let mut user_prompt = user_prompt_base;

    // Iterative mode: append previous output + feedback to the prompt
    if let Some(fb) = feedback {
        let prev_output = db
            .lock()
            .ok()
            .and_then(|db| db.list_agent_history(piece_id).ok())
            .and_then(|h| h.into_iter().next().map(|e| e.output_text))
            .unwrap_or_default();
        if !prev_output.is_empty() {
            user_prompt.push_str(&format!("\n\n--- Previous output ---\n{prev_output}"));
        }
        user_prompt.push_str(&format!("\n\n--- Your feedback ---\n{fb}"));
    }

    let timeout_secs = piece.agent_config.timeout.unwrap_or(300);

    // For Codex, pass the OpenAI API key from our keyring
    let env_vars = if engine_name == "codex" {
        let key = resolve_api_key("openai");
        if key.is_empty() {
            return Err(
                "No OpenAI API key found for Codex. Add it in Settings.".to_string()
            );
        }
        vec![("OPENAI_API_KEY".to_string(), key)]
    } else {
        vec![]
    };

    // ── Git: pre-execution ──────────────────────────────────
    let branch_name = git_ops::slugify_branch_name(&piece.name);
    let mut git_branch: Option<String> = None;
    let mut _before_sha: Option<String> = None;

    // Save HEAD SHA before any changes
    match git_ops::get_head_sha(working_dir).await {
        Ok(sha) => _before_sha = Some(sha),
        Err(e) => emit_git_info(app_handle, piece_id, &format!("Warning: {e}")),
    }

    // WIP-commit any dirty state so we don't lose uncommitted work
    match git_ops::has_uncommitted_changes(working_dir).await {
        Ok(true) => {
            emit_git_info(app_handle, piece_id, "Saving uncommitted changes...");
            if let Err(e) = git_ops::stage_and_commit(working_dir, "WIP: save uncommitted changes before agent run").await {
                emit_git_info(app_handle, piece_id, &format!("Warning: could not save changes: {e}"));
            }
        }
        Ok(false) => {}
        Err(e) => emit_git_info(app_handle, piece_id, &format!("Warning: {e}")),
    }

    // Switch to piece branch
    match git_ops::ensure_branch(working_dir, &branch_name).await {
        Ok(()) => {
            emit_git_info(app_handle, piece_id, &format!("On branch {branch_name}"));
            git_branch = Some(branch_name.clone());
        }
        Err(e) => {
            emit_git_info(app_handle, piece_id, &format!("Warning: could not switch to branch: {e}"));
        }
    }

    // Record HEAD after branch switch (for diff stat later)
    let branch_head_sha = git_ops::get_head_sha(working_dir).await.ok();

    // ── Run external tool (unchanged) ───────────────────────
    let run_config = super::external::ExternalRunConfig {
        system_prompt,
        user_prompt: user_prompt.clone(),
        working_dir: working_dir.to_string(),
        timeout_secs,
        env_vars,
    };

    let (tx, mut rx) = mpsc::channel::<String>(256);
    let piece_id_owned = piece_id.to_string();
    let app = app_handle.clone();

    let stream_handle = tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            let _ = app.emit(
                "agent-output-chunk",
                json!({
                    "pieceId": piece_id_owned,
                    "chunk": chunk,
                    "done": false,
                }),
            );
        }
    });

    let result = super::external::run_external(engine_name, &run_config, tx, cancel).await;
    let _ = stream_handle.await;

    // ── Git: post-execution ─────────────────────────────────
    match result {
        Ok(run_result) => {
            let exit_code = run_result.exit_code;
            let mut git_commit_sha: Option<String> = None;
            let mut git_diff_stat: Option<String> = None;
            let mut validation: Option<crate::db::ValidationResult> = None;

            if exit_code == 0 {
                // Auto-commit changes on success
                let phase_str = format!("{:?}", piece.phase).to_lowercase();
                let commit_msg = format!(
                    "{}: {} phase agent run\n\nPiece: {}\nEngine: {}",
                    branch_name, phase_str, piece.name, engine_name
                );
                match git_ops::stage_and_commit(working_dir, &commit_msg).await {
                    Ok(Some(sha)) => {
                        emit_git_info(app_handle, piece_id, &format!("Committed: {sha}"));
                        git_commit_sha = Some(sha);
                    }
                    Ok(None) => {
                        emit_git_info(app_handle, piece_id, "No changes to commit");
                    }
                    Err(e) => {
                        emit_git_info(app_handle, piece_id, &format!("Warning: commit failed: {e}"));
                    }
                }

                // Get diff stat since branch start
                if let Some(ref base) = branch_head_sha {
                    if let Ok(stat) = git_ops::diff_stat(working_dir, base).await {
                        if !stat.is_empty() {
                            git_diff_stat = Some(stat);
                        }
                    }
                }

                if piece.phase == Phase::Implementing {
                    if let Some(command) = settings.post_run_validation_command.as_deref() {
                        let trimmed = command.trim();
                        if !trimmed.is_empty() {
                            emit_git_info(app_handle, piece_id, &format!("Running validation: {trimmed}"));
                            validation = Some(match run_validation_command(trimmed, working_dir, piece_id, app_handle).await {
                                Ok(result) => result,
                                Err(error) => crate::db::ValidationResult {
                                    command: trimmed.to_string(),
                                    passed: false,
                                    exit_code: -1,
                                    output: error,
                                },
                            });
                        }
                    }
                }
            }

            let success = exit_code == 0
                && validation
                    .as_ref()
                    .map(|result| result.passed)
                    .unwrap_or(true);

            let metadata = crate::db::AgentHistoryMetadata {
                usage: Some(TokenUsage::default()),
                success: Some(success),
                exit_code: Some(exit_code),
                git_branch: git_branch.clone(),
                git_commit_sha: git_commit_sha.clone(),
                git_diff_stat: git_diff_stat.clone(),
                validation: validation.clone(),
                ..Default::default()
            };

            {
                let db = db.lock().map_err(|e| e.to_string())?;
                if let Err(e) = db.insert_agent_history(
                    piece_id,
                    role,
                    "external-run",
                    &user_prompt,
                    &run_result.output,
                    Some(&metadata),
                    0,
                ) {
                    warn!(piece_id, error = %e, "Failed to insert external agent history");
                }
            }

            Ok(AgentResult::External {
                exit_code,
                success,
                git_branch,
                git_commit_sha,
                git_diff_stat,
                validation,
            })
        }
        Err(err) => Err(err),
    }
}

async fn store_generated_files_artifact<R: tauri::Runtime>(
    piece_id: &str,
    working_dir: &str,
    branch: &str,
    db: &Mutex<Database>,
    _app_handle: &AppHandle<R>,
) -> Result<(), String> {
    let files = super::git_ops::list_branch_files(working_dir, branch).await?;
    let trimmed = files.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let file_count = trimmed.lines().count();
    let title = format!("Generated files on {branch}");
    let content = format!(
        "Branch: {branch}\nFiles: {file_count}\n\n{}",
        trimmed
    );

    let db = db.lock().map_err(|e| e.to_string())?;
    db.upsert_artifact(piece_id, "generated_files", &title, &content)?;
    Ok(())
}

/// Generate a concise context summary from an agent's output and store as an artifact.
/// Called fire-and-forget after successful agent runs.
async fn generate_context_summary<R: tauri::Runtime>(
    piece_id: &str,
    piece_name: &str,
    agent_output: &str,
    git_info: Option<(&str, &str)>, // (working_dir, branch_name)
    settings: &ProjectSettings,
    app_handle: &AppHandle<R>,
) -> Result<(), String> {
    debug!(piece_id, piece_name, output_len = agent_output.len(), has_git = git_info.is_some(), "Generating context summary");

    if agent_output.is_empty() {
        return Ok(());
    }

    let (provider_name, api_key, model, base_url) = resolve_llm_config(settings);
    if api_key.is_empty() {
        return Err("No API key available for summarization".to_string());
    }

    // Optionally get file listing for external tool runs
    let file_listing = if let Some((working_dir, branch)) = git_info {
        super::git_ops::list_branch_files(working_dir, branch)
            .await
            .ok()
    } else {
        None
    };

    let system_msg = "You are a technical summarizer. Given the output of an AI agent run on a software component, produce a concise context summary that other agents working on connected components will use.\n\nFocus on:\n- What was produced (files, APIs, data structures, schemas)\n- Key interfaces: endpoint paths, function signatures, event names, message formats\n- Important decisions or constraints that affect other components\n- File structure (if applicable)\n\nRules:\n- Be concise: aim for 200-400 words\n- Use bullet points, not prose\n- Include specific names (endpoint paths, type names, file paths) — not vague descriptions\n- Omit internal implementation details that don't affect other components";

    let mut user_content = format!("Component: \"{piece_name}\"\n\nAgent output:\n{agent_output}");

    if let Some(ref files) = file_listing {
        if !files.is_empty() {
            user_content.push_str(&format!("\n\nFiles on branch:\n{files}"));
        }
    }

    // Truncate if extremely long (keep last 8000 chars which are most relevant)
    if user_content.len() > 10000 {
        let truncated = &user_content[user_content.len() - 8000..];
        user_content = format!("(output truncated — showing last portion)\n\n{truncated}");
    }

    let messages = vec![
        Message {
            role: "system".to_string(),
            content: system_msg.to_string(),
        },
        Message {
            role: "user".to_string(),
            content: user_content,
        },
    ];

    let provider = llm::create_provider(&provider_name);
    let config = LlmConfig {
        api_key,
        model,
        base_url,
        max_tokens: 1024,
    };

    let response = provider.chat(&messages, &config).await?;
    let summary = response.content;

    // Store as artifact
    let state = app_handle.state::<AppState>();
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let title = format!("{piece_name} — Context Summary");
    db.upsert_artifact(piece_id, "context_summary", &title, &summary)?;

    info!(piece_id, summary_len = summary.len(), "Context summary stored");

    Ok(())
}

/// Generate a design document from a Design-phase agent's output and store as an artifact.
/// Called fire-and-forget after successful Design-phase runs.
async fn generate_design_doc<R: tauri::Runtime>(
    piece_id: &str,
    piece: &Piece,
    agent_output: &str,
    settings: &ProjectSettings,
    app_handle: &AppHandle<R>,
) -> Result<(), String> {
    debug!(piece_id, piece_name = %piece.name, "Generating design doc");

    if agent_output.is_empty() {
        return Ok(());
    }

    let (provider_name, api_key, model, base_url) = resolve_llm_config(settings);
    if api_key.is_empty() {
        return Err("No API key available for design doc generation".to_string());
    }

    let system_msg = "You are a technical documentation agent. Given a design agent's output for a software component, produce a concise design document. Cover: purpose and scope, key architectural decisions and rationale, API contracts (interfaces, events, data types, endpoint paths), dependencies on other pieces, and any non-obvious constraints or tradeoffs. Be specific and technical. Aim for 300-500 words using bullet points.";

    // Build user content from piece metadata + agent output
    let mut user_parts = vec![format!("Component: \"{}\"", piece.name)];
    if !piece.responsibilities.is_empty() {
        user_parts.push(format!("Responsibilities: {}", piece.responsibilities));
    }
    if !piece.interfaces.is_empty() {
        let ifaces: Vec<String> = piece.interfaces.iter()
            .map(|i| format!("  - {} ({:?}): {}", i.name, i.direction, i.description))
            .collect();
        user_parts.push(format!("Interfaces:\n{}", ifaces.join("\n")));
    }
    if !piece.constraints.is_empty() {
        let constraints: Vec<String> = piece.constraints.iter()
            .map(|c| format!("  - [{}] {}", c.category, c.description))
            .collect();
        user_parts.push(format!("Constraints:\n{}", constraints.join("\n")));
    }

    let mut output = agent_output.to_string();
    if output.len() > 8000 {
        output = format!("(truncated — last portion)\n\n{}", &output[output.len() - 8000..]);
    }
    user_parts.push(format!("Agent output:\n{output}"));

    let messages = vec![
        Message {
            role: "system".to_string(),
            content: system_msg.to_string(),
        },
        Message {
            role: "user".to_string(),
            content: user_parts.join("\n\n"),
        },
    ];

    let provider = llm::create_provider(&provider_name);
    let config = LlmConfig {
        api_key,
        model,
        base_url,
        max_tokens: 1500,
    };

    let response = provider.chat(&messages, &config).await?;
    let doc_text = response.content;

    let state = app_handle.state::<AppState>();
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let title = format!("{} — Design Document", piece.name);
    db.upsert_artifact(piece_id, "design_doc", &title, &doc_text)?;

    info!(piece_id, doc_len = doc_text.len(), "Design document stored");
    Ok(())
}

/// Run the Leader Agent: analyze full diagram, produce a structured work plan
pub async fn run_leader_agent<R: tauri::Runtime>(
    project_id: &str,
    user_guidance: &str,
    db: &Mutex<Database>,
    app_handle: &AppHandle<R>,
) -> Result<WorkPlan, String> {
    // 1. Create WorkPlan row, supersede existing drafts
    let (plan_id, messages, provider_name, api_key, model, base_url) = {
        let db = db.lock().map_err(|e| e.to_string())?;

        // Mark existing draft plans as superseded
        db.supersede_draft_plans(project_id)?;

        // Create new plan
        let plan = db.create_work_plan(project_id, user_guidance)?;

        // Build prompt
        let messages = build_leader_prompt(&db, project_id, user_guidance);

        // Resolve LLM config — uses project settings, falls back to any available key
        let project = db.get_project(project_id)?;
        let (provider_name, api_key, model, base_url) =
            resolve_llm_config(&project.settings);

        (plan.id, messages, provider_name, api_key, model, base_url)
    };

    info!(project_id, plan_id = %plan_id, provider = %provider_name, "Starting leader agent");

    if api_key.is_empty() {
        return Err(format!(
            "No API key found for provider '{provider_name}'. Add it in Settings or set the environment variable."
        ));
    }

    let provider = llm::create_provider(&provider_name);
    let config = LlmConfig {
        api_key,
        model,
        base_url,
        max_tokens: 4096,
    };

    // Stream via channel -> Tauri events
    let (tx, mut rx) = mpsc::channel::<String>(256);
    let project_id_for_stream = project_id.to_string();
    let plan_id_for_stream = plan_id.clone();
    let app = app_handle.clone();
    let full_output = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
    let full_output_writer = full_output.clone();

    let stream_handle = tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            full_output_writer.lock().await.push_str(&chunk);
            let _ = app.emit(
                "leader-plan-chunk",
                json!({
                    "projectId": project_id_for_stream,
                    "planId": plan_id_for_stream,
                    "chunk": chunk,
                    "done": false,
                }),
            );
        }
    });

    let usage = provider.chat_stream(&messages, &config, tx).await?;

    // Wait for all chunks to be emitted
    let _ = stream_handle.await;

    let raw_output = full_output.lock().await.clone();

    // Parse the JSON output
    let (summary, tasks) = parse_plan_output(&raw_output);

    debug!(plan_id = %plan_id, task_count = tasks.len(), summary_len = summary.len(), "Leader agent output parsed");

    // Update plan row
    let plan = {
        let db = db.lock().map_err(|e| e.to_string())?;
        db.update_work_plan(
            &plan_id,
            &WorkPlanUpdate {
                status: Some(PlanStatus::Draft),
                summary: Some(summary),
                tasks: Some(tasks),
                raw_output: Some(raw_output),
                tokens_used: Some((usage.input + usage.output) as i64),
                integration_review: None,
            },
        )?
    };

    // Emit done event
    if let Err(e) = app_handle.emit(
        "leader-plan-chunk",
        json!({
            "projectId": project_id,
            "planId": plan_id,
            "chunk": "",
            "done": true,
        }),
    ) {
        warn!(error = %e, "Failed to emit leader-plan-chunk event");
    }

    info!(plan_id = %plan_id, version = plan.version, task_count = plan.tasks.len(), "Leader agent complete");

    Ok(plan)
}

/// Parse the LLM's JSON output into summary + tasks
fn parse_plan_output(raw: &str) -> (String, Vec<PlanTask>) {
    // Strip markdown fences if present
    let cleaned = raw.trim();
    let cleaned = if cleaned.starts_with("```") {
        // Remove opening fence (possibly ```json)
        let after_fence = cleaned
            .find('\n')
            .map(|i| &cleaned[i + 1..])
            .unwrap_or(cleaned);
        // Remove closing fence
        after_fence
            .rfind("```")
            .map(|i| &after_fence[..i])
            .unwrap_or(after_fence)
    } else {
        cleaned
    };

    // Find first { to last }
    let start = match cleaned.find('{') {
        Some(i) => i,
        None => {
            warn!("Failed to parse plan JSON output");
            return (
                "Plan generated but could not be parsed".to_string(),
                vec![],
            )
        }
    };
    let end = match cleaned.rfind('}') {
        Some(i) => i + 1,
        None => {
            warn!("Failed to parse plan JSON output");
            return (
                "Plan generated but could not be parsed".to_string(),
                vec![],
            )
        }
    };

    let json_str = &cleaned[start..end];

    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => {
            warn!("Failed to parse plan JSON output");
            return (
                "Plan generated but could not be parsed".to_string(),
                vec![],
            )
        }
    };

    let summary = parsed["summary"]
        .as_str()
        .unwrap_or("Work plan generated")
        .to_string();

    let tasks = parsed["tasks"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|t| {
                    let deps: Vec<String> = t["dependsOn"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();

                    PlanTask {
                        id: uuid::Uuid::new_v4().to_string(),
                        piece_id: t["pieceId"].as_str().unwrap_or("").to_string(),
                        piece_name: t["pieceName"].as_str().unwrap_or("").to_string(),
                        title: t["title"].as_str().unwrap_or("Untitled task").to_string(),
                        description: t["description"].as_str().unwrap_or("").to_string(),
                        priority: serde_json::from_str(
                            &format!("\"{}\"", t["priority"].as_str().unwrap_or("medium")),
                        )
                        .unwrap_or(TaskPriority::Medium),
                        suggested_phase: t["suggestedPhase"]
                            .as_str()
                            .unwrap_or("design")
                            .to_string(),
                        dependencies: deps,
                        status: TaskStatus::Pending,
                        order: t["order"].as_i64().unwrap_or(0),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    (summary, tasks)
}

/// Resolve the best LLM config from project settings, falling back to
/// whichever provider actually has an API key available.
pub fn resolve_llm_config(
    settings: &crate::models::ProjectSettings,
) -> (String, String, String, Option<String>) {
    // If the project has explicit LLM configs, try them in order
    for cfg in &settings.llm_configs {
        let key = resolve_api_key(&cfg.provider);
        if !key.is_empty() {
            return (cfg.provider.clone(), key, cfg.model.clone(), cfg.base_url.clone());
        }
    }

    // No explicit config (or none had a key) — try known providers
    for (provider, default_model) in [
        ("claude", "claude-sonnet-4-6"),
        ("openai", "gpt-4o"),
    ] {
        let key = resolve_api_key(provider);
        if !key.is_empty() {
            return (provider.to_string(), key, default_model.to_string(), None);
        }
    }

    // Nothing found — return claude so the caller can produce a clear error
    ("claude".to_string(), String::new(), "claude-sonnet-4-6".to_string(), None)
}

/// Resolve API key: try OS keyring first, then fall back to env var.
pub fn resolve_api_key(provider_name: &str) -> String {
    // Try keyring
    if let Ok(entry) = keyring::Entry::new("project-builder-dashboard", provider_name) {
        if let Ok(key) = entry.get_password() {
            if !key.is_empty() {
                debug!(provider = provider_name, source = "keyring", "API key resolved from keyring");
                return key;
            }
        }
    }
    // Env var fallback
    let env_var = match provider_name.to_lowercase().as_str() {
        "claude" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        _ => "LLM_API_KEY",
    };
    let val = std::env::var(env_var).unwrap_or_default();
    if !val.is_empty() {
        debug!(provider = provider_name, source = "env", "API key resolved from environment");
    }
    val
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::project_commands::create_project_impl;
    use crate::llm::{set_test_llm_responses, TestLlmResponses};
    use crate::test_support::ensure_test_tools;
    use std::collections::HashSet;
    use std::fs;
    use std::process::Command;
    use std::sync::Mutex;

    fn temp_workspace(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "project-builder-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).expect("create test workspace");
        path
    }

    #[test]
    fn parse_plan_output_handles_fenced_json_and_fallbacks() {
        let raw = r#"```json
{"summary":"Ready to ship","tasks":[{"pieceId":"piece-1","pieceName":"API","title":"Build API","description":"Implement it","priority":"high","suggestedPhase":"implementing","dependsOn":[],"order":1}]}
```"#;

        let (summary, tasks) = parse_plan_output(raw);
        assert_eq!(summary, "Ready to ship");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].piece_id, "piece-1");
        assert_eq!(tasks[0].suggested_phase, "implementing");

        let (summary, tasks) = parse_plan_output("definitely not json");
        assert_eq!(summary, "Plan generated but could not be parsed");
        assert!(tasks.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn run_all_plan_tasks_rejects_unapproved_plans() {
        ensure_test_tools();

        let workspace = temp_workspace("run-all-gate");
        let db_path = workspace.join("projects.db");
        let db = Database::new_at_path(&db_path).expect("open test db");
        let state = Mutex::new(db);
        let app = tauri::test::mock_app();
        let app_handle = app.handle().clone();

        let project = create_project_impl(
            &state,
            "Gate Project".to_string(),
            "Approval gate".to_string(),
            Some(workspace.to_string_lossy().to_string()),
        )
        .expect("create project");

        let piece = {
            let db = state.lock().expect("lock db");
            db.create_piece(&project.id, None, "Gate Piece", 0.0, 0.0)
                .expect("create piece")
        };

        let plan = {
            let db = state.lock().expect("lock db");
            let plan = db
                .create_work_plan(&project.id, "Gate guidance")
                .expect("create plan");
            db.update_work_plan(
                &plan.id,
                &WorkPlanUpdate {
                    tasks: Some(vec![PlanTask {
                        id: uuid::Uuid::new_v4().to_string(),
                        piece_id: piece.id,
                        piece_name: piece.name,
                        title: "Gate task".to_string(),
                        description: "Should not run".to_string(),
                        priority: TaskPriority::High,
                        suggested_phase: "implementing".to_string(),
                        dependencies: vec![],
                        status: TaskStatus::Pending,
                        order: 1,
                    }]),
                    ..Default::default()
                },
            )
            .expect("seed plan");
            plan
        };

        let running_pieces = Mutex::new(HashSet::new());
        let err = run_all_plan_tasks(&plan.id, None, &state, &running_pieces, &app_handle, None)
            .await
            .expect_err("draft plan should be rejected");
        assert!(err.contains("approved"));

        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn run_piece_agent_recovers_after_external_failure() {
        ensure_test_tools();

        let workspace = temp_workspace("run-all-recovery");
        let db_path = workspace.join("projects.db");
        let db = Database::new_at_path(&db_path).expect("open test db");
        let state = Mutex::new(db);
        let app = tauri::test::mock_app();
        let app_handle = app.handle().clone();

        let project = create_project_impl(
            &state,
            "Recovery Project".to_string(),
            "Retry after a failed external run".to_string(),
            Some(workspace.to_string_lossy().to_string()),
        )
        .expect("create project");
        let working_dir = project
            .settings
            .working_directory
            .clone()
            .expect("project working directory");

        let piece = {
            let db = state.lock().expect("lock db");
            let piece = db
                .create_piece(&project.id, None, "Retry Service", 0.0, 0.0)
                .expect("create piece");
            db.update_piece(
                &piece.id,
                &PieceUpdate {
                    agent_config: Some(AgentConfig {
                        execution_engine: Some("codex".to_string()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .expect("configure external engine");
            piece
        };

        let fail_marker = std::path::Path::new(&working_dir).join(".fake-codex-fail");
        fs::write(&fail_marker, "1").expect("mark codex failure");
        run_piece_agent(&piece.id, None, &state, &app_handle, None)
            .await
            .expect("first run should complete with failure metadata");

        {
            let db = state.lock().expect("lock db");
            let history = db.list_agent_history(&piece.id).expect("list history");
            assert_eq!(history.len(), 1);
            assert_eq!(history[0].metadata.success, Some(false));
        }

        fs::remove_file(&fail_marker).expect("clear codex failure");
        run_piece_agent(&piece.id, None, &state, &app_handle, None)
            .await
            .expect("retry should succeed");

        {
            let db = state.lock().expect("lock db");
            let history = db.list_agent_history(&piece.id).expect("list history");
            assert_eq!(history.len(), 2);
            assert_eq!(history[0].metadata.success, Some(true));
            assert_eq!(history[1].metadata.success, Some(false));

            let files_artifact = db
                .get_artifact_by_type(&piece.id, "generated_files")
                .expect("load generated files artifact")
                .expect("generated files artifact exists");
            assert!(files_artifact.content.contains("generated-from-codex.txt"));
        }

        let generated = fs::read_to_string(std::path::Path::new(&working_dir).join("generated-from-codex.txt"))
            .expect("read fake codex output");
        assert!(generated.contains("fake codex run"));

        let _ = fs::remove_dir_all(&workspace);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn core_orchestration_happy_path_completes_create_plan_run_merge_review() {
        ensure_test_tools();
        std::env::set_var("OPENAI_API_KEY", "test-openai-key");

        let workspace = temp_workspace("orchestration-smoke");
        let db_path = workspace.join("projects.db");
        let db = Database::new_at_path(&db_path).expect("open test db");
        let state = Mutex::new(db);

        let app = tauri::test::mock_app();
        let app_handle = app.handle().clone();

        let project = create_project_impl(
            &state,
            "Smoke Project".to_string(),
            "End-to-end orchestration smoke".to_string(),
            Some(workspace.to_string_lossy().to_string()),
        )
        .expect("create project");
        let working_dir = project
            .settings
            .working_directory
            .clone()
            .expect("project working directory");

        let piece = {
            let db = state.lock().expect("lock db");
            db.create_piece(&project.id, None, "Auth Service", 0.0, 0.0)
                .expect("create piece")
        };
        set_test_llm_responses(TestLlmResponses {
            leader_plan: serde_json::json!({
                "summary": "Ship the feature",
                "tasks": [{
                    "pieceId": piece.id.clone(),
                    "pieceName": piece.name.clone(),
                    "title": "Implement the feature",
                    "description": "Execute the end-to-end path",
                    "priority": "high",
                    "suggestedPhase": "implementing",
                    "dependsOn": [],
                    "order": 1
                }]
            })
            .to_string(),
            integration_review: "Integration review passed.".to_string(),
            summary: String::new(),
        });

        {
            let db = state.lock().expect("lock db");
            let mut settings = project.settings.clone();
            settings.llm_configs = vec![crate::models::LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4o-mini".to_string(),
                api_key_env: None,
                base_url: None,
            }];
            db.update_project(&project.id, None, None, None, Some(&settings))
                .expect("update project settings");

            db.update_piece(
                &piece.id,
                &PieceUpdate {
                    agent_config: Some(AgentConfig {
                        execution_engine: Some("codex".to_string()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .expect("configure external engine");

            drop(db);

            let plan = run_leader_agent(&project.id, "Build the end-to-end feature", &state, &app_handle)
                .await
                .expect("generate plan");
            let plan_id = plan.id.clone();
            assert_eq!(plan.status, PlanStatus::Draft);
            assert_eq!(plan.tasks.len(), 1);

            {
                let db = state.lock().expect("lock db");
                let approved = db
                    .update_work_plan(
                        &plan.id,
                        &WorkPlanUpdate {
                            status: Some(PlanStatus::Approved),
                            ..Default::default()
                        },
                    )
                    .expect("approve plan");
                assert_eq!(approved.status, PlanStatus::Approved);
            }

            let running_pieces = Mutex::new(HashSet::new());
            run_all_plan_tasks(&plan.id, None, &state, &running_pieces, &app_handle, None)
                .await
                .expect("run all tasks");

            let db = state.lock().expect("lock db");
            let plan = db.get_work_plan(&plan_id).expect("reload plan");
            assert_eq!(plan.status, PlanStatus::Approved);
            assert_eq!(plan.tasks[0].status, TaskStatus::Complete);
            assert!(plan.integration_review.contains("Integration review passed"));
        }

        let db = state.lock().expect("lock db");
        let plans = db.list_work_plans(&project.id).expect("list plans");
        assert_eq!(plans.len(), 1);
        let plan = &plans[0];
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].status, TaskStatus::Complete);
        assert!(plan.integration_review.contains("Integration review passed"));

        let piece_after = db.get_piece(&piece.id).expect("reload piece");
        assert_eq!(piece_after.phase, Phase::Implementing);

        let files_artifact = db
            .get_artifact_by_type(&piece.id, "generated_files")
            .expect("load generated files artifact")
            .expect("generated files artifact exists");
        assert!(files_artifact.content.contains("generated-from-codex.txt"));

        let generated = std::fs::read_to_string(std::path::Path::new(&working_dir).join("generated-from-codex.txt"))
            .expect("read fake codex output");
        assert!(generated.contains("fake codex run"));

        let git_status = Command::new("git")
            .args([
                "-C",
                working_dir.as_str(),
                "status",
                "--porcelain",
            ])
            .output()
            .expect("run git status");
        assert!(git_status.status.success());
        assert!(String::from_utf8_lossy(&git_status.stdout).trim().is_empty());

        drop(db);
        let _ = std::fs::remove_dir_all(&workspace);
    }
}
