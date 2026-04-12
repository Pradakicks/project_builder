use crate::agent;
use crate::db::AgentHistoryEntry;
use crate::llm::{self, LlmConfig, Message};
use crate::models::{
    Artifact, CtoDecision, CtoDecisionRecordInput, CtoRollbackResult, CtoRollbackResultStep,
    CtoRollbackResultStepStatus, CtoRollbackKind,
};
use crate::AppState;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;
use tracing::{info, warn};

#[tracing::instrument(skip(state, app_handle))]
#[tauri::command]
pub async fn run_piece_agent(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    piece_id: String,
    feedback: Option<String>,
) -> Result<(), String> {
    info!(piece_id = %piece_id, feedback = feedback.is_some(), "IPC: run_piece_agent");

    // Piece-level run lock — prevent double-runs on the same piece
    {
        let mut running = state.running_pieces.lock().map_err(|e| e.to_string())?;
        if !running.insert(piece_id.clone()) {
            return Err(format!(
                "An agent is already running for this piece. Wait for it to finish."
            ));
        }
    }

    let result = agent::runner::run_piece_agent(&piece_id, feedback.as_deref(), &state.db, &app_handle).await;

    // Always release the lock
    {
        let mut running = state.running_pieces.lock().map_err(|e| e.to_string())?;
        running.remove(&piece_id);
    }

    result.map(|_| ())
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn get_agent_history(
    state: State<'_, AppState>,
    piece_id: String,
) -> Result<Vec<AgentHistoryEntry>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_agent_history(&piece_id)
}

#[tracing::instrument(skip(state, app_handle, conversation), fields(project_id = %project_id, msg_len = user_message.len()))]
#[tauri::command]
pub async fn chat_with_cto(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    project_id: String,
    user_message: String,
    conversation: Vec<Message>,
    request_id: String,
) -> Result<(), String> {
    info!(project_id = %project_id, conversation_len = conversation.len(), "IPC: chat_with_cto");

    // Build CTO context and combine with conversation
    let (mut messages, provider_name, api_key, model, base_url) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let cto_messages = agent::build_cto_prompt(&db, &project_id);

        // Resolve LLM config — uses project settings, falls back to any available key
        let project = db.get_project(&project_id)?;
        let (provider_name, api_key, model, base_url) =
            agent::runner::resolve_llm_config(&project.settings);

        (cto_messages, provider_name, api_key, model, base_url)
    };

    if api_key.is_empty() {
        return Err(format!(
            "No API key found for provider '{provider_name}'. Add it in Settings or set the environment variable."
        ));
    }

    // Append conversation history + new user message
    for msg in &conversation {
        messages.push(msg.clone());
    }
    messages.push(Message {
        role: "user".to_string(),
        content: user_message,
    });

    let provider = llm::create_provider(&provider_name);
    let config = LlmConfig {
        api_key,
        model,
        base_url,
        max_tokens: 4096,
    };

    let (tx, mut rx) = mpsc::channel::<String>(256);
    let app = app_handle.clone();
    let project_id_for_stream = project_id.clone();
    let request_id_for_stream = request_id.clone();

    let stream_handle = tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            let _ = app.emit(
                "cto-chat-chunk",
                json!({
                    "projectId": project_id_for_stream,
                    "requestId": request_id_for_stream,
                    "chunk": chunk,
                    "done": false,
                }),
            );
        }
    });

    let usage = provider.chat_stream(&messages, &config, tx).await?;

    let _ = stream_handle.await;

    if let Err(e) = app_handle.emit(
        "cto-chat-chunk",
        json!({
            "projectId": project_id,
            "requestId": request_id,
            "chunk": "",
            "done": true,
            "usage": {
                "input": usage.input,
                "output": usage.output,
            }
        }),
    ) {
        warn!(error = %e, "Failed to emit cto-chat-chunk done event");
    }

    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusInfo {
    pub current_branch: String,
    pub has_uncommitted_changes: bool,
    pub last_commit_message: Option<String>,
    pub last_commit_sha: Option<String>,
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub async fn get_git_status(
    state: State<'_, AppState>,
    piece_id: String,
) -> Result<Option<GitStatusInfo>, String> {
    // Load piece → project → working_directory
    let working_dir = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let piece = db.get_piece(&piece_id)?;
        let project = db.get_project(&piece.project_id)?;
        project.settings.working_directory
    };

    let working_dir = match working_dir {
        Some(dir) => dir,
        None => return Ok(None),
    };

    let current_branch = agent::git_ops::current_branch(&working_dir)
        .await
        .unwrap_or_else(|_| "unknown".to_string());

    let has_uncommitted = agent::git_ops::has_uncommitted_changes(&working_dir)
        .await
        .unwrap_or(false);

    let last_sha = agent::git_ops::get_head_sha(&working_dir).await.ok();

    // Get last commit message
    let last_message = tokio::process::Command::new("git")
        .current_dir(&working_dir)
        .args(["log", "-1", "--format=%s"])
        .output()
        .await
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let msg = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if msg.is_empty() { None } else { Some(msg) }
            } else {
                None
            }
        });

    Ok(Some(GitStatusInfo {
        current_branch,
        has_uncommitted_changes: has_uncommitted,
        last_commit_message: last_message,
        last_commit_sha: last_sha,
    }))
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn list_artifacts(
    state: State<'_, AppState>,
    piece_id: String,
) -> Result<Vec<Artifact>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_artifacts(&piece_id)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn log_cto_decision(
    state: State<'_, AppState>,
    project_id: String,
    decision: CtoDecisionRecordInput,
) -> Result<CtoDecision, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.insert_cto_decision(&project_id, &decision)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn list_cto_decisions(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<CtoDecision>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_cto_decisions(&project_id)
}

