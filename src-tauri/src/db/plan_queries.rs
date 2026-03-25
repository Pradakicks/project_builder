use crate::models::*;
use rusqlite::params;

use super::Database;

impl Database {
    pub fn create_work_plan(
        &self,
        project_id: &str,
        user_guidance: &str,
    ) -> Result<WorkPlan, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        // version = max existing + 1
        let max_version: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM work_plans WHERE project_id = ?1",
                params![project_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())?;
        let version = max_version + 1;

        self.conn
            .execute(
                "INSERT INTO work_plans (id, project_id, version, status, summary, user_guidance, tasks_json, raw_output, tokens_used, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![id, project_id, version, "generating", "", user_guidance, "[]", "", 0, now, now],
            )
            .map_err(|e| e.to_string())?;

        Ok(WorkPlan {
            id,
            project_id: project_id.to_string(),
            version,
            status: PlanStatus::Generating,
            summary: String::new(),
            user_guidance: user_guidance.to_string(),
            tasks: vec![],
            raw_output: String::new(),
            tokens_used: 0,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn get_work_plan(&self, id: &str) -> Result<WorkPlan, String> {
        self.conn
            .query_row(
                "SELECT id, project_id, version, status, summary, user_guidance, tasks_json, raw_output, tokens_used, created_at, updated_at FROM work_plans WHERE id = ?1",
                params![id],
                |row| {
                    let status_str: String = row.get(3)?;
                    let tasks_json: String = row.get(6)?;

                    Ok(WorkPlan {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        version: row.get(2)?,
                        status: serde_json::from_str(&format!("\"{}\"", status_str))
                            .unwrap_or(PlanStatus::Draft),
                        summary: row.get(4)?,
                        user_guidance: row.get(5)?,
                        tasks: serde_json::from_str(&tasks_json).unwrap_or_default(),
                        raw_output: row.get(7)?,
                        tokens_used: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                    })
                },
            )
            .map_err(|e| e.to_string())
    }

    pub fn update_work_plan(
        &self,
        id: &str,
        updates: &WorkPlanUpdate,
    ) -> Result<WorkPlan, String> {
        let now = chrono::Utc::now().to_rfc3339();

        if let Some(ref status) = updates.status {
            let json = serde_json::to_string(status).map_err(|e| e.to_string())?;
            let val = json.trim_matches('"');
            self.conn
                .execute(
                    "UPDATE work_plans SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    params![val, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref summary) = updates.summary {
            self.conn
                .execute(
                    "UPDATE work_plans SET summary = ?1, updated_at = ?2 WHERE id = ?3",
                    params![summary, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref tasks) = updates.tasks {
            let json = serde_json::to_string(tasks).map_err(|e| e.to_string())?;
            self.conn
                .execute(
                    "UPDATE work_plans SET tasks_json = ?1, updated_at = ?2 WHERE id = ?3",
                    params![json, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref raw_output) = updates.raw_output {
            self.conn
                .execute(
                    "UPDATE work_plans SET raw_output = ?1, updated_at = ?2 WHERE id = ?3",
                    params![raw_output, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(tokens_used) = updates.tokens_used {
            self.conn
                .execute(
                    "UPDATE work_plans SET tokens_used = ?1, updated_at = ?2 WHERE id = ?3",
                    params![tokens_used, now, id],
                )
                .map_err(|e| e.to_string())?;
        }

        self.get_work_plan(id)
    }

    pub fn list_work_plans(&self, project_id: &str) -> Result<Vec<WorkPlan>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM work_plans WHERE project_id = ?1 ORDER BY version DESC")
            .map_err(|e| e.to_string())?;

        let ids: Vec<String> = stmt
            .query_map(params![project_id], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        ids.iter().map(|id| self.get_work_plan(id)).collect()
    }

    pub fn get_latest_work_plan(
        &self,
        project_id: &str,
    ) -> Result<Option<WorkPlan>, String> {
        let id: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM work_plans WHERE project_id = ?1 ORDER BY version DESC LIMIT 1",
                params![project_id],
                |row| row.get(0),
            )
            .ok();

        match id {
            Some(id) => self.get_work_plan(&id).map(Some),
            None => Ok(None),
        }
    }

    pub fn delete_work_plan(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM work_plans WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Mark all draft plans for a project as superseded
    pub fn supersede_draft_plans(&self, project_id: &str) -> Result<(), String> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE work_plans SET status = 'superseded', updated_at = ?1 WHERE project_id = ?2 AND status = 'draft'",
                params![now, project_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
