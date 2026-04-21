use serde::{Deserialize, Serialize};

/// One row in the `team_briefs` table. There's at most one brief per
/// `(project_id, team)` pair — regeneration updates in place. The brief is
/// LLM-generated, consumed by pieces in OTHER teams via `PieceContext`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamBrief {
    pub team: String,
    pub project_id: String,
    pub content: String,
    /// Snapshot of the piece IDs that contributed to this brief — useful for
    /// auditing "which team members did the LLM see at brief time?" in the
    /// debug report. Stored as JSON in the DB.
    #[serde(default)]
    pub member_piece_ids: Vec<String>,
    #[serde(default)]
    pub tokens_used: i64,
    pub updated_at: String,
}
