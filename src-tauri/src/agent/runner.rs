use crate::agent::{build_agent_prompt, build_external_prompt, build_leader_prompt, PieceContext};
use crate::db::Database;
use crate::llm::{self, LlmConfig, TokenUsage};
use crate::models::*;
use serde_json::json;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

/// Load a piece and its context from the database.
fn load_piece_context(
    piece_id: &str,
    db: &Mutex<Database>,
) -> Result<(Piece, PieceContext, ProjectSettings), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let piece = db.get_piece(piece_id)?;

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

    let project = db.get_project(&piece.project_id).ok();
    let settings = project.map(|p| p.settings).unwrap_or_default();

    let context = PieceContext {
        connected_pieces,
        parent,
    };

    Ok((piece, context, settings))
}

/// Run a piece's agent: dispatches to built-in LLM or external tool based on config.
pub async fn run_piece_agent(
    piece_id: &str,
    db: &Mutex<Database>,
    app_handle: &AppHandle,
) -> Result<TokenUsage, String> {
    let (piece, context, settings) = load_piece_context(piece_id, db)?;

    let engine = piece
        .agent_config
        .execution_engine
        .as_deref()
        .unwrap_or("built-in");

    match engine {
        "built-in" | "" => {
            run_builtin_agent(&piece, &context, &settings, piece_id, db, app_handle).await
        }
        name => {
            run_external_agent(&piece, &context, &settings, name, piece_id, db, app_handle).await
        }
    }
}

/// Run the built-in LLM agent (existing behavior).
async fn run_builtin_agent(
    piece: &Piece,
    context: &PieceContext,
    settings: &ProjectSettings,
    piece_id: &str,
    db: &Mutex<Database>,
    app_handle: &AppHandle,
) -> Result<TokenUsage, String> {
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

    if api_key.is_empty() {
        return Err(format!(
            "No API key found for provider '{provider_name}'. Add it in Settings or set the environment variable."
        ));
    }

    let messages = build_agent_prompt(piece, context);
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
    let _ = stream_handle.await;

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

/// Run an external tool (Claude Code, Codex, etc.) as the execution engine.
async fn run_external_agent(
    piece: &Piece,
    context: &PieceContext,
    settings: &ProjectSettings,
    engine_name: &str,
    piece_id: &str,
    db: &Mutex<Database>,
    app_handle: &AppHandle,
) -> Result<TokenUsage, String> {
    let working_dir = settings
        .working_directory
        .as_deref()
        .ok_or_else(|| {
            "No working directory set. Configure one in Project Settings before using external tools."
                .to_string()
        })?;

    let (system_prompt, user_prompt) = build_external_prompt(piece, context);
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

    let run_config = super::external::ExternalRunConfig {
        system_prompt,
        user_prompt: user_prompt.clone(),
        working_dir: working_dir.to_string(),
        timeout_secs,
        env_vars,
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

    let result = super::external::run_external(engine_name, &run_config, tx).await;
    let _ = stream_handle.await;

    match result {
        Ok(run_result) => {
            let _ = app_handle.emit(
                "agent-output-chunk",
                json!({
                    "pieceId": piece_id,
                    "chunk": "",
                    "done": true,
                    "exitCode": run_result.exit_code,
                    "usage": { "input": 0, "output": 0 },
                }),
            );

            {
                let db = db.lock().map_err(|e| e.to_string())?;
                let _ = db.insert_agent_history(
                    piece_id,
                    "external-run",
                    &user_prompt,
                    &run_result.output,
                    0,
                );
            }

            Ok(TokenUsage { input: 0, output: 0 })
        }
        Err(err) => {
            let _ = app_handle.emit(
                "agent-output-chunk",
                json!({
                    "pieceId": piece_id,
                    "chunk": "",
                    "done": true,
                    "exitCode": -1,
                    "usage": { "input": 0, "output": 0 },
                }),
            );
            Err(err)
        }
    }
}

/// Run the Leader Agent: analyze full diagram, produce a structured work plan
pub async fn run_leader_agent(
    project_id: &str,
    user_guidance: &str,
    db: &Mutex<Database>,
    app_handle: &AppHandle,
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
            },
        )?
    };

    // Emit done event
    let _ = app_handle.emit(
        "leader-plan-chunk",
        json!({
            "planId": plan_id,
            "chunk": "",
            "done": true,
        }),
    );

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
            return (
                "Plan generated but could not be parsed".to_string(),
                vec![],
            )
        }
    };
    let end = match cleaned.rfind('}') {
        Some(i) => i + 1,
        None => {
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
