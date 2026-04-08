//! Merge orchestrator: combines piece branches back to main after plan completion,
//! handles conflicts (with optional AI resolution), and runs integration review.

use crate::agent::git_ops;
use crate::agent::runner::resolve_llm_config;
use crate::db::Database;
use crate::llm::{self, LlmConfig, Message};
use crate::models::*;
use serde::Serialize;
use serde_json::json;
use std::collections::HashSet;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tracing::{debug, info, warn, error, trace};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeProgress {
    pub plan_id: String,
    pub piece_name: String,
    pub branch: String,
    pub status: String,
    pub message: String,
    pub current: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeSummary {
    pub merged: Vec<String>,
    pub skipped: Vec<String>,
    pub conflict: Option<ConflictInfo>,
    pub combined_diff_stat: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub piece_id: String,
    pub piece_name: String,
    pub branch: String,
    pub conflicting_files: Vec<String>,
    pub conflict_diff: String,
}

/// Merge all piece branches for a completed plan back to main.
/// Emits "merge-progress" events throughout.
pub async fn merge_plan_branches(
    plan_id: &str,
    db: &Mutex<Database>,
    app_handle: &AppHandle,
) -> Result<MergeSummary, String> {
    // 1. Load plan and validate
    let (plan, working_dir, conflict_policy) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let plan = db.get_work_plan(plan_id)?;
        let project = db.get_project(&plan.project_id)?;
        let working_dir = project
            .settings
            .working_directory
            .ok_or("No working directory configured. Set it in project Settings.")?;
        let policy = project.settings.conflict_resolution;
        (plan, working_dir, policy)
    };

    // Verify plan is approved and all tasks are complete
    if !matches!(plan.status, PlanStatus::Approved) {
        return Err("Plan must be approved before merging.".to_string());
    }
    let all_complete = plan.tasks.iter().all(|t| matches!(t.status, TaskStatus::Complete | TaskStatus::Skipped));
    if !all_complete {
        return Err("All tasks must be complete before merging.".to_string());
    }

    // 2. Collect unique piece branches in task order
    let mut seen = HashSet::new();
    let mut branches: Vec<(String, String, String)> = Vec::new(); // (piece_id, piece_name, branch)
    for task in plan.tasks.iter() {
        if seen.contains(&task.piece_id) {
            continue;
        }
        seen.insert(task.piece_id.clone());
        let branch = git_ops::slugify_branch_name(&task.piece_name);
        branches.push((task.piece_id.clone(), task.piece_name.clone(), branch));
    }

    // 3. Filter to branches that actually exist
    let mut valid_branches: Vec<(String, String, String)> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for (pid, pname, branch) in branches {
        match git_ops::branch_exists(&working_dir, &branch).await {
            Ok(true) => valid_branches.push((pid, pname, branch)),
            _ => skipped.push(branch),
        }
    }

    let total = valid_branches.len();
    info!(plan_id, branch_count = total, skipped = skipped.len(), ?conflict_policy, "Starting branch merge");

    if total == 0 {
        return Ok(MergeSummary {
            merged: vec![],
            skipped,
            conflict: None,
            combined_diff_stat: String::new(),
        });
    }

    // 4. Stash any dirty state and checkout main
    if git_ops::has_uncommitted_changes(&working_dir).await.unwrap_or(false) {
        let _ = git_ops::stage_and_commit(&working_dir, "WIP: save state before merge").await;
    }
    let before_sha = git_ops::get_head_sha(&working_dir).await.unwrap_or_default();
    git_ops::checkout_branch(&working_dir, "main").await?;

    // 5. Merge each branch in order
    let mut merged: Vec<String> = Vec::new();
    for (i, (piece_id, piece_name, branch)) in valid_branches.iter().enumerate() {
        // Emit merging
        emit_progress(app_handle, plan_id, piece_name, branch, "merging",
            &format!("Merging {branch} into main..."), i + 1, total);

        match git_ops::try_merge(&working_dir, branch).await {
            Ok(true) => {
                // Clean merge — commit it
                let msg = format!("Merge {branch}: {} phase work", piece_name);
                let _ = git_ops::complete_merge(&working_dir, &msg).await;
                emit_progress(app_handle, plan_id, piece_name, branch, "merged",
                    &format!("Successfully merged {branch}"), i + 1, total);
                merged.push(branch.clone());
            }
            Ok(false) => {
                // Conflict detected
                let conflict_files = git_ops::list_conflict_files(&working_dir).await.unwrap_or_default();
                warn!(branch = %branch, files = ?conflict_files, "Merge conflict detected");
                let conflict_diff = git_ops::get_conflict_diff(&working_dir).await.unwrap_or_default();

                let conflict_info = ConflictInfo {
                    piece_id: piece_id.clone(),
                    piece_name: piece_name.clone(),
                    branch: branch.clone(),
                    conflicting_files: conflict_files.clone(),
                    conflict_diff: conflict_diff.clone(),
                };

                match conflict_policy {
                    ConflictResolutionPolicy::AutoResolve => {
                        emit_progress(app_handle, plan_id, piece_name, branch, "conflict-resolving",
                            &format!("Conflict in {branch} — AI resolving..."), i + 1, total);

                        match resolve_conflict_with_ai_internal(
                            piece_id, piece_name, branch, &conflict_diff, &conflict_files,
                            &working_dir, db, app_handle,
                        ).await {
                            Ok(()) => {
                                info!(branch = %branch, "AI auto-resolved conflict");
                                emit_progress(app_handle, plan_id, piece_name, branch, "conflict-resolved",
                                    &format!("AI resolved conflict in {branch}"), i + 1, total);
                                merged.push(branch.clone());
                            }
                            Err(e) => {
                                error!(branch = %branch, error = %e, "AI auto-resolution failed");
                                if let Err(abort_err) = git_ops::abort_merge(&working_dir).await {
                                    warn!(error = %abort_err, "Failed to abort merge after resolution failure");
                                }
                                emit_progress(app_handle, plan_id, piece_name, branch, "failed",
                                    &format!("AI resolution failed: {e}"), i + 1, total);
                                let combined = diff_stat_if_merged(&working_dir, &before_sha).await;
                                return Ok(MergeSummary {
                                    merged,
                                    skipped,
                                    conflict: Some(conflict_info),
                                    combined_diff_stat: combined,
                                });
                            }
                        }
                    }
                    ConflictResolutionPolicy::AiAssisted => {
                        // Stop and wait for user to click "Resolve with AI"
                        emit_progress(app_handle, plan_id, piece_name, branch, "conflict",
                            &format!("Conflict in {} files — waiting for resolution", conflict_files.len()),
                            i + 1, total);

                        // Don't abort — leave merge in progress for resolve_merge_conflict to pick up
                        let combined = diff_stat_if_merged(&working_dir, &before_sha).await;
                        return Ok(MergeSummary {
                            merged,
                            skipped,
                            conflict: Some(conflict_info),
                            combined_diff_stat: combined,
                        });
                    }
                    ConflictResolutionPolicy::Manual => {
                        if let Err(e) = git_ops::abort_merge(&working_dir).await {
                            warn!(error = %e, "Failed to abort merge (manual policy)");
                        }
                        emit_progress(app_handle, plan_id, piece_name, branch, "conflict",
                            &format!("Conflict in {} files — resolve manually", conflict_files.len()),
                            i + 1, total);

                        let combined = diff_stat_if_merged(&working_dir, &before_sha).await;
                        return Ok(MergeSummary {
                            merged,
                            skipped,
                            conflict: Some(conflict_info),
                            combined_diff_stat: combined,
                        });
                    }
                }
            }
            Err(e) => {
                emit_progress(app_handle, plan_id, piece_name, branch, "failed",
                    &format!("Merge error: {e}"), i + 1, total);
                let combined = diff_stat_if_merged(&working_dir, &before_sha).await;
                return Ok(MergeSummary {
                    merged,
                    skipped,
                    conflict: None,
                    combined_diff_stat: combined,
                });
            }
        }
    }

    // 6. All done — compute combined diff stat
    let combined = diff_stat_if_merged(&working_dir, &before_sha).await;
    info!(plan_id, merged_count = merged.len(), "Branch merge complete");

    Ok(MergeSummary {
        merged,
        skipped,
        conflict: None,
        combined_diff_stat: combined,
    })
}

