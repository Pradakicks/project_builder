use rusqlite::params;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::Database;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHistoryEntry {
    pub id: String,
    pub agent_id: String,
    pub action: String,
    pub input_text: String,
    pub output_text: String,
    pub tokens_used: i64,
    pub created_at: String,
}

impl Database {
    /// Insert a history entry (uses piece_id as agent_id for MVP 1 simplicity)
    pub fn insert_agent_history(
        &self,
        piece_id: &str,
        action: &str,
        input_text: &str,
        output_text: &str,
        tokens_used: i64,
    ) -> Result<String, String> {
        debug!(piece_id, action, tokens_used, "Inserting agent history");
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        // Ensure an agent row exists for this piece
        self.conn
            .execute(
                "INSERT OR IGNORE INTO agents (id, piece_id, role, state, token_budget, token_usage, created_at, updated_at) VALUES (?1, ?2, 'implementation', 'idle', 0, 0, ?3, ?4)",
                params![piece_id, piece_id, now, now],
            )
            .map_err(|e| e.to_string())?;

        // Update token usage
        self.conn
            .execute(
                "UPDATE agents SET token_usage = token_usage + ?1, updated_at = ?2 WHERE id = ?3",
                params![tokens_used, now, piece_id],
            )
            .map_err(|e| e.to_string())?;

        self.conn
            .execute(
                "INSERT INTO agent_history (id, agent_id, action, input_text, output_text, tokens_used, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, piece_id, action, input_text, output_text, tokens_used, now],
            )
            .map_err(|e| e.to_string())?;

        info!(piece_id, history_id = %id, action, tokens_used, "Agent history recorded");
        Ok(id)
    }

    /// List history entries for a piece
    pub fn list_agent_history(&self, piece_id: &str) -> Result<Vec<AgentHistoryEntry>, String> {
        debug!(piece_id, "Listing agent history");
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, agent_id, action, input_text, output_text, tokens_used, created_at FROM agent_history WHERE agent_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![piece_id], |row| {
                Ok(AgentHistoryEntry {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    action: row.get(2)?,
                    input_text: row.get(3)?,
                    output_text: row.get(4)?,
                    tokens_used: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Get total token usage for a piece
    pub fn get_piece_token_usage(&self, piece_id: &str) -> Result<i64, String> {
        self.conn
            .query_row(
                "SELECT COALESCE(SUM(tokens_used), 0) FROM agent_history WHERE agent_id = ?1",
                params![piece_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())
    }
}
