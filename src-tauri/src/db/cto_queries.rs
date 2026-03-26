use crate::models::CtoDecision;

use super::Database;

impl Database {
    pub fn insert_cto_decision(
        &self,
        project_id: &str,
        summary: &str,
        actions_json: &str,
    ) -> Result<CtoDecision, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        self.conn
            .execute(
                "INSERT INTO cto_decisions (id, project_id, summary, actions_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, project_id, summary, actions_json, now],
            )
            .map_err(|e| e.to_string())?;

        Ok(CtoDecision {
            id,
            project_id: project_id.to_string(),
            summary: summary.to_string(),
            actions_json: actions_json.to_string(),
            created_at: now,
        })
    }

    pub fn list_cto_decisions(&self, project_id: &str) -> Result<Vec<CtoDecision>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, project_id, summary, actions_json, created_at FROM cto_decisions WHERE project_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(rusqlite::params![project_id], |row| {
                Ok(CtoDecision {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    summary: row.get(2)?,
                    actions_json: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }
}
