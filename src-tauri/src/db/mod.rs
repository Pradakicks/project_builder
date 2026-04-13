mod queries;
mod agent_queries;
mod artifact_queries;
mod goal_run_queries;
mod runtime_session_queries;
mod plan_queries;
mod cto_queries;

pub use queries::*;
pub use agent_queries::*;
#[allow(unused_imports)]
pub use plan_queries::*;
#[allow(unused_imports)]
pub use goal_run_queries::*;
#[allow(unused_imports)]
pub use runtime_session_queries::*;

use rusqlite::Connection;
use std::path::PathBuf;
use tracing::{error, info};

const CURRENT_SCHEMA_VERSION: i32 = 4;

type MigrationFn = fn(&Connection) -> Result<(), String>;

struct Migration {
    version: i32,
    description: &'static str,
    apply: MigrationFn,
}

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    description: "Bootstrap the core application schema",
    apply: migrate_v1,
}, Migration {
    version: 2,
    description: "Add structured CTO audit records",
    apply: migrate_v2,
}, Migration {
    version: 3,
    description: "Add goal run orchestration state",
    apply: migrate_v3,
}, Migration {
    version: CURRENT_SCHEMA_VERSION,
    description: "Persist goal run executor state, events, and runtime sessions",
    apply: migrate_v4,
}];

pub struct Database {
    pub conn: Connection,
}

impl Database {
    pub fn new() -> Result<Self, String> {
        Self::new_at_path(Self::db_path()?)
    }

    pub fn new_at_path(db_path: impl Into<PathBuf>) -> Result<Self, String> {
        let db_path = db_path.into();
        info!(path = %db_path.display(), "Initializing database");

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
        let mut db = Database { conn };
        db.init_schema()?;
        info!("Database schema initialized successfully");
        Ok(db)
    }

    fn db_path() -> Result<PathBuf, String> {
        let mut path = dirs_next().ok_or("Could not determine data directory")?;
        path.push("project-builder-dashboard");
        path.push("data.db");
        Ok(path)
    }

    fn init_schema(&mut self) -> Result<(), String> {
        let mut version = self.schema_version()?;
        if version > CURRENT_SCHEMA_VERSION {
            let message = format!(
                "Database schema version {} is newer than supported version {}",
                version, CURRENT_SCHEMA_VERSION
            );
            error!("{message}");
            return Err(message);
        }

        for migration in MIGRATIONS {
            if migration.version > version {
                info!(
                    version = migration.version,
                    description = migration.description,
                    "Applying database migration"
                );
                (migration.apply)(&self.conn)?;
                self.set_schema_version(migration.version)?;
                version = migration.version;
            }
        }

        ensure_agent_history_metadata_column(&self.conn)?;
        Ok(())
    }

    fn schema_version(&self) -> Result<i32, String> {
        self.conn
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .map_err(|e| e.to_string())
    }

    fn set_schema_version(&self, version: i32) -> Result<(), String> {
        self.conn
            .execute_batch(&format!("PRAGMA user_version = {version};"))
            .map_err(|e| e.to_string())
    }
}

