use crate::models::*;
use rusqlite::params;
use tracing::{debug, info};

use super::Database;

fn enum_to_sql<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value)
        .map_err(|e| e.to_string())
        .map(|json| json.trim_matches('"').to_string())
}

fn sql_to_goal_run_phase(value: &str) -> GoalRunPhase {
    serde_json::from_str(&format!("\"{}\"", value)).unwrap_or(GoalRunPhase::PromptReceived)
}

fn sql_to_goal_run_status(value: &str) -> GoalRunStatus {
    serde_json::from_str(&format!("\"{}\"", value)).unwrap_or(GoalRunStatus::Running)
}

impl Database {
    pub fn create_goal_run(&self, project_id: &str, prompt: &str) -> Result<GoalRun, String> {
        debug!(project_id, "Creating goal run");

        if prompt.trim().is_empty() {
            return Err("Goal run prompt cannot be empty".to_string());
        }

        self.get_project(project_id)?;

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let phase = enum_to_sql(&GoalRunPhase::PromptReceived)?;
        let status = enum_to_sql(&GoalRunStatus::Running)?;

        self.conn
            .execute(
                "INSERT INTO goal_runs (id, project_id, prompt, phase, status, blocker_reason, current_plan_id, runtime_status_summary, verification_summary, retry_count, last_failure_summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, NULL, NULL, 0, NULL, ?6, ?7)",
                params![id, project_id, prompt.trim(), phase, status, now, now],
            )
            .map_err(|e| e.to_string())?;

        info!(goal_run_id = %id, project_id, "Goal run created");
        self.get_goal_run(&id)
    }

    pub fn get_goal_run(&self, id: &str) -> Result<GoalRun, String> {
        debug!(goal_run_id = id, "Getting goal run");
        self.conn
            .query_row(
                "SELECT id, project_id, prompt, phase, status, blocker_reason, current_plan_id, runtime_status_summary, verification_summary, retry_count, last_failure_summary, created_at, updated_at FROM goal_runs WHERE id = ?1",
                params![id],
                |row| {
                    let phase: String = row.get(3)?;
                    let status: String = row.get(4)?;
                    Ok(GoalRun {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        prompt: row.get(2)?,
                        phase: sql_to_goal_run_phase(&phase),
                        status: sql_to_goal_run_status(&status),
                        blocker_reason: row.get(5)?,
                        current_plan_id: row.get(6)?,
                        runtime_status_summary: row.get(7)?,
                        verification_summary: row.get(8)?,
                        retry_count: row.get(9)?,
                        last_failure_summary: row.get(10)?,
                        created_at: row.get(11)?,
                        updated_at: row.get(12)?,
                    })
                },
            )
            .map_err(|e| e.to_string())
    }

