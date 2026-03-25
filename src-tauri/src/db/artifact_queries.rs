use rusqlite::params;

use super::Database;
use crate::models::Artifact;

impl Database {
    /// Create or update an artifact. If a matching (piece_id, artifact_type) exists,
    /// overwrite content and bump version. Otherwise insert new.
    pub fn upsert_artifact(
        &self,
        piece_id: &str,
        artifact_type: &str,
        title: &str,
        content: &str,
    ) -> Result<Artifact, String> {
        let now = chrono::Utc::now().to_rfc3339();

        // Check if one already exists
        let existing: Option<(String, i32)> = self
            .conn
            .query_row(
                "SELECT id, version FROM artifacts WHERE piece_id = ?1 AND artifact_type = ?2",
                params![piece_id, artifact_type],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let id = if let Some((existing_id, version)) = existing {
            self.conn
                .execute(
                    "UPDATE artifacts SET title = ?1, content = ?2, version = ?3, updated_at = ?4 WHERE id = ?5",
                    params![title, content, version + 1, now, existing_id],
                )
                .map_err(|e| e.to_string())?;
            existing_id
        } else {
            let new_id = uuid::Uuid::new_v4().to_string();
            self.conn
                .execute(
                    "INSERT INTO artifacts (id, piece_id, agent_id, artifact_type, title, content, review_status, version, created_at, updated_at) VALUES (?1, ?2, NULL, ?3, ?4, ?5, 'draft', 1, ?6, ?7)",
                    params![new_id, piece_id, artifact_type, title, content, now, now],
                )
                .map_err(|e| e.to_string())?;
            new_id
        };

        self.get_artifact(&id)
    }

    /// Get an artifact by ID.
    pub fn get_artifact(&self, id: &str) -> Result<Artifact, String> {
        self.conn
            .query_row(
                "SELECT id, piece_id, agent_id, artifact_type, title, content, review_status, version, created_at, updated_at FROM artifacts WHERE id = ?1",
                params![id],
                |row| {
                    Ok(Artifact {
                        id: row.get(0)?,
                        piece_id: row.get(1)?,
                        agent_id: row.get(2)?,
                        artifact_type: row.get(3)?,
                        title: row.get(4)?,
                        content: row.get(5)?,
                        review_status: serde_json::from_str(&format!("\"{}\"", row.get::<_, String>(6)?))
                            .unwrap_or(crate::models::artifact::ReviewStatus::Draft),
                        version: row.get(7)?,
                        created_at: row.get(8)?,
                        updated_at: row.get(9)?,
                    })
                },
            )
            .map_err(|e| format!("Artifact not found: {e}"))
    }

    /// Get the latest artifact of a given type for a piece.
    pub fn get_artifact_by_type(
        &self,
        piece_id: &str,
        artifact_type: &str,
    ) -> Result<Option<Artifact>, String> {
        let result = self.conn.query_row(
            "SELECT id, piece_id, agent_id, artifact_type, title, content, review_status, version, created_at, updated_at FROM artifacts WHERE piece_id = ?1 AND artifact_type = ?2",
            params![piece_id, artifact_type],
            |row| {
                Ok(Artifact {
                    id: row.get(0)?,
                    piece_id: row.get(1)?,
                    agent_id: row.get(2)?,
                    artifact_type: row.get(3)?,
                    title: row.get(4)?,
                    content: row.get(5)?,
                    review_status: serde_json::from_str(&format!("\"{}\"", row.get::<_, String>(6)?))
                        .unwrap_or(crate::models::artifact::ReviewStatus::Draft),
                    version: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        );

        match result {
            Ok(artifact) => Ok(Some(artifact)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// List all artifacts for a piece.
    pub fn list_artifacts(&self, piece_id: &str) -> Result<Vec<Artifact>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, piece_id, agent_id, artifact_type, title, content, review_status, version, created_at, updated_at FROM artifacts WHERE piece_id = ?1 ORDER BY updated_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![piece_id], |row| {
                Ok(Artifact {
                    id: row.get(0)?,
                    piece_id: row.get(1)?,
                    agent_id: row.get(2)?,
                    artifact_type: row.get(3)?,
                    title: row.get(4)?,
                    content: row.get(5)?,
                    review_status: serde_json::from_str(&format!("\"{}\"", row.get::<_, String>(6)?))
                        .unwrap_or(crate::models::artifact::ReviewStatus::Draft),
                    version: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
}