fn migrate_v1(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            root_piece_id TEXT,
            settings_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS pieces (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            parent_id TEXT,
            name TEXT NOT NULL,
            piece_type TEXT NOT NULL DEFAULT '',
            color TEXT,
            icon TEXT,
            responsibilities TEXT NOT NULL DEFAULT '',
            interfaces_json TEXT NOT NULL DEFAULT '[]',
            constraints_json TEXT NOT NULL DEFAULT '[]',
            notes TEXT NOT NULL DEFAULT '',
            agent_prompt TEXT NOT NULL DEFAULT '',
            agent_config_json TEXT NOT NULL DEFAULT '{}',
            output_mode TEXT NOT NULL DEFAULT 'both',
            phase TEXT NOT NULL DEFAULT 'design',
            position_x REAL NOT NULL DEFAULT 0.0,
            position_y REAL NOT NULL DEFAULT 0.0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS connections (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            source_piece_id TEXT NOT NULL,
            target_piece_id TEXT NOT NULL,
            direction TEXT NOT NULL DEFAULT 'unidirectional',
            label TEXT NOT NULL DEFAULT '',
            data_type TEXT,
            protocol TEXT,
            constraints_json TEXT NOT NULL DEFAULT '[]',
            notes TEXT NOT NULL DEFAULT '',
            metadata_json TEXT NOT NULL DEFAULT '{}',
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
            FOREIGN KEY (source_piece_id) REFERENCES pieces(id) ON DELETE CASCADE,
            FOREIGN KEY (target_piece_id) REFERENCES pieces(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS agents (
            id TEXT PRIMARY KEY,
            piece_id TEXT NOT NULL,
            role TEXT NOT NULL,
            state TEXT NOT NULL DEFAULT 'idle',
            token_budget INTEGER NOT NULL DEFAULT 0,
            token_usage INTEGER NOT NULL DEFAULT 0,
            provider TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (piece_id) REFERENCES pieces(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS agent_history (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            action TEXT NOT NULL,
            input_text TEXT NOT NULL DEFAULT '',
            output_text TEXT NOT NULL DEFAULT '',
            metadata_json TEXT NOT NULL DEFAULT '{}',
            tokens_used INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS artifacts (
            id TEXT PRIMARY KEY,
            piece_id TEXT NOT NULL,
            agent_id TEXT,
            artifact_type TEXT NOT NULL,
            title TEXT NOT NULL,
            content TEXT NOT NULL DEFAULT '',
            review_status TEXT NOT NULL DEFAULT 'draft',
            version INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (piece_id) REFERENCES pieces(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS work_plans (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            version INTEGER NOT NULL DEFAULT 1,
            status TEXT NOT NULL DEFAULT 'generating',
            summary TEXT NOT NULL DEFAULT '',
            user_guidance TEXT NOT NULL DEFAULT '',
            tasks_json TEXT NOT NULL DEFAULT '[]',
            raw_output TEXT NOT NULL DEFAULT '',
            tokens_used INTEGER NOT NULL DEFAULT 0,
            integration_review TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_pieces_project ON pieces(project_id);
        CREATE INDEX IF NOT EXISTS idx_pieces_parent ON pieces(parent_id);
        CREATE INDEX IF NOT EXISTS idx_connections_project ON connections(project_id);
        CREATE INDEX IF NOT EXISTS idx_agents_piece ON agents(piece_id);
        CREATE INDEX IF NOT EXISTS idx_artifacts_piece ON artifacts(piece_id);
        CREATE INDEX IF NOT EXISTS idx_work_plans_project ON work_plans(project_id);

        CREATE TABLE IF NOT EXISTS cto_decisions (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            actions_json TEXT NOT NULL DEFAULT '[]',
            review_json TEXT NOT NULL DEFAULT '{}',
            execution_json TEXT,
            rollback_json TEXT,
            status TEXT NOT NULL DEFAULT 'rejected',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_cto_decisions_project ON cto_decisions(project_id);
        ",
    )
    .map_err(|e| {
        error!(error = %e, "Failed to initialize database schema");
        e.to_string()
    })?;

    ensure_agent_history_metadata_column(conn)?;
    ensure_cto_decisions_schema(conn)?;
    Ok(())
}

fn migrate_v2(conn: &Connection) -> Result<(), String> {
    ensure_cto_decisions_schema(conn)
}

fn migrate_v3(conn: &Connection) -> Result<(), String> {
    ensure_goal_runs_schema(conn)
}

fn migrate_v4(conn: &Connection) -> Result<(), String> {
    ensure_goal_runs_schema(conn)?;
    ensure_goal_run_events_schema(conn)?;
    ensure_runtime_sessions_schema(conn)
}

fn ensure_agent_history_metadata_column(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(agent_history)")
        .map_err(|e| e.to_string())?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?;

    let mut has_metadata_json = false;
    for column in columns {
        if column.map_err(|e| e.to_string())? == "metadata_json" {
            has_metadata_json = true;
            break;
        }
    }

    if !has_metadata_json {
        conn.execute(
            "ALTER TABLE agent_history ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn ensure_cto_decisions_schema(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(cto_decisions)")
        .map_err(|e| e.to_string())?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?;

    let mut has_review_json = false;
    let mut has_execution_json = false;
    let mut has_rollback_json = false;
    let mut has_status = false;
    let mut has_updated_at = false;

    for column in columns {
        match column.map_err(|e| e.to_string())?.as_str() {
            "review_json" => has_review_json = true,
            "execution_json" => has_execution_json = true,
            "rollback_json" => has_rollback_json = true,
            "status" => has_status = true,
            "updated_at" => has_updated_at = true,
            _ => {}
        }
    }

    if !has_review_json {
        conn.execute(
            "ALTER TABLE cto_decisions ADD COLUMN review_json TEXT NOT NULL DEFAULT '{}'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if !has_execution_json {
        conn.execute("ALTER TABLE cto_decisions ADD COLUMN execution_json TEXT", [])
            .map_err(|e| e.to_string())?;
    }
    if !has_rollback_json {
        conn.execute("ALTER TABLE cto_decisions ADD COLUMN rollback_json TEXT", [])
            .map_err(|e| e.to_string())?;
    }
    if !has_status {
        conn.execute(
            "ALTER TABLE cto_decisions ADD COLUMN status TEXT NOT NULL DEFAULT 'rejected'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if !has_updated_at {
        conn.execute(
            "ALTER TABLE cto_decisions ADD COLUMN updated_at TEXT NOT NULL DEFAULT (datetime('now'))",
            [],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn ensure_goal_runs_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS goal_runs (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            prompt TEXT NOT NULL,
            phase TEXT NOT NULL DEFAULT 'prompt-received',
            status TEXT NOT NULL DEFAULT 'running',
            blocker_reason TEXT,
            current_plan_id TEXT,
            runtime_status_summary TEXT,
            verification_summary TEXT,
            retry_count INTEGER NOT NULL DEFAULT 0,
            last_failure_summary TEXT,
            stop_requested INTEGER NOT NULL DEFAULT 0,
            current_piece_id TEXT,
            current_task_id TEXT,
            retry_backoff_until TEXT,
            last_failure_fingerprint TEXT,
            attention_required INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_goal_runs_project ON goal_runs(project_id);
        CREATE INDEX IF NOT EXISTS idx_goal_runs_status ON goal_runs(status, updated_at DESC);
        ",
    )
    .map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("PRAGMA table_info(goal_runs)")
        .map_err(|e| e.to_string())?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?;

    let mut has_stop_requested = false;
    let mut has_current_piece_id = false;
    let mut has_current_task_id = false;
    let mut has_retry_backoff_until = false;
    let mut has_last_failure_fingerprint = false;
    let mut has_attention_required = false;

    for column in columns {
        match column.map_err(|e| e.to_string())?.as_str() {
            "stop_requested" => has_stop_requested = true,
            "current_piece_id" => has_current_piece_id = true,
            "current_task_id" => has_current_task_id = true,
            "retry_backoff_until" => has_retry_backoff_until = true,
            "last_failure_fingerprint" => has_last_failure_fingerprint = true,
            "attention_required" => has_attention_required = true,
            _ => {}
        }
    }

    if !has_stop_requested {
        conn.execute(
            "ALTER TABLE goal_runs ADD COLUMN stop_requested INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if !has_current_piece_id {
        conn.execute("ALTER TABLE goal_runs ADD COLUMN current_piece_id TEXT", [])
            .map_err(|e| e.to_string())?;
    }
    if !has_current_task_id {
        conn.execute("ALTER TABLE goal_runs ADD COLUMN current_task_id TEXT", [])
            .map_err(|e| e.to_string())?;
    }
    if !has_retry_backoff_until {
        conn.execute("ALTER TABLE goal_runs ADD COLUMN retry_backoff_until TEXT", [])
            .map_err(|e| e.to_string())?;
    }
    if !has_last_failure_fingerprint {
        conn.execute(
            "ALTER TABLE goal_runs ADD COLUMN last_failure_fingerprint TEXT",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if !has_attention_required {
        conn.execute(
            "ALTER TABLE goal_runs ADD COLUMN attention_required INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn ensure_goal_run_events_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS goal_run_events (
            id TEXT PRIMARY KEY,
            goal_run_id TEXT NOT NULL,
            phase TEXT NOT NULL,
            kind TEXT NOT NULL,
            summary TEXT NOT NULL,
            payload_json TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (goal_run_id) REFERENCES goal_runs(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_goal_run_events_goal_run ON goal_run_events(goal_run_id, created_at ASC);
        ",
    )
    .map_err(|e| e.to_string())
}

fn ensure_runtime_sessions_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS runtime_sessions (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            goal_run_id TEXT,
            session_id TEXT NOT NULL,
            status TEXT NOT NULL,
            url TEXT,
            port_hint INTEGER,
            log_path TEXT,
            pid INTEGER,
            last_error TEXT,
            exit_code INTEGER,
            started_at TEXT,
            updated_at TEXT NOT NULL,
            ended_at TEXT,
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
            FOREIGN KEY (goal_run_id) REFERENCES goal_runs(id) ON DELETE SET NULL
        );
        CREATE INDEX IF NOT EXISTS idx_runtime_sessions_project ON runtime_sessions(project_id, updated_at DESC);
        ",
    )
    .map_err(|e| e.to_string())
}

/// Get the platform-appropriate data directory
fn dirs_next() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").ok().map(PathBuf::from)
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".local").join("share"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::fs;
    use std::path::Path;

    fn temp_db_path(case: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "project-builder-dashboard-{case}-{}",
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

    fn user_version(conn: &Connection) -> i32 {
        conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .expect("read user_version")
    }

    fn table_has_column(conn: &Connection, table: &str, column: &str) -> bool {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("prepare table_info");
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query table_info");

        for row in rows {
            if row.expect("read table_info row") == column {
                return true;
            }
        }
        false
    }

    #[test]
    fn new_database_initializes_schema_and_is_idempotent() {
        let db_path = temp_db_path("fresh");

        let project_id = {
            let db = Database::new_at_path(&db_path).expect("initial database open");
            assert_eq!(user_version(&db.conn), CURRENT_SCHEMA_VERSION);
            assert!(table_has_column(&db.conn, "agent_history", "metadata_json"));

            let project = db
                .create_project("Alpha", "First project")
                .expect("create project");
            assert_eq!(user_version(&db.conn), CURRENT_SCHEMA_VERSION);
            project.id
        };

        let reopened = Database::new_at_path(&db_path).expect("reopen database");
        assert_eq!(user_version(&reopened.conn), CURRENT_SCHEMA_VERSION);
        assert!(table_has_column(&reopened.conn, "agent_history", "metadata_json"));

        let projects = reopened.list_projects().expect("list projects");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, project_id);

        drop(reopened);
        cleanup(&db_path);
    }

    #[test]
    fn legacy_database_is_upgraded_without_losing_rows() {
        let db_path = temp_db_path("legacy");
        let conn = Connection::open(&db_path).expect("open legacy db");
        conn.execute_batch(
            "
            CREATE TABLE projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                root_piece_id TEXT,
                settings_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE agent_history (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                action TEXT NOT NULL,
                input_text TEXT NOT NULL DEFAULT '',
                output_text TEXT NOT NULL DEFAULT '',
                tokens_used INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );
            CREATE TABLE cto_decisions (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                actions_json TEXT NOT NULL DEFAULT '[]',
                review_json TEXT NOT NULL DEFAULT '{}',
                execution_json TEXT,
                rollback_json TEXT,
                status TEXT NOT NULL DEFAULT 'rejected',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT INTO projects (
                id, name, description, settings_json, created_at, updated_at
            ) VALUES (
                'project-1', 'Legacy project', 'Before goal runs', '{}', '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z'
            );
            INSERT INTO agent_history (
                id, agent_id, action, input_text, output_text, tokens_used, created_at
            ) VALUES (
                'history-1', 'agent-1', 'run', 'input', 'output', 5, '2024-01-01T00:00:00Z'
            );
        ",
        )
        .expect("seed legacy schema");
        conn.execute_batch("PRAGMA user_version = 2;")
            .expect("set legacy version");

        drop(conn);

        let db = Database::new_at_path(&db_path).expect("upgrade legacy db");
        assert_eq!(user_version(&db.conn), CURRENT_SCHEMA_VERSION);
        assert!(table_has_column(&db.conn, "agent_history", "metadata_json"));
        assert!(table_has_column(&db.conn, "goal_runs", "retry_count"));

        let metadata: String = db
            .conn
            .query_row(
                "SELECT metadata_json FROM agent_history WHERE id = 'history-1'",
                [],
                |row| row.get(0),
            )
            .expect("read migrated row");
        assert_eq!(metadata, "{}");

        let projects = db.list_projects().expect("list projects");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, "project-1");

        drop(db);
        cleanup(&db_path);
    }
}
