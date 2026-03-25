use crate::agent;
use crate::db::AgentHistoryEntry;
use crate::llm::{self, LlmConfig, Message};
use crate::AppState;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

#[tauri::command]
pub async fn run_piece_agent(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    piece_id: String,
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

    let result = agent::runner::run_piece_agent(&piece_id, &state.db, &app_handle).await;

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
