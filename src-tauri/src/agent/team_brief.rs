//! Team-brief generation. When a piece in a team finishes running, we
//! opportunistically regenerate the team's brief — a short LLM-summarised
//! snapshot of what the team owns, what interfaces it exposes, what
//! constraints matter, and what's currently blocked. Other teams' pieces read
//! these briefs at prompt-build time so each piece sees the state of the
//! whole project, not just its direct connections.
//!
//! Debounced at 5 minutes per team to avoid thrash when several team members
//! run in quick succession during a work plan execution.

use crate::agent::runner::resolve_llm_config;
use crate::llm::{self, LlmConfig, Message};
use crate::models::Piece;
use crate::AppState;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tauri::{AppHandle, Manager};
use tracing::{debug, info, warn};

/// Minimum seconds between two brief regenerations for the same team. A second
/// call within the window no-ops — the existing brief stays.
pub(crate) const TEAM_BRIEF_DEBOUNCE_SECS: i64 = 300; // 5 minutes

/// Generate (or regenerate) the brief for a team. Idempotent + debounced.
/// Returns `Ok(true)` if a new brief was written, `Ok(false)` if we skipped
/// (debounce or empty team), `Err` on hard failure.
///
/// Intended use: fire-and-forget from `run_piece_agent` after a successful
/// piece run. The caller should `tokio::spawn` and log the error, not
/// propagate it into the piece-run result.
pub async fn generate_team_brief<R: tauri::Runtime>(
    team: String,
    project_id: String,
    app_handle: AppHandle<R>,
) -> Result<bool, String> {
    debug!(team, project_id, "Considering team-brief regeneration");

    let state = app_handle
        .try_state::<AppState>()
        .ok_or_else(|| "AppState unavailable (app teardown?)".to_string())?;
    let db = &state.db;

    // Debounce: if the existing brief is fresh enough, no-op.
    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = db_lock.get_team_brief(&project_id, &team)? {
            if !should_regenerate(&existing.updated_at, TEAM_BRIEF_DEBOUNCE_SECS) {
                debug!(team, "Brief is fresh; skipping regeneration");
                return Ok(false);
            }
        }
    }

    // Gather team members. Empty team = nothing to brief.
    let (members, project_settings) = {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        let project = db_lock.get_project(&project_id)?;
        let pieces = db_lock.list_pieces(&project_id)?;
        let members: Vec<Piece> = pieces
            .into_iter()
            .filter(|p| {
                p.agent_config
                    .team
                    .as_deref()
                    .map(|t| t == team)
                    .unwrap_or(false)
            })
            .collect();
        (members, project.settings)
    };

    if members.is_empty() {
        debug!(team, "Team has no members; no brief to write");
        return Ok(false);
    }

    // Per-member context summary excerpts — grounded, recent.
    let mut member_snippets: Vec<String> = Vec::with_capacity(members.len());
    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        for piece in &members {
            let interfaces = piece
                .interfaces
                .iter()
                .map(|i| format!("  - {} ({:?}): {}", i.name, i.direction, i.description))
                .collect::<Vec<_>>()
                .join("\n");
            let constraints = piece
                .constraints
                .iter()
                .map(|c| format!("  - [{}] {}", c.category, c.description))
                .collect::<Vec<_>>()
                .join("\n");
            let summary_excerpt = db_lock
                .get_artifact_by_type(&piece.id, "context_summary")
                .ok()
                .flatten()
                .map(|a| snip_for_brief(&a.content, 800))
                .unwrap_or_else(|| "(no context summary yet)".to_string());

            let mut section = format!(
                "## {} ({})\nResponsibilities:\n{}\n",
                piece.name, piece.piece_type, piece.responsibilities
            );
            if !interfaces.is_empty() {
                section.push_str(&format!("Interfaces:\n{}\n", interfaces));
            }
            if !constraints.is_empty() {
                section.push_str(&format!("Constraints:\n{}\n", constraints));
            }
            section.push_str(&format!("Latest summary:\n{}\n", summary_excerpt));
            member_snippets.push(section);
        }
    }

    let member_ids: Vec<String> = members.iter().map(|p| p.id.clone()).collect();

    let (provider_name, api_key, model, base_url) = resolve_llm_config(&project_settings);
    if api_key.is_empty() {
        // Not a hard error — some projects run entirely on external CLI
        // engines and haven't set a built-in provider key. Skip quietly.
        info!(team, "No API key for team-brief summarizer; skipping");
        return Ok(false);
    }

    let system_msg = r#"You are the TEAM BRIEF agent. Produce a concise operational summary of a single team's work. Other teams' agents will read this as context when they run, so every word needs to carry weight.

Required sections (in this order, use exactly these headers):
## Owned surface
## Exposed interfaces
## Key constraints
## Current blockers
## Recent decisions

Be specific — name files, endpoint paths, event names, data types. Avoid filler. Aim for ~200 words total. No preamble."#;

    let user_content = format!(
        "Team: {}\nProject members ({}): see below.\n\n{}",
        team,
        members.len(),
        member_snippets.join("\n\n")
    );

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
        max_tokens: 800,
    };

    let response = match provider.chat(&messages, &config).await {
        Ok(r) => r,
        Err(e) => {
            warn!(team, error = %e, "Team brief LLM call failed");
            return Err(e);
        }
    };

    let content = response.content.trim().to_string();
    let tokens_used = (response.tokens_used.input + response.tokens_used.output) as i64;

    {
        let db_lock = db.lock().map_err(|e| e.to_string())?;
        db_lock.upsert_team_brief(&project_id, &team, &content, &member_ids, tokens_used)?;
    }

    info!(
        team,
        members = member_ids.len(),
        tokens_used,
        "Team brief regenerated"
    );
    Ok(true)
}

fn should_regenerate(updated_at: &str, min_age_secs: i64) -> bool {
    let Ok(parsed) = DateTime::parse_from_rfc3339(updated_at) else {
        // If we can't parse the timestamp, play it safe and regenerate.
        return true;
    };
    let now = Utc::now();
    now.signed_duration_since(parsed.with_timezone(&Utc)) >= ChronoDuration::seconds(min_age_secs)
}

fn snip_for_brief(s: &str, max_chars: usize) -> String {
    let total = s.chars().count();
    if total <= max_chars {
        return s.to_string();
    }
    let kept: String = s.chars().take(max_chars).collect();
    format!("{kept}…[truncated {} more chars]", total - max_chars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce_skips_fresh_brief() {
        let now = Utc::now().to_rfc3339();
        assert!(!should_regenerate(&now, TEAM_BRIEF_DEBOUNCE_SECS));
    }

    #[test]
    fn debounce_regenerates_after_window() {
        let stale = (Utc::now() - ChronoDuration::seconds(TEAM_BRIEF_DEBOUNCE_SECS + 60))
            .to_rfc3339();
        assert!(should_regenerate(&stale, TEAM_BRIEF_DEBOUNCE_SECS));
    }

    #[test]
    fn debounce_regenerates_on_bad_timestamp() {
        // Unparseable timestamp should regenerate rather than silently no-op.
        assert!(should_regenerate("not-a-date", TEAM_BRIEF_DEBOUNCE_SECS));
    }

    #[test]
    fn snip_truncates_with_marker() {
        let long = "x".repeat(1000);
        let snipped = snip_for_brief(&long, 100);
        assert!(snipped.contains("…[truncated"));
        assert!(snipped.len() < long.len());
    }

    #[test]
    fn snip_passes_through_short() {
        let short = "hello";
        assert_eq!(snip_for_brief(short, 100), "hello");
    }
}