    pub fn list_goal_runs(&self, project_id: &str) -> Result<Vec<GoalRun>, String> {
        debug!(project_id, "Listing goal runs");
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id FROM goal_runs WHERE project_id = ?1 ORDER BY updated_at DESC, created_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let ids: Vec<String> = stmt
            .query_map(params![project_id], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        ids.iter().map(|id| self.get_goal_run(id)).collect()
    }

    pub fn update_goal_run(
        &self,
        id: &str,
        updates: &GoalRunUpdate,
    ) -> Result<GoalRun, String> {
        debug!(
            goal_run_id = id,
            has_prompt = updates.prompt.is_some(),
            has_phase = updates.phase.is_some(),
            has_status = updates.status.is_some(),
            "Updating goal run"
        );

        if let Some(ref prompt) = updates.prompt {
            if prompt.trim().is_empty() {
                return Err("Goal run prompt cannot be empty".to_string());
            }
        }
        if let Some(retry_count) = updates.retry_count {
            if retry_count < 0 {
                return Err("Goal run retry_count cannot be negative".to_string());
            }
        }

        let now = chrono::Utc::now().to_rfc3339();

        if let Some(ref prompt) = updates.prompt {
            self.conn
                .execute(
                    "UPDATE goal_runs SET prompt = ?1, updated_at = ?2 WHERE id = ?3",
                    params![prompt.trim(), now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref phase) = updates.phase {
            let phase = enum_to_sql(phase)?;
            self.conn
                .execute(
                    "UPDATE goal_runs SET phase = ?1, updated_at = ?2 WHERE id = ?3",
                    params![phase, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref status) = updates.status {
            let status = enum_to_sql(status)?;
            self.conn
                .execute(
                    "UPDATE goal_runs SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    params![status, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref blocker_reason) = updates.blocker_reason {
            self.conn
                .execute(
                    "UPDATE goal_runs SET blocker_reason = ?1, updated_at = ?2 WHERE id = ?3",
                    params![blocker_reason, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref current_plan_id) = updates.current_plan_id {
            self.conn
                .execute(
                    "UPDATE goal_runs SET current_plan_id = ?1, updated_at = ?2 WHERE id = ?3",
                    params![current_plan_id, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref runtime_status_summary) = updates.runtime_status_summary {
            self.conn
                .execute(
                    "UPDATE goal_runs SET runtime_status_summary = ?1, updated_at = ?2 WHERE id = ?3",
                    params![runtime_status_summary, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref verification_summary) = updates.verification_summary {
            self.conn
                .execute(
                    "UPDATE goal_runs SET verification_summary = ?1, updated_at = ?2 WHERE id = ?3",
                    params![verification_summary, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(retry_count) = updates.retry_count {
            self.conn
                .execute(
                    "UPDATE goal_runs SET retry_count = ?1, updated_at = ?2 WHERE id = ?3",
                    params![retry_count, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref last_failure_summary) = updates.last_failure_summary {
            self.conn
                .execute(
                    "UPDATE goal_runs SET last_failure_summary = ?1, updated_at = ?2 WHERE id = ?3",
                    params![last_failure_summary, now, id],
                )
                .map_err(|e| e.to_string())?;
        }

        self.get_goal_run(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn temp_db_path(case: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "project-builder-goal-run-{case}-{}",
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
    fn goal_run_roundtrips_and_updates_partial_state() {
        let db_path = temp_db_path("roundtrip");
        let db = Database::new_at_path(&db_path).expect("open test db");
        let project = db
            .create_project("Goal run project", "Testing goal runs")
            .expect("create project");

        let created = db
            .create_goal_run(&project.id, "Create a todo app")
            .expect("create goal run");
        assert_eq!(created.project_id, project.id);
        assert_eq!(created.prompt, "Create a todo app");
        assert_eq!(created.phase, GoalRunPhase::PromptReceived);
        assert_eq!(created.status, GoalRunStatus::Running);
        assert_eq!(created.retry_count, 0);
        assert!(created.blocker_reason.is_none());
        assert!(created.current_plan_id.is_none());

        let updated = db
            .update_goal_run(
                &created.id,
                &GoalRunUpdate {
                    phase: Some(GoalRunPhase::Planning),
                    status: Some(GoalRunStatus::Blocked),
                    blocker_reason: Some(Some("Waiting on runtime spec".to_string())),
                    current_plan_id: Some(Some("plan-123".to_string())),
                    runtime_status_summary: Some(Some("runtime not configured".to_string())),
                    verification_summary: Some(Some("verification not started".to_string())),
                    retry_count: Some(2),
                    last_failure_summary: Some(Some("missing command".to_string())),
                    ..Default::default()
                },
            )
            .expect("update goal run");

        assert_eq!(updated.phase, GoalRunPhase::Planning);
        assert_eq!(updated.status, GoalRunStatus::Blocked);
        assert_eq!(updated.blocker_reason.as_deref(), Some("Waiting on runtime spec"));
        assert_eq!(updated.current_plan_id.as_deref(), Some("plan-123"));
        assert_eq!(updated.runtime_status_summary.as_deref(), Some("runtime not configured"));
        assert_eq!(updated.verification_summary.as_deref(), Some("verification not started"));
        assert_eq!(updated.retry_count, 2);
        assert_eq!(updated.last_failure_summary.as_deref(), Some("missing command"));

        let listed = db.list_goal_runs(&project.id).expect("list goal runs");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);

        cleanup(&db_path);
    }
}