/// Resolve a merge conflict using AI. Called from IPC when user clicks "Resolve with AI".
/// Expects to be called while a merge conflict is still active (not aborted).
pub async fn resolve_merge_conflict(
    plan_id: &str,
    piece_id: &str,
    db: &Mutex<Database>,
    app_handle: &AppHandle,
) -> Result<(), String> {
    let (piece_name, working_dir) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let piece = db.get_piece(piece_id)?;
        let project = db.get_project(&piece.project_id)?;
        let wd = project.settings.working_directory
            .ok_or("No working directory configured.")?;
        (piece.name.clone(), wd)
    };

    let branch = git_ops::slugify_branch_name(&piece_name);
    info!(plan_id, piece_id, piece_name = %piece_name, "Resolving merge conflict via AI");
    let conflict_diff = git_ops::get_conflict_diff(&working_dir).await?;
    let conflict_files = git_ops::list_conflict_files(&working_dir).await?;

    emit_progress(app_handle, plan_id, &piece_name, &branch, "conflict-resolving",
        "AI resolving conflict...", 0, 0);

    resolve_conflict_with_ai_internal(
        piece_id, &piece_name, &branch, &conflict_diff, &conflict_files,
        &working_dir, db, app_handle,
    ).await?;

    emit_progress(app_handle, plan_id, &piece_name, &branch, "conflict-resolved",
        "AI resolved conflict", 0, 0);

    Ok(())
}

