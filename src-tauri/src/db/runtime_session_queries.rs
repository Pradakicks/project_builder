use crate::models::{ProjectRuntimeSession, RuntimeSessionStatus};
use rusqlite::params;
use tracing::debug;

use super::Database;

#[derive(Debug, Clone)]
pub struct RuntimeSessionRecord {
    pub id: String,
    pub project_id: String,
    pub goal_run_id: Option<String>,
    pub session: ProjectRuntimeSession,
}

fn sql_to_runtime_status(value: &str) -> RuntimeSessionStatus {
    serde_json::from_str(&format!("\"{}\"", value)).unwrap_or(RuntimeSessionStatus::Stopped)
}

fn enum_to_sql<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value)
        .map_err(|e| e.to_string())
        .map(|json| json.trim_matches('"').to_string())
}

impl Database {
    pub fn upsert_runtime_session(
        &self,
        project_id: &str,
        goal_run_id: Option<&str>,
        session: &ProjectRuntimeSession,
    ) -> Result<RuntimeSessionRecord, String> {
        debug!(project_id, session_id = %session.session_id, "Upserting runtime session");
        let id = format!("{project_id}:{}", session.session_id);
        let status = enum_to_sql(&session.status)?;
        self.conn
            .execute(
                "INSERT INTO runtime_sessions
                 (id, project_id, goal_run_id, session_id, status, url, port_hint, log_path, pid, last_error, exit_code, started_at, updated_at, ended_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT(id) DO UPDATE SET
                    goal_run_id = excluded.goal_run_id,
                    status = excluded.status,
                    url = excluded.url,
                    port_hint = excluded.port_hint,
                    log_path = excluded.log_path,
                    pid = excluded.pid,
                    last_error = excluded.last_error,
                    exit_code = excluded.exit_code,
                    started_at = excluded.started_at,
                    updated_at = excluded.updated_at,
                    ended_at = excluded.ended_at",
                params![
                    id,
                    project_id,
                    goal_run_id,
                    session.session_id,
                    status,
                    session.url,
                    session.port_hint,
                    session.log_path,
                    session.pid.map(|pid| pid as i64),
                    session.last_error,
                    session.exit_code,
                    session.started_at,
                    session.updated_at,
                    session.ended_at
                ],
            )
            .map_err(|e| e.to_string())?;

        Ok(RuntimeSessionRecord {
            id,
            project_id: project_id.to_string(),
            goal_run_id: goal_run_id.map(str::to_string),
            session: session.clone(),
        })
    }

    pub fn latest_runtime_session(
        &self,
        project_id: &str,
    ) -> Result<Option<RuntimeSessionRecord>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, project_id, goal_run_id, session_id, status, url, port_hint, log_path, pid, last_error, exit_code, started_at, updated_at, ended_at
                 FROM runtime_sessions WHERE project_id = ?1 ORDER BY updated_at DESC, rowid DESC LIMIT 1",
            )
            .map_err(|e| e.to_string())?;

        let mut rows = stmt.query(params![project_id]).map_err(|e| e.to_string())?;
        let Some(row) = rows.next().map_err(|e| e.to_string())? else {
            return Ok(None);
        };

        let status: String = row.get(4).map_err(|e| e.to_string())?;
        Ok(Some(RuntimeSessionRecord {
            id: row.get(0).map_err(|e| e.to_string())?,
            project_id: row.get(1).map_err(|e| e.to_string())?,
            goal_run_id: row.get(2).map_err(|e| e.to_string())?,
            session: ProjectRuntimeSession {
                session_id: row.get(3).map_err(|e| e.to_string())?,
                status: sql_to_runtime_status(&status),
                started_at: row.get(11).map_err(|e| e.to_string())?,
                updated_at: row.get(12).map_err(|e| e.to_string())?,
                ended_at: row.get(13).map_err(|e| e.to_string())?,
                url: row.get(5).map_err(|e| e.to_string())?,
                port_hint: row.get(6).map_err(|e| e.to_string())?,
                log_path: row.get(7).map_err(|e| e.to_string())?,
                recent_logs: Vec::new(),
                last_error: row.get(9).map_err(|e| e.to_string())?,
                exit_code: row.get(10).map_err(|e| e.to_string())?,
                pid: row
                    .get::<_, Option<i64>>(8)
                    .map_err(|e| e.to_string())?
                    .map(|pid| pid as u32),
            },
        }))
    }

    pub fn mark_runtime_sessions_interrupted(&self) -> Result<usize, String> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE runtime_sessions
                 SET status = 'stopped',
                     last_error = COALESCE(last_error, 'Runtime session interrupted when the app closed'),
                     ended_at = COALESCE(ended_at, ?1),
                     updated_at = ?1
                 WHERE status IN ('running', 'starting', 'stopping')",
                params![now],
            )
            .map_err(|e| e.to_string())
    }
}
