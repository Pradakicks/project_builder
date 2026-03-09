use crate::agent::{build_agent_prompt, PieceContext};
use crate::db::Database;
use crate::llm::{self, LlmConfig, TokenUsage};
use serde_json::json;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

/// Run a piece's agent: build prompt, call LLM, stream results via events
pub async fn run_piece_agent(
    piece_id: &str,
    db: &Mutex<Database>,
    app_handle: &AppHandle,
) -> Result<TokenUsage, String> {
    // Load piece and context while holding the lock briefly
    let (piece, context, provider_name, api_key, model, base_url, max_tokens) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let piece = db.get_piece(piece_id)?;

        // Get connected pieces
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

        // Determine provider config
        let provider_name = piece
            .agent_config
            .provider
            .clone()
            .unwrap_or_else(|| "claude".to_string());
        let model = piece
            .agent_config
            .model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
        let max_tokens = piece.agent_config.token_budget.unwrap_or(4096) as u32;

        // Get API key: keyring first, then env var fallback
        let api_key = resolve_api_key(&provider_name);

        // Get project settings for base_url
        let project = db.get_project(&piece.project_id).ok();
        let base_url = project.and_then(|p| {
            p.settings
                .llm_configs
                .iter()
                .find(|c| c.provider.to_lowercase() == provider_name.to_lowercase())
                .and_then(|c| c.base_url.clone())
        });

        let context = PieceContext {
            connected_pieces,
            parent,
        };

        (piece, context, provider_name, api_key, model, base_url, max_tokens)
    };

    if api_key.is_empty() {
        return Err(format!(
            "No API key found for provider '{provider_name}'. Add it in Settings or set the environment variable."
        ));
    }

    let messages = build_agent_prompt(&piece, &context);
    let provider = llm::create_provider(&provider_name);
    let config = LlmConfig {
        api_key,
        model,
        base_url,
        max_tokens,
    };

    // Stream via channel -> Tauri events
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

    let usage = provider.chat_stream(&messages, &config, tx).await?;

    // Wait for all chunks to be emitted
    let _ = stream_handle.await;

    // Emit done event
    let _ = app_handle.emit(
        "agent-output-chunk",
        json!({
            "pieceId": piece_id,
            "chunk": "",
            "done": true,
            "usage": {
                "input": usage.input,
                "output": usage.output,
            }
        }),
    );

    // Store in agent_history
    {
        let db = db.lock().map_err(|e| e.to_string())?;
        let _ = db.insert_agent_history(
            piece_id,
            "run",
            &piece.agent_prompt,
            "",
            (usage.input + usage.output) as i64,
        );
    }

    Ok(usage)
}

/// Resolve API key: try OS keyring first, then fall back to env var.
pub fn resolve_api_key(provider_name: &str) -> String {
    // Try keyring
    if let Ok(entry) = keyring::Entry::new("project-builder-dashboard", provider_name) {
        if let Ok(key) = entry.get_password() {
            if !key.is_empty() {
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
    std::env::var(env_var).unwrap_or_default()
}
