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

fn sql_to_goal_run_event_kind(value: &str) -> GoalRunEventKind {
    serde_json::from_str(&format!("\"{}\"", value)).unwrap_or(GoalRunEventKind::Note)
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
                "SELECT id, project_id, prompt, phase, status, blocker_reason, current_plan_id, runtime_status_summary, verification_summary, retry_count, last_failure_summary, stop_requested, current_piece_id, current_task_id, retry_backoff_until, last_failure_fingerprint, attention_required, created_at, updated_at FROM goal_runs WHERE id = ?1",
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
                        stop_requested: row.get::<_, i64>(11)? != 0,
                        current_piece_id: row.get(12)?,
                        current_task_id: row.get(13)?,
                        retry_backoff_until: row.get(14)?,
                        last_failure_fingerprint: row.get(15)?,
                        attention_required: row.get::<_, i64>(16)? != 0,
                        created_at: row.get(17)?,
                        updated_at: row.get(18)?,
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
        if let Some(stop_requested) = updates.stop_requested {
            self.conn
                .execute(
                    "UPDATE goal_runs SET stop_requested = ?1, updated_at = ?2 WHERE id = ?3",
                    params![if stop_requested { 1 } else { 0 }, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref current_piece_id) = updates.current_piece_id {
            self.conn
                .execute(
                    "UPDATE goal_runs SET current_piece_id = ?1, updated_at = ?2 WHERE id = ?3",
                    params![current_piece_id, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref current_task_id) = updates.current_task_id {
            self.conn
                .execute(
                    "UPDATE goal_runs SET current_task_id = ?1, updated_at = ?2 WHERE id = ?3",
                    params![current_task_id, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref retry_backoff_until) = updates.retry_backoff_until {
            self.conn
                .execute(
                    "UPDATE goal_runs SET retry_backoff_until = ?1, updated_at = ?2 WHERE id = ?3",
                    params![retry_backoff_until, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref last_failure_fingerprint) = updates.last_failure_fingerprint {
            self.conn
                .execute(
                    "UPDATE goal_runs SET last_failure_fingerprint = ?1, updated_at = ?2 WHERE id = ?3",
                    params![last_failure_fingerprint, now, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(attention_required) = updates.attention_required {
            self.conn
                .execute(
                    "UPDATE goal_runs SET attention_required = ?1, updated_at = ?2 WHERE id = ?3",
                    params![if attention_required { 1 } else { 0 }, now, id],
                )
                .map_err(|e| e.to_string())?;
        }

        self.get_goal_run(id)
    }

    /// On app startup, mark any goal runs that were mid-execution (status="running")
    /// as interrupted, since they can never complete now that the process died.
    pub fn mark_all_interrupted_runs(&self) -> Result<usize, String> {
        let now = chrono::Utc::now().to_rfc3339();
        let count = self.conn
            .execute(
                "UPDATE goal_runs SET status = 'interrupted', updated_at = ?1 WHERE status = 'running'",
                params![now],
            )
            .map_err(|e| e.to_string())?;
        Ok(count)
    }

    pub fn append_goal_run_event(
        &self,
        goal_run_id: &str,
        phase: GoalRunPhase,
        kind: GoalRunEventKind,
        summary: &str,
        payload_json: Option<&str>,
    ) -> Result<GoalRunEvent, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let phase = enum_to_sql(&phase)?;
        let kind = enum_to_sql(&kind)?;
        self.conn
            .execute(
                "INSERT INTO goal_run_events (id, goal_run_id, phase, kind, summary, payload_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, goal_run_id, phase, kind, summary, payload_json, now],
            )
            .map_err(|e| e.to_string())?;
        Ok(GoalRunEvent {
            id,
            goal_run_id: goal_run_id.to_string(),
            phase: sql_to_goal_run_phase(&phase),
            kind: sql_to_goal_run_event_kind(&kind),
            summary: summary.to_string(),
            payload_json: payload_json.map(str::to_string),
            created_at: now,
        })
    }

    pub fn list_goal_run_events(&self, goal_run_id: &str) -> Result<Vec<GoalRunEvent>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, goal_run_id, phase, kind, summary, payload_json, created_at
                 FROM goal_run_events WHERE goal_run_id = ?1 ORDER BY created_at ASC, rowid ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt.query_map(params![goal_run_id], |row| {
            let phase: String = row.get(2)?;
            let kind: String = row.get(3)?;
            Ok(GoalRunEvent {
                id: row.get(0)?,
                goal_run_id: row.get(1)?,
                phase: sql_to_goal_run_phase(&phase),
                kind: sql_to_goal_run_event_kind(&kind),
                summary: row.get(4)?,
                payload_json: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
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

    #[test]
    fn goal_run_events_roundtrip_in_order() {
        let db_path = temp_db_path("events");
        let db = Database::new_at_path(&db_path).expect("open test db");
        let project = db
            .create_project("Goal run project", "Testing goal run events")
            .expect("create project");
        let created = db
            .create_goal_run(&project.id, "Create a todo app")
            .expect("create goal run");

        let first = db
            .append_goal_run_event(
                &created.id,
                GoalRunPhase::Planning,
                GoalRunEventKind::PhaseStarted,
                "Planning started",
                Some("{\"step\":1}"),
            )
            .expect("append first event");
        let second = db
            .append_goal_run_event(
                &created.id,
                GoalRunPhase::Implementation,
                GoalRunEventKind::PhaseCompleted,
                "Implementation finished",
                None,
            )
            .expect("append second event");

        let events = db
            .list_goal_run_events(&created.id)
            .expect("list goal run events");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, first.id);
        assert_eq!(events[0].kind, GoalRunEventKind::PhaseStarted);
        assert_eq!(events[0].payload_json.as_deref(), Some("{\"step\":1}"));
        assert_eq!(events[1].id, second.id);
        assert_eq!(events[1].kind, GoalRunEventKind::PhaseCompleted);

        cleanup(&db_path);
    }
}
