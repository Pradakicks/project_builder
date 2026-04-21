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
                "SELECT id, project_id, prompt, phase, status, blocker_reason, current_plan_id, runtime_status_summary, verification_summary, retry_count, last_failure_summary, stop_requested, current_piece_id, current_task_id, retry_backoff_until, last_failure_fingerprint, attention_required, last_heartbeat_at, operator_repair_requested, created_at, updated_at FROM goal_runs WHERE id = ?1",
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
                        last_heartbeat_at: row.get(17)?,
                        operator_repair_requested: row.get::<_, i64>(18)? != 0,
                        created_at: row.get(19)?,
                        updated_at: row.get(20)?,
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

    pub fn update_goal_run(&self, id: &str, updates: &GoalRunUpdate) -> Result<GoalRun, String> {
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

        // Collect all set-clauses into a single UPDATE so multi-field writes are
        // atomic. The per-field UPDATE-per-field pattern was crash-unsafe: a
        // mid-sequence crash could desync `phase` vs `retry_count` vs `status`
        // and break resume/recovery invariants.
        let mut sets: Vec<&'static str> = vec!["updated_at = ?"];
        let mut params_vec: Vec<rusqlite::types::Value> = vec![now.into()];

        if let Some(ref prompt) = updates.prompt {
            sets.push("prompt = ?");
            params_vec.push(prompt.trim().to_string().into());
        }
        if let Some(ref phase) = updates.phase {
            sets.push("phase = ?");
            params_vec.push(enum_to_sql(phase)?.into());
        }
        if let Some(ref status) = updates.status {
            sets.push("status = ?");
            params_vec.push(enum_to_sql(status)?.into());
        }
        if let Some(ref blocker_reason) = updates.blocker_reason {
            sets.push("blocker_reason = ?");
            params_vec.push(blocker_reason.clone().into());
        }
        if let Some(ref current_plan_id) = updates.current_plan_id {
            sets.push("current_plan_id = ?");
            params_vec.push(current_plan_id.clone().into());
        }
        if let Some(ref runtime_status_summary) = updates.runtime_status_summary {
            sets.push("runtime_status_summary = ?");
            params_vec.push(runtime_status_summary.clone().into());
        }
        if let Some(ref verification_summary) = updates.verification_summary {
            sets.push("verification_summary = ?");
            params_vec.push(verification_summary.clone().into());
        }
        if let Some(retry_count) = updates.retry_count {
            sets.push("retry_count = ?");
            params_vec.push(retry_count.into());
        }
        if let Some(ref last_failure_summary) = updates.last_failure_summary {
            sets.push("last_failure_summary = ?");
            params_vec.push(last_failure_summary.clone().into());
        }
        if let Some(stop_requested) = updates.stop_requested {
            sets.push("stop_requested = ?");
            params_vec.push(i64::from(stop_requested).into());
        }
        if let Some(ref current_piece_id) = updates.current_piece_id {
            sets.push("current_piece_id = ?");
            params_vec.push(current_piece_id.clone().into());
        }
        if let Some(ref current_task_id) = updates.current_task_id {
            sets.push("current_task_id = ?");
            params_vec.push(current_task_id.clone().into());
        }
        if let Some(ref retry_backoff_until) = updates.retry_backoff_until {
            sets.push("retry_backoff_until = ?");
            params_vec.push(retry_backoff_until.clone().into());
        }
        if let Some(ref last_failure_fingerprint) = updates.last_failure_fingerprint {
            sets.push("last_failure_fingerprint = ?");
            params_vec.push(last_failure_fingerprint.clone().into());
        }
        if let Some(attention_required) = updates.attention_required {
            sets.push("attention_required = ?");
            params_vec.push(i64::from(attention_required).into());
        }
        if let Some(ref last_heartbeat_at) = updates.last_heartbeat_at {
            sets.push("last_heartbeat_at = ?");
            params_vec.push(last_heartbeat_at.clone().into());
        }
        if let Some(operator_repair_requested) = updates.operator_repair_requested {
            sets.push("operator_repair_requested = ?");
            params_vec.push(i64::from(operator_repair_requested).into());
        }

        // No substantive updates → skip the write; only `updated_at` would change.
        if sets.len() == 1 {
            return self.get_goal_run(id);
        }

        params_vec.push(id.to_string().into());
        let sql = format!("UPDATE goal_runs SET {} WHERE id = ?", sets.join(", "));
        self.conn
            .execute(&sql, rusqlite::params_from_iter(params_vec))
            .map_err(|e| e.to_string())?;

        self.get_goal_run(id)
    }

    /// Fast heartbeat write — bumps `last_heartbeat_at` without touching `updated_at`.
    /// Called on a timer from the executor; `updated_at` churn would blow up the
    /// idx_goal_runs_status index for no UX gain.
    pub fn update_heartbeat(&self, goal_run_id: &str) -> Result<(), String> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE goal_runs SET last_heartbeat_at = ?1 WHERE id = ?2",
                params![now, goal_run_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// On app startup, flag runs whose heartbeat is missing or older than
    /// `stale_secs` seconds as Interrupted. Only touches status in ('running','retrying')
    /// and never touches Paused rows (pause has no live heartbeat by design).
    pub fn mark_stale_runs_interrupted(&self, stale_secs: i64) -> Result<usize, String> {
        let now = chrono::Utc::now().to_rfc3339();
        let cutoff = (chrono::Utc::now() - chrono::Duration::seconds(stale_secs)).to_rfc3339();
        let count = self
            .conn
            .execute(
                "UPDATE goal_runs SET status = 'interrupted', updated_at = ?1 \
                 WHERE status IN ('running','retrying') \
                   AND (last_heartbeat_at IS NULL OR last_heartbeat_at < ?2)",
                params![now, cutoff],
            )
            .map_err(|e| e.to_string())?;
        Ok(count)
    }

    /// Rows that were scheduled for a backoff-and-retry whose window has now elapsed.
    pub fn list_runs_due_for_backoff(&self) -> Result<Vec<String>, String> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id FROM goal_runs \
                 WHERE status = 'retrying' \
                   AND retry_backoff_until IS NOT NULL \
                   AND retry_backoff_until <= ?1",
            )
            .map_err(|e| e.to_string())?;
        let ids: Vec<String> = stmt
            .query_map(params![now], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(ids)
    }

    /// Runs flagged Interrupted — powers the startup "resume these?" banner.
    pub fn list_interrupted_runs(&self) -> Result<Vec<GoalRun>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id FROM goal_runs WHERE status = 'interrupted' ORDER BY updated_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        ids.iter().map(|id| self.get_goal_run(id)).collect()
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

        let rows = stmt
            .query_map(params![goal_run_id], |row| {
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
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
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
        assert!(!created.operator_repair_requested);

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
                    operator_repair_requested: Some(true),
                    ..Default::default()
                },
            )
            .expect("update goal run");

        assert_eq!(updated.phase, GoalRunPhase::Planning);
        assert_eq!(updated.status, GoalRunStatus::Blocked);
        assert_eq!(
            updated.blocker_reason.as_deref(),
            Some("Waiting on runtime spec")
        );
        assert_eq!(updated.current_plan_id.as_deref(), Some("plan-123"));
        assert_eq!(
            updated.runtime_status_summary.as_deref(),
            Some("runtime not configured")
        );
        assert_eq!(
            updated.verification_summary.as_deref(),
            Some("verification not started")
        );
        assert_eq!(updated.retry_count, 2);
        assert_eq!(
            updated.last_failure_summary.as_deref(),
            Some("missing command")
        );
        assert!(updated.operator_repair_requested);

        let listed = db.list_goal_runs(&project.id).expect("list goal runs");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);

        cleanup(&db_path);
    }

    #[test]
    fn update_goal_run_writes_many_fields_atomically_in_one_call() {
        // Regression: multi-field updates used to execute one UPDATE per field,
        // which was crash-unsafe. They now collapse into a single UPDATE.
        let db_path = temp_db_path("atomic-update");
        let db = Database::new_at_path(&db_path).expect("open test db");
        let project = db
            .create_project("Atomic project", "Testing atomic update")
            .expect("create project");
        let created = db
            .create_goal_run(&project.id, "atomic run")
            .expect("create goal run");

        let updated = db
            .update_goal_run(
                &created.id,
                &GoalRunUpdate {
                    phase: Some(GoalRunPhase::Implementation),
                    status: Some(GoalRunStatus::Retrying),
                    blocker_reason: Some(Some("stuck on merge".to_string())),
                    current_plan_id: Some(Some("plan-42".to_string())),
                    current_piece_id: Some(Some("piece-7".to_string())),
                    current_task_id: Some(Some("task-3".to_string())),
                    retry_count: Some(4),
                    last_failure_summary: Some(Some("cli crashed".to_string())),
                    last_failure_fingerprint: Some(Some("impl:cli-crashed".to_string())),
                    stop_requested: Some(true),
                    attention_required: Some(true),
                    ..Default::default()
                },
            )
            .expect("multi-field update");

        assert_eq!(updated.phase, GoalRunPhase::Implementation);
        assert_eq!(updated.status, GoalRunStatus::Retrying);
        assert_eq!(updated.blocker_reason.as_deref(), Some("stuck on merge"));
        assert_eq!(updated.current_plan_id.as_deref(), Some("plan-42"));
        assert_eq!(updated.current_piece_id.as_deref(), Some("piece-7"));
        assert_eq!(updated.current_task_id.as_deref(), Some("task-3"));
        assert_eq!(updated.retry_count, 4);
        assert_eq!(updated.last_failure_summary.as_deref(), Some("cli crashed"));
        assert_eq!(
            updated.last_failure_fingerprint.as_deref(),
            Some("impl:cli-crashed")
        );
        assert!(updated.stop_requested);
        assert!(updated.attention_required);

        // No-op update (nothing set) should not error and should return the row.
        let untouched = db
            .update_goal_run(&created.id, &GoalRunUpdate::default())
            .expect("no-op update");
        assert_eq!(untouched.id, created.id);
        assert_eq!(untouched.retry_count, 4);

        // Clearing nullable fields via Some(None).
        let cleared = db
            .update_goal_run(
                &created.id,
                &GoalRunUpdate {
                    blocker_reason: Some(None),
                    last_failure_summary: Some(None),
                    ..Default::default()
                },
            )
            .expect("clear fields");
        assert!(cleared.blocker_reason.is_none());
        assert!(cleared.last_failure_summary.is_none());

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
        let third = db
            .append_goal_run_event(
                &created.id,
                GoalRunPhase::Verification,
                GoalRunEventKind::RepairStarted,
                "CTO repair agent started",
                Some("{\"context\":{\"goalRunId\":\"goal-run-1\"}}"),
            )
            .expect("append third event");

        let events = db
            .list_goal_run_events(&created.id)
            .expect("list goal run events");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].id, first.id);
        assert_eq!(events[0].kind, GoalRunEventKind::PhaseStarted);
        assert_eq!(events[0].payload_json.as_deref(), Some("{\"step\":1}"));
        assert_eq!(events[1].id, second.id);
        assert_eq!(events[1].kind, GoalRunEventKind::PhaseCompleted);
        assert_eq!(events[2].id, third.id);
        assert_eq!(events[2].kind, GoalRunEventKind::RepairStarted);

        cleanup(&db_path);
    }

    #[test]
    fn stale_heartbeat_sweeper_preserves_paused_rows() {
        let db_path = temp_db_path("stale-sweeper");
        let db = Database::new_at_path(&db_path).expect("open test db");
        let project = db
            .create_project("Heartbeat project", "Testing stale sweeper")
            .expect("create project");

        // running + no heartbeat => should be flagged interrupted
        let stale = db
            .create_goal_run(&project.id, "stale run")
            .expect("create stale run");

        // paused => must NOT be touched even without a heartbeat
        let paused = db
            .create_goal_run(&project.id, "paused run")
            .expect("create paused run");
        db.update_goal_run(
            &paused.id,
            &GoalRunUpdate {
                status: Some(GoalRunStatus::Paused),
                ..Default::default()
            },
        )
        .expect("mark paused");

        // running + fresh heartbeat => must NOT be touched
        let alive = db
            .create_goal_run(&project.id, "alive run")
            .expect("create alive run");
        db.update_heartbeat(&alive.id).expect("bump heartbeat");

        let count = db.mark_stale_runs_interrupted(30).expect("sweep");
        assert_eq!(count, 1, "only the stale running row should flip");

        assert_eq!(
            db.get_goal_run(&stale.id).unwrap().status,
            GoalRunStatus::Interrupted
        );
        assert_eq!(
            db.get_goal_run(&paused.id).unwrap().status,
            GoalRunStatus::Paused
        );
        assert_eq!(
            db.get_goal_run(&alive.id).unwrap().status,
            GoalRunStatus::Running
        );

        cleanup(&db_path);
    }

    #[test]
    fn runs_due_for_backoff_respects_time_window() {
        let db_path = temp_db_path("backoff-window");
        let db = Database::new_at_path(&db_path).expect("open test db");
        let project = db
            .create_project("Backoff project", "Testing backoff")
            .expect("create project");

        let now = chrono::Utc::now();
        let past = (now - chrono::Duration::seconds(60)).to_rfc3339();
        let future = (now + chrono::Duration::seconds(300)).to_rfc3339();

        let due = db
            .create_goal_run(&project.id, "due run")
            .expect("create due run");
        db.update_goal_run(
            &due.id,
            &GoalRunUpdate {
                status: Some(GoalRunStatus::Retrying),
                retry_backoff_until: Some(Some(past)),
                ..Default::default()
            },
        )
        .expect("set due");

        let pending = db
            .create_goal_run(&project.id, "pending run")
            .expect("create pending run");
        db.update_goal_run(
            &pending.id,
            &GoalRunUpdate {
                status: Some(GoalRunStatus::Retrying),
                retry_backoff_until: Some(Some(future)),
                ..Default::default()
            },
        )
        .expect("set pending");

        let ids = db.list_runs_due_for_backoff().expect("list due");
        assert_eq!(ids, vec![due.id.clone()]);

        cleanup(&db_path);
    }

    #[test]
    fn interrupted_runs_are_listed_for_resume_banner() {
        let db_path = temp_db_path("interrupted-list");
        let db = Database::new_at_path(&db_path).expect("open test db");
        let project = db
            .create_project("Banner project", "Testing interrupted listing")
            .expect("create project");

        let interrupted = db
            .create_goal_run(&project.id, "interrupted run")
            .expect("create");
        db.update_goal_run(
            &interrupted.id,
            &GoalRunUpdate {
                status: Some(GoalRunStatus::Interrupted),
                ..Default::default()
            },
        )
        .expect("flip to interrupted");

        let running = db
            .create_goal_run(&project.id, "running run")
            .expect("create running");
        db.update_heartbeat(&running.id).expect("beat");

        let listed = db.list_interrupted_runs().expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, interrupted.id);

        cleanup(&db_path);
    }
}
