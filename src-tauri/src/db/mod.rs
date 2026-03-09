mod queries;
mod agent_queries;

pub use queries::*;
pub use agent_queries::*;

use rusqlite::Connection;
use std::path::PathBuf;

pub struct Database {
    pub conn: Connection,
}

impl Database {
    pub fn new() -> Result<Self, String> {
        let db_path = Self::db_path()?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
        let db = Database { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn db_path() -> Result<PathBuf, String> {
        let mut path = dirs_next().ok_or("Could not determine data directory")?;
        path.push("project-builder-dashboard");
        path.push("data.db");
        Ok(path)
    }

    fn init_schema(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
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

            CREATE INDEX IF NOT EXISTS idx_pieces_project ON pieces(project_id);
            CREATE INDEX IF NOT EXISTS idx_pieces_parent ON pieces(parent_id);
            CREATE INDEX IF NOT EXISTS idx_connections_project ON connections(project_id);
            CREATE INDEX IF NOT EXISTS idx_agents_piece ON agents(piece_id);
            CREATE INDEX IF NOT EXISTS idx_artifacts_piece ON artifacts(piece_id);
            ",
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
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
