use crate::models::{AgentRecord, AgentRole, AgentState};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tracing::{debug, info};

use super::Database;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    pub command: String,
    pub passed: bool,
    pub exit_code: i32,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentHistoryMetadata {
    pub usage: Option<crate::llm::TokenUsage>,
    pub success: Option<bool>,
    pub exit_code: Option<i32>,
    pub phase_proposal: Option<String>,
    pub phase_changed: Option<String>,
    pub git_branch: Option<String>,
    pub git_commit_sha: Option<String>,
    pub git_diff_stat: Option<String>,
    pub validation: Option<ValidationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHistoryEntry {
    pub id: String,
    pub agent_id: String,
    pub action: String,
    pub input_text: String,
    pub output_text: String,
    #[serde(default)]
    pub metadata: AgentHistoryMetadata,
    pub tokens_used: i64,
    pub created_at: String,
    /// Role that produced this history row. Defaults to `implementation` for
    /// legacy rows via the tail-migration `DEFAULT 'implementation'`.
    #[serde(default = "default_history_role")]
    pub role: AgentRole,
}

fn default_history_role() -> AgentRole {
    AgentRole::Implementation
}

impl Database {
    /// Create (or return) the `agents` row for this (piece_id, role) pair.
    /// Deterministic id = `"{piece_id}:{role}"` so it's stable and debuggable,
    /// and the UNIQUE (piece_id, role) index catches any accidental duplicates.
    pub fn upsert_agent(&self, piece_id: &str, role: AgentRole) -> Result<String, String> {
        let id = format!("{piece_id}:{role}");
        let now = chrono::Utc::now().to_rfc3339();
        self.conn
            .execute(
                "INSERT OR IGNORE INTO agents (id, piece_id, role, state, token_budget, token_usage, created_at, updated_at) VALUES (?1, ?2, ?3, 'idle', 0, 0, ?4, ?5)",
                params![id, piece_id, role.as_str(), now, now],
            )
            .map_err(|e| e.to_string())?;
        Ok(id)
    }

    /// Transition the state of a specific (piece_id, role) agent row.
    pub fn set_agent_state(
        &self,
        piece_id: &str,
        role: AgentRole,
        state: AgentState,
    ) -> Result<(), String> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE agents SET state = ?1, updated_at = ?2 WHERE piece_id = ?3 AND role = ?4",
                params![state.as_str(), now, piece_id, role.as_str()],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// List every role's agent row for a piece. Used by the Agents panel and
    /// the per-role canvas indicators.
    pub fn list_agents_for_piece(&self, piece_id: &str) -> Result<Vec<AgentRecord>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, piece_id, role, state, token_budget, token_usage, provider, created_at, updated_at \
                 FROM agents WHERE piece_id = ?1 ORDER BY role ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![piece_id], |row| {
                let role: String = row.get(2)?;
                let state: String = row.get(3)?;
                Ok(AgentRecord {
                    id: row.get(0)?,
                    piece_id: row.get(1)?,
                    role: AgentRole::from_str(&role).unwrap_or(AgentRole::Implementation),
                    state: AgentState::from_str(&state).unwrap_or(AgentState::Idle),
                    token_budget: row.get(4)?,
                    token_usage: row.get(5)?,
                    provider: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Insert a history entry tagged with the producing role. Rows point at the
    /// role-specific agents row (via upsert_agent's deterministic id) so the
    /// `agent_id` FK is satisfied and queries can JOIN back to the role.
    ///
    /// Legacy rows inserted before Phase 1 stored `agent_id = piece_id` and
    /// relied on a sibling `agents` row with that same id. Those keep working
    /// because list_* queries go via `agents.piece_id`, which legacy rows
    /// still satisfy.
    pub fn insert_agent_history(
        &self,
        piece_id: &str,
        role: AgentRole,
        action: &str,
        input_text: &str,
        output_text: &str,
        metadata: Option<&AgentHistoryMetadata>,
        tokens_used: i64,
    ) -> Result<String, String> {
        debug!(
            piece_id,
            role = role.as_str(),
            action,
            tokens_used,
            "Inserting agent history"
        );
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let metadata_json =
            serde_json::to_string(metadata.unwrap_or(&AgentHistoryMetadata::default()))
                .map_err(|e| e.to_string())?;

        // Ensure the (piece_id, role) agent row exists; use its id as the FK
        // target. Also bump its token usage counter.
        let agent_id = self.upsert_agent(piece_id, role)?;
        self.conn
            .execute(
                "UPDATE agents SET token_usage = token_usage + ?1, updated_at = ?2 WHERE id = ?3",
                params![tokens_used, now, agent_id],
            )
            .map_err(|e| e.to_string())?;

        self.conn
            .execute(
                "INSERT INTO agent_history (id, agent_id, action, input_text, output_text, metadata_json, tokens_used, created_at, role) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    agent_id,
                    action,
                    input_text,
                    output_text,
                    metadata_json,
                    tokens_used,
                    now,
                    role.as_str(),
                ],
            )
            .map_err(|e| e.to_string())?;

        info!(
            piece_id,
            role = role.as_str(),
            history_id = %id,
            action,
            tokens_used,
            "Agent history recorded"
        );
        Ok(id)
    }

    /// List history entries for a piece (all roles), newest first. Joins via
    /// `agents` so we pick up both new role-specific rows and legacy rows
    /// whose agent_id equalled piece_id.
    pub fn list_agent_history(&self, piece_id: &str) -> Result<Vec<AgentHistoryEntry>, String> {
        debug!(piece_id, "Listing agent history");
        let mut stmt = self
            .conn
            .prepare(
                "SELECT h.id, h.agent_id, h.action, h.input_text, h.output_text, h.metadata_json, \
                        h.tokens_used, h.created_at, h.role \
                 FROM agent_history h JOIN agents a ON a.id = h.agent_id \
                 WHERE a.piece_id = ?1 \
                 ORDER BY h.created_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![piece_id], |row| {
                let metadata_json: String = row.get(5)?;
                let role_str: String = row.get(8)?;
                Ok(AgentHistoryEntry {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    action: row.get(2)?,
                    input_text: row.get(3)?,
                    output_text: row.get(4)?,
                    metadata: serde_json::from_str(&metadata_json).unwrap_or_default(),
                    tokens_used: row.get(6)?,
                    created_at: row.get(7)?,
                    role: AgentRole::from_str(&role_str).unwrap_or(AgentRole::Implementation),
                })
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// History for a specific (piece_id, role) pair — used by the per-role UI
    /// panels so each role has its own timeline.
    pub fn list_agent_history_by_role(
        &self,
        piece_id: &str,
        role: AgentRole,
    ) -> Result<Vec<AgentHistoryEntry>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT h.id, h.agent_id, h.action, h.input_text, h.output_text, h.metadata_json, \
                        h.tokens_used, h.created_at, h.role \
                 FROM agent_history h JOIN agents a ON a.id = h.agent_id \
                 WHERE a.piece_id = ?1 AND h.role = ?2 \
                 ORDER BY h.created_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![piece_id, role.as_str()], |row| {
                let metadata_json: String = row.get(5)?;
                let role_str: String = row.get(8)?;
                Ok(AgentHistoryEntry {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    action: row.get(2)?,
                    input_text: row.get(3)?,
                    output_text: row.get(4)?,
                    metadata: serde_json::from_str(&metadata_json).unwrap_or_default(),
                    tokens_used: row.get(6)?,
                    created_at: row.get(7)?,
                    role: AgentRole::from_str(&role_str).unwrap_or(AgentRole::Implementation),
                })
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Get total token usage for a piece (all roles combined).
    pub fn get_piece_token_usage(&self, piece_id: &str) -> Result<i64, String> {
        self.conn
            .query_row(
                "SELECT COALESCE(SUM(h.tokens_used), 0) \
                 FROM agent_history h JOIN agents a ON a.id = h.agent_id \
                 WHERE a.piece_id = ?1",
                params![piece_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())
    }
}