/// Internal AI conflict resolution — spawns external tool or calls LLM to resolve.
async fn resolve_conflict_with_ai_internal(
    piece_id: &str,
    piece_name: &str,
    branch: &str,
    conflict_diff: &str,
    conflict_files: &[String],
    working_dir: &str,
    db: &Mutex<Database>,
    app_handle: &AppHandle,
) -> Result<(), String> {
    let (execution_engine, settings) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let piece = db.get_piece(piece_id)?;
        let project = db.get_project(&piece.project_id)?;
        (piece.agent_config.execution_engine.clone(), project.settings.clone())
    };

    let files_list = conflict_files.join(", ");
    let system_prompt = format!(
        "You are resolving merge conflicts in the following files: {files_list}\n\
         The branch '{branch}' is being merged into main.\n\
         Below is the conflict diff showing both sides.\n\
         Edit the conflicting files to resolve all conflict markers (<<<<<<< ======= >>>>>>>).\n\
         Produce the correct merged version that preserves the intent of both sides."
    );
    let user_prompt = format!(
        "Resolve these merge conflicts:\n\n{conflict_diff}"
    );

    let engine = execution_engine.as_deref().unwrap_or("built-in");
    debug!(piece_id, engine, conflict_files = ?conflict_files, "AI conflict resolution starting");
    trace!(conflict_diff_len = conflict_diff.len(), "Conflict diff size");

    match engine {
        "claude-code" | "codex" => {
            // Use external tool to resolve — it can directly edit files
            use crate::agent::external::{ExternalRunConfig, run_external};

            let mut env_vars = Vec::new();
            if engine == "codex" {
                let key = crate::agent::runner::resolve_api_key("openai");
                if !key.is_empty() {
                    env_vars.push(("OPENAI_API_KEY".to_string(), key));
                }
            }

            let config = ExternalRunConfig {
                system_prompt: system_prompt.clone(),
                user_prompt: user_prompt.clone(),
                working_dir: working_dir.to_string(),
                timeout_secs: 120,
                env_vars,
            };

            let (tx, mut rx) = mpsc::channel::<String>(256);
            let app = app_handle.clone();
            let piece_name_owned = piece_name.to_string();
            let stream_task = tokio::spawn(async move {
                while let Some(chunk) = rx.recv().await {
                    let _ = app.emit("merge-resolve-chunk", json!({
                        "pieceName": piece_name_owned,
                        "chunk": chunk,
                        "done": false,
                    }));
                }
            });

            let result = run_external(engine, &config, tx).await?;
            let _ = stream_task.await;

            if result.exit_code != 0 {
                return Err(format!("External tool exited with code {}", result.exit_code));
            }

            // Complete the merge
            let msg = format!("Resolve merge conflict for {branch} (AI-assisted)");
            git_ops::complete_merge(working_dir, &msg).await?;
            Ok(())
        }
        _ => {
            // Built-in LLM: get resolution via chat, but we can't easily write files
            // Instead, tell the user to use an external tool for conflict resolution
            let (provider_name, api_key, model, base_url) = resolve_llm_config(&settings);
            if api_key.is_empty() {
                return Err("No API key available for conflict resolution.".to_string());
            }

            let provider = llm::create_provider(&provider_name);
            let config = LlmConfig { api_key, model, base_url, max_tokens: 4096 };
            let messages = vec![
                Message { role: "system".into(), content: system_prompt },
                Message { role: "user".into(), content: user_prompt },
            ];

            let response = provider.chat(&messages, &config).await?;

            // Write resolved content — the LLM should produce file contents
            // For each conflict file, try to extract and write the resolution
            for file in conflict_files {
                let file_path = std::path::Path::new(working_dir).join(file);
                if let Ok(content) = std::fs::read_to_string(&file_path) {
                    // If file still has conflict markers after LLM response, try to apply
                    if content.contains("<<<<<<<") {
                        // Use the full LLM response as a hint — write the response content
                        // This is best-effort for built-in LLM
                        if let Some(resolved) = extract_file_content(&response.content, file) {
                            let _ = std::fs::write(&file_path, resolved);
                        }
                    }
                }
            }

            let msg = format!("Resolve merge conflict for {branch} (AI-assisted)");
            git_ops::complete_merge(working_dir, &msg).await?;
            Ok(())
        }
    }
}