fn piece_to_update(piece: &crate::models::Piece) -> crate::db::PieceUpdate {
    crate::db::PieceUpdate {
        name: Some(piece.name.clone()),
        piece_type: Some(piece.piece_type.clone()),
        color: piece.color.clone(),
        icon: piece.icon.clone(),
        responsibilities: Some(piece.responsibilities.clone()),
        interfaces: Some(piece.interfaces.clone()),
        constraints: Some(piece.constraints.clone()),
        notes: Some(piece.notes.clone()),
        agent_prompt: Some(piece.agent_prompt.clone()),
        agent_config: Some(piece.agent_config.clone()),
        output_mode: Some(piece.output_mode.clone()),
        phase: Some(piece.phase.clone()),
        position_x: Some(piece.position_x),
        position_y: Some(piece.position_y),
    }
}

fn connection_to_update(connection: &crate::models::Connection) -> crate::db::ConnectionUpdate {
    crate::db::ConnectionUpdate {
        label: Some(connection.label.clone()),
        direction: Some(connection.direction.clone()),
        data_type: connection.data_type.clone(),
        protocol: connection.protocol.clone(),
        constraints: Some(connection.constraints.clone()),
        notes: Some(connection.notes.clone()),
        metadata: Some(connection.metadata.clone()),
    }
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn rollback_cto_decision(
    state: State<'_, AppState>,
    decision_id: String,
) -> Result<CtoDecision, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let decision = db.get_cto_decision(&decision_id)?;

    if matches!(decision.status, crate::models::CtoDecisionStatus::RolledBack) {
        return Err("This CTO decision has already been rolled back.".to_string());
    }

    let execution = decision
        .execution
        .as_ref()
        .ok_or("This CTO decision does not have execution data to roll back.")?;
    if !execution.rollback.supported {
        return Err(
            execution
                .rollback
                .reason
                .clone()
                .unwrap_or_else(|| "This CTO decision is not rollback-safe.".to_string()),
        );
    }

    let mut results: Vec<CtoRollbackResultStep> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for step in execution.rollback.steps.iter().rev() {
        if !step.supported {
            results.push(CtoRollbackResultStep {
                index: step.index,
                action: step.action.clone(),
                description: step.description.clone(),
                status: CtoRollbackResultStepStatus::Skipped,
                error: step.reason.clone(),
            });
            continue;
        }

        let outcome = match &step.kind {
            Some(CtoRollbackKind::RestorePiece { piece }) => {
                db.update_piece(&piece.id, &piece_to_update(piece))
                    .map(|_| CtoRollbackResultStepStatus::Applied)
                    .map_err(|e| e.to_string())
            }
            Some(CtoRollbackKind::DeletePiece { piece_id }) => {
                db.delete_piece(piece_id)
                    .map(|_| CtoRollbackResultStepStatus::Applied)
                    .map_err(|e| e.to_string())
            }
            Some(CtoRollbackKind::RestoreConnection { connection }) => {
                db.update_connection(&connection.id, &connection_to_update(connection))
                    .map(|_| CtoRollbackResultStepStatus::Applied)
                    .map_err(|e| e.to_string())
            }
            Some(CtoRollbackKind::DeleteConnection { connection_id }) => {
                db.delete_connection(connection_id)
                    .map(|_| CtoRollbackResultStepStatus::Applied)
                    .map_err(|e| e.to_string())
            }
            Some(CtoRollbackKind::RestorePlanStatus { plan_id, status }) => {
                db.update_work_plan(
                    plan_id,
                    &crate::models::WorkPlanUpdate {
                        status: Some(status.clone()),
                        ..Default::default()
                    },
                )
                .map(|_| CtoRollbackResultStepStatus::Applied)
                .map_err(|e| e.to_string())
            }
            None => Err("Rollback data missing for executed action".to_string()),
        };

        match outcome {
            Ok(status) => results.push(CtoRollbackResultStep {
                index: step.index,
                action: step.action.clone(),
                description: step.description.clone(),
                status,
                error: None,
            }),
            Err(error) => {
                errors.push(format!("Action {} rollback failed: {}", step.action, error));
                results.push(CtoRollbackResultStep {
                    index: step.index,
                    action: step.action.clone(),
                    description: step.description.clone(),
                    status: CtoRollbackResultStepStatus::Failed,
                    error: Some(error),
                });
            }
        }
    }

    let rollback = CtoRollbackResult {
        applied_at: chrono::Utc::now().to_rfc3339(),
        steps: results,
        errors: errors.clone(),
    };

    let updated = db.record_cto_decision_rollback(
        &decision_id,
        &rollback,
        if errors.is_empty() {
            crate::models::CtoDecisionStatus::RolledBack
        } else {
            crate::models::CtoDecisionStatus::Failed
        },
    )?;
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }

    Ok(updated)
}
