use rusqlite::params;
use tracing::{debug, info};

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
        debug!(piece_id, artifact_type, title, "Upserting artifact");
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
            info!(piece_id, artifact_type, artifact_id = %existing_id, "Artifact upserted");
            existing_id
        } else {
            let new_id = uuid::Uuid::new_v4().to_string();
            self.conn
                .execute(
                    "INSERT INTO artifacts (id, piece_id, agent_id, artifact_type, title, content, review_status, version, created_at, updated_at) VALUES (?1, ?2, NULL, ?3, ?4, ?5, 'draft', 1, ?6, ?7)",
                    params![new_id, piece_id, artifact_type, title, content, now, now],
                )
                .map_err(|e| e.to_string())?;
            info!(piece_id, artifact_type, artifact_id = %new_id, "Artifact upserted");
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

    /// Set the review_status for an existing artifact identified by (piece_id, artifact_type).
    /// If no such artifact exists, logs a debug message and returns Ok(()) silently.
    pub fn set_artifact_review_status(
        &self,
        piece_id: &str,
        artifact_type: &str,
        status: crate::models::artifact::ReviewStatus,
    ) -> Result<(), String> {
        let existing_id: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM artifacts WHERE piece_id = ?1 AND artifact_type = ?2",
                params![piece_id, artifact_type],
                |row| row.get(0),
            )
            .ok();

        let Some(id) = existing_id else {
            debug!(
                piece_id,
                artifact_type, "No artifact found for review status update; no-op"
            );
            return Ok(());
        };

        let now = chrono::Utc::now().to_rfc3339();
        let status_str = serde_json::to_string(&status)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        self.conn
            .execute(
                "UPDATE artifacts SET review_status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status_str, now, id],
            )
            .map_err(|e| e.to_string())?;

        info!(piece_id, artifact_type, artifact_id = %id, status = %status_str, "Artifact review status updated");
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::artifact::ReviewStatus;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn temp_db_path(case: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "project-builder-artifact-{case}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("create temp test directory");
        dir.join("data.db")
    }

    fn cleanup(db_path: &Path) {
        if let Some(parent) = db_path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn set_artifact_review_status_updates_existing_row() {
        let db_path = temp_db_path("set-review-status");
        let db = Database::new_at_path(&db_path).expect("open test db");
        let project = db
            .create_project("Artifact project", "Testing review status")
            .expect("create project");
        let piece = db
            .create_piece(&project.id, None, "root", 0.0, 0.0)
            .expect("create piece");

        db.upsert_artifact(&piece.id, "generated_files", "Gen", "{}")
            .expect("upsert artifact");

        db.set_artifact_review_status(&piece.id, "generated_files", ReviewStatus::Approved)
            .expect("set review status");

        let fetched = db
            .get_artifact_by_type(&piece.id, "generated_files")
            .expect("get artifact")
            .expect("artifact exists");
        assert!(matches!(fetched.review_status, ReviewStatus::Approved));

        cleanup(&db_path);
    }

    #[test]
    fn set_artifact_review_status_on_missing_is_noop() {
        let db_path = temp_db_path("set-review-status-missing");
        let db = Database::new_at_path(&db_path).expect("open test db");
        let project = db
            .create_project("Artifact project", "Testing review status missing")
            .expect("create project");
        let piece = db
            .create_piece(&project.id, None, "root", 0.0, 0.0)
            .expect("create piece");

        let result =
            db.set_artifact_review_status(&piece.id, "nonexistent", ReviewStatus::Rejected);
        assert!(result.is_ok());

        cleanup(&db_path);
    }
}
