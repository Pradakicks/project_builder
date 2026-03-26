use crate::agent::{build_agent_prompt, build_external_prompt, build_leader_prompt, next_phase, PieceContext};
use crate::db::{Database, PieceUpdate};
use crate::llm::{self, LlmConfig, Message, TokenUsage};
use crate::models::*;
use crate::AppState;
use serde_json::json;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
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

    // Load context summaries from connected pieces
    let connected_summaries: Vec<(String, String)> = connected_pieces
        .iter()
        .filter_map(|cp| {
            db.get_artifact_by_type(&cp.id, "context_summary")
                .ok()
                .flatten()
                .map(|a| (cp.name.clone(), a.content))
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
        git_branch: Option<String>,
        git_commit_sha: Option<String>,
        git_diff_stat: Option<String>,
    },
}

/// Run a piece's agent: dispatches to built-in LLM or external tool based on config.
/// Emits the unified done event with phase transition fields.
/// Optional `feedback` enables iterative mode: previous output + feedback are injected as context.
pub async fn run_piece_agent(
    piece_id: &str,
    feedback: Option<&str>,
    db: &Mutex<Database>,
    app_handle: &AppHandle,
) -> Result<TokenUsage, String> {
    let (piece, context, settings) = load_piece_context(piece_id, db)?;

    let engine = piece
        .agent_config
        .execution_engine
        .as_deref()
        .unwrap_or("built-in");

    let result = match engine {
        "built-in" | "" => {
            run_builtin_agent(&piece, &context, &settings, piece_id, feedback, db, app_handle).await
        }
        name => {
            run_external_agent(&piece, &context, &settings, name, piece_id, feedback, db, app_handle).await
        }
    };

    // Determine if the run was successful
    let success = match &result {
        Ok(AgentResult::Builtin { .. }) => true,
        Ok(AgentResult::External { exit_code, .. }) => *exit_code == 0,
        Err(_) => false,
    };

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
        }
        Ok(AgentResult::External { exit_code, git_branch, git_commit_sha, git_diff_stat }) => {
            done_payload["usage"] = json!({"input": 0, "output": 0});
            done_payload["exitCode"] = json!(exit_code);
            if let Some(ref branch) = git_branch {
                done_payload["gitBranch"] = json!(branch);
            }
            if let Some(ref sha) = git_commit_sha {
                done_payload["gitCommitSha"] = json!(sha);
            }
            if let Some(ref stat) = git_diff_stat {
                done_payload["gitDiffStat"] = json!(stat);
            }
        }
        Err(_) => {
            done_payload["usage"] = json!({"input": 0, "output": 0});
            done_payload["exitCode"] = json!(-1);
        }
    }

    if let Some(ref proposal) = phase_proposal {
        done_payload["phaseProposal"] = json!(proposal);
    }
    if let Some(ref changed) = phase_changed {
        done_payload["phaseChanged"] = json!(changed);
    }
    let _ = app_handle.emit("agent-output-chunk", done_payload);

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
                eprintln!("Warning: context summary generation failed: {e}");
            }
        });
    }

    // Convert result to TokenUsage for return
    match result {
        Ok(AgentResult::Builtin { usage, .. }) => Ok(usage),
        Ok(AgentResult::External { .. }) => Ok(TokenUsage { input: 0, output: 0 }),
        Err(e) => Err(e),
    }
}

/// Run the built-in LLM agent. Streams chunks but does NOT emit the done event
/// (that's handled by run_piece_agent).
async fn run_builtin_agent(
    piece: &Piece,
    context: &PieceContext,
    settings: &ProjectSettings,
    piece_id: &str,
    feedback: Option<&str>,
    db: &Mutex<Database>,
    app_handle: &AppHandle,
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

    if api_key.is_empty() {
        return Err(format!(
            "No API key found for provider '{provider_name}'. Add it in Settings or set the environment variable."
        ));
    }

    let mut messages = build_agent_prompt(piece, context);

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

    let usage = provider.chat_stream(&messages, &config, tx).await?;
    let _ = stream_handle.await;

    let output_text = full_output.lock().await.clone();

    {
        let db = db.lock().map_err(|e| e.to_string())?;
        let _ = db.insert_agent_history(
            piece_id,
            "run",
            &piece.agent_prompt,
            &output_text,
            (usage.input + usage.output) as i64,
        );
    }

    Ok(AgentResult::Builtin { usage, output: output_text })
}

/// Emit a git-related info line through the agent output stream.
fn emit_git_info(app_handle: &AppHandle, piece_id: &str, message: &str) {
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
async fn run_external_agent(
    piece: &Piece,
    context: &PieceContext,
    settings: &ProjectSettings,
    engine_name: &str,
    piece_id: &str,
    feedback: Option<&str>,
    db: &Mutex<Database>,
    app_handle: &AppHandle,
) -> Result<AgentResult, String> {
    use super::git_ops;

    let working_dir = settings
        .working_directory
        .as_deref()
        .ok_or_else(|| {
            "No working directory set. Configure one in Project Settings before using external tools."
                .to_string()
        })?;

    let (system_prompt, user_prompt_base) = build_external_prompt(piece, context);
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
    let mut before_sha: Option<String> = None;

    // Save HEAD SHA before any changes
    match git_ops::get_head_sha(working_dir).await {
        Ok(sha) => before_sha = Some(sha),
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

    let result = super::external::run_external(engine_name, &run_config, tx).await;
    let _ = stream_handle.await;

    // ── Git: post-execution ─────────────────────────────────
    match result {
        Ok(run_result) => {
            let exit_code = run_result.exit_code;
            let mut git_commit_sha: Option<String> = None;
            let mut git_diff_stat: Option<String> = None;

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
            }

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

            Ok(AgentResult::External {
                exit_code,
                git_branch,
                git_commit_sha,
                git_diff_stat,
            })
        }
        Err(err) => Err(err),
    }
}

/// Generate a concise context summary from an agent's output and store as an artifact.
/// Called fire-and-forget after successful agent runs.
async fn generate_context_summary(
    piece_id: &str,
    piece_name: &str,
    agent_output: &str,
    git_info: Option<(&str, &str)>, // (working_dir, branch_name)
    settings: &ProjectSettings,
    app_handle: &AppHandle,
) -> Result<(), String> {
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

    Ok(())
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