/// Try to extract resolved file content from LLM response for a specific file.
fn extract_file_content(response: &str, filename: &str) -> Option<String> {
    // Look for a code block labeled with the filename or following a mention of it
    let patterns = [
        format!("```\n"), // generic code block
        format!("```{}\n", filename.rsplit('/').next().unwrap_or(filename)),
    ];
    for pattern in &patterns {
        if let Some(start) = response.find(pattern.as_str()) {
            let content_start = start + pattern.len();
            if let Some(end) = response[content_start..].find("```") {
                return Some(response[content_start..content_start + end].to_string());
            }
        }
    }
    None
}

/// Run an integration review after all branches are merged.
/// Streams output via "integration-review-chunk" events.
pub async fn run_integration_review(
    plan_id: &str,
    db: &Mutex<Database>,
    app_handle: &AppHandle,
) -> Result<String, String> {
    info!(plan_id, "Starting integration review");
    let (_project_id, plan, pieces, connections, summaries, settings) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let plan = db.get_work_plan(plan_id)?;
        let project = db.get_project(&plan.project_id)?;
        let pieces = db.list_pieces(&plan.project_id)?;
        let connections = db.list_connections(&plan.project_id)?;

        // Load all context summaries
        let summaries: Vec<(String, String)> = pieces.iter()
            .filter_map(|p| {
                db.get_artifact_by_type(&p.id, "context_summary")
                    .ok()
                    .flatten()
                    .map(|a| (p.name.clone(), a.content))
            })
            .collect();

        (plan.project_id.clone(), plan, pieces, connections, summaries, project.settings)
    };

    let (provider_name, api_key, model, base_url) = resolve_llm_config(&settings);
    if api_key.is_empty() {
        return Err("No API key available for integration review.".to_string());
    }

    // Build the integration review prompt
    let system = "You are an integration reviewer for a software project. Multiple components were \
        developed independently on separate branches and have now been merged into main.\n\n\
        Evaluate whether they work together correctly. Check for:\n\
        - API contract consistency (do callers match what providers expose?)\n\
        - Data type compatibility across boundaries\n\
        - Missing integrations (connections defined but not implemented)\n\
        - Configuration consistency (ports, env vars, URLs)\n\
        - Error handling at boundaries\n\n\
        Be specific. Reference file paths and function names. Flag severity: critical / warning / info.";

    let mut user_parts: Vec<String> = Vec::new();

    // Merged branches
    let merged_names: Vec<&str> = plan.tasks.iter()
        .map(|t| t.piece_name.as_str())
        .collect();
    user_parts.push(format!("## Branches Merged\n{}", merged_names.join(", ")));

    // Components
    let mut comp_parts: Vec<String> = Vec::new();
    for p in &pieces {
        let ptype = if p.piece_type.is_empty() { "component" } else { &p.piece_type };
        let mut desc = format!("### {} ({})\n", p.name, ptype);
        desc.push_str(&format!("Responsibilities: {}\n", p.responsibilities));
        if !p.interfaces.is_empty() {
            desc.push_str("Interfaces:\n");
            for iface in &p.interfaces {
                desc.push_str(&format!("  - {} ({:?}): {}\n", iface.name, iface.direction, iface.description));
            }
        }
        if !p.constraints.is_empty() {
            desc.push_str("Constraints:\n");
            for c in &p.constraints {
                desc.push_str(&format!("  - [{}] {}\n", c.category, c.description));
            }
        }
        // Add context summary if available
        if let Some((_, summary)) = summaries.iter().find(|(name, _)| name == &p.name) {
            desc.push_str(&format!("\nContext summary:\n{}\n", summary));
        }
        comp_parts.push(desc);
    }
    user_parts.push(format!("## Components\n{}", comp_parts.join("\n")));

    // Connections
    let piece_name_map: std::collections::HashMap<&str, &str> = pieces.iter()
        .map(|p| (p.id.as_str(), p.name.as_str()))
        .collect();
    let mut conn_parts: Vec<String> = Vec::new();
    for c in &connections {
        let src = piece_name_map.get(c.source_piece_id.as_str()).unwrap_or(&"?");
        let tgt = piece_name_map.get(c.target_piece_id.as_str()).unwrap_or(&"?");
        let mut desc = format!("{} → {} ({})", src, tgt, c.label);
        if !c.notes.is_empty() {
            desc.push_str(&format!(" — {}", c.notes));
        }
        conn_parts.push(desc);
    }
    user_parts.push(format!("## Connections\n{}", conn_parts.join("\n")));

    let user_content = user_parts.join("\n\n");
    let messages = vec![
        Message { role: "system".into(), content: system.to_string() },
        Message { role: "user".into(), content: user_content },
    ];

    // Stream the review
    let provider = llm::create_provider(&provider_name);
    let config = LlmConfig { api_key, model, base_url, max_tokens: 4096 };

    let (tx, mut rx) = mpsc::channel::<String>(256);
    let plan_id_owned = plan_id.to_string();
    let app = app_handle.clone();
    let full_output = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
    let full_output_writer = full_output.clone();

    let stream_handle = tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            full_output_writer.lock().await.push_str(&chunk);
            let _ = app.emit("integration-review-chunk", json!({
                "planId": plan_id_owned,
                "chunk": chunk,
                "done": false,
            }));
        }
    });

    let _usage = provider.chat_stream(&messages, &config, tx).await?;
    let _ = stream_handle.await;

    let review_text = full_output.lock().await.clone();
    info!(plan_id, review_length = review_text.len(), "Integration review complete");

    // Store the review in the plan
    {
        let db = db.lock().map_err(|e| e.to_string())?;
        db.update_work_plan(plan_id, &WorkPlanUpdate {
            integration_review: Some(review_text.clone()),
            ..Default::default()
        })?;
    }

    // Emit done
    let _ = app_handle.emit("integration-review-chunk", json!({
        "planId": plan_id,
        "chunk": "",
        "done": true,
    }));

    Ok(review_text)
}

fn emit_progress(
    app: &AppHandle,
    plan_id: &str,
    piece_name: &str,
    branch: &str,
    status: &str,
    message: &str,
    current: usize,
    total: usize,
) {
    if let Err(e) = app.emit("merge-progress", MergeProgress {
        plan_id: plan_id.to_string(),
        piece_name: piece_name.to_string(),
        branch: branch.to_string(),
        status: status.to_string(),
        message: message.to_string(),
        current,
        total,
    }) {
        warn!(error = %e, "Failed to emit merge-progress event");
    }
}

async fn diff_stat_if_merged(working_dir: &str, before_sha: &str) -> String {
    git_ops::diff_stat_between(working_dir, before_sha, "HEAD")
        .await
        .unwrap_or_default()
}
