use crate::agent;
use crate::db::AgentHistoryEntry;
use crate::llm::{self, LlmConfig, Message};
use crate::models::{Artifact, CtoDecision};
use crate::AppState;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

#[tauri::command]
pub async fn run_piece_agent(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    piece_id: String,
    feedback: Option<String>,
) -> Result<(), String> {
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

#[tauri::command]
pub fn get_agent_history(
    state: State<'_, AppState>,
    piece_id: String,
) -> Result<Vec<AgentHistoryEntry>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_agent_history(&piece_id)
}

#[tauri::command]
pub async fn chat_with_cto(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    project_id: String,
    user_message: String,
    conversation: Vec<Message>,
) -> Result<(), String> {
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

    let stream_handle = tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            let _ = app.emit(
                "cto-chat-chunk",
                json!({
                    "chunk": chunk,
                    "done": false,
                }),
            );
        }
    });

    let usage = provider.chat_stream(&messages, &config, tx).await?;

    let _ = stream_handle.await;

    let _ = app_handle.emit(
        "cto-chat-chunk",
        json!({
            "chunk": "",
            "done": true,
            "usage": {
                "input": usage.input,
                "output": usage.output,
            }
        }),
    );

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

#[tauri::command]
pub fn list_artifacts(
    state: State<'_, AppState>,
    piece_id: String,
) -> Result<Vec<Artifact>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_artifacts(&piece_id)
}

#[tauri::command]
pub fn log_cto_decision(
    state: State<'_, AppState>,
    project_id: String,
    summary: String,
    actions_json: String,
) -> Result<CtoDecision, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.insert_cto_decision(&project_id, &summary, &actions_json)
}

#[tauri::command]
pub fn list_cto_decisions(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<CtoDecision>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_cto_decisions(&project_id)
}
