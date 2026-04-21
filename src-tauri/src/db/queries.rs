use crate::models::*;
use rusqlite::params;
use std::collections::HashMap;
use tracing::{debug, error};

use super::Database;

/// Parse JSON with error logging. Returns default on failure.
fn parse_json_logged<T: serde::de::DeserializeOwned + Default>(json_str: &str, field: &str) -> T {
    serde_json::from_str(json_str).unwrap_or_else(|e| {
        error!(field, error = %e, "JSON parse failed in DB query — possible data corruption");
        Default::default()
    })
}

impl Database {
    // ── Projects ──────────────────────────────────────────────

    pub fn create_project(&self, name: &str, description: &str) -> Result<Project, String> {
        self.create_project_with_settings(name, description, ProjectSettings::default())
    }

    pub fn create_project_with_settings(
        &self,
        name: &str,
        description: &str,
        settings: ProjectSettings,
    ) -> Result<Project, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let settings_json = serde_json::to_string(&settings).map_err(|e| e.to_string())?;

        self.conn
            .execute(
                "INSERT INTO projects (id, name, description, settings_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, name, description, settings_json, now, now],
            )
            .map_err(|e| e.to_string())?;

        debug!(project_id = %id, name, "Created project");
        Ok(Project {
            id,
            name: name.to_string(),
            description: description.to_string(),
            root_piece_id: None,
            settings,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn get_project(&self, id: &str) -> Result<Project, String> {
        debug!(project_id = id, "Getting project");
        self.conn
            .query_row(
                "SELECT id, name, description, root_piece_id, settings_json, created_at, updated_at FROM projects WHERE id = ?1",
                params![id],
                |row| {
                    let settings_json: String = row.get(4)?;
                    Ok(Project {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        description: row.get(2)?,
                        root_piece_id: row.get(3)?,
                        settings: parse_json_logged(&settings_json, "project_settings"),
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .map_err(|e| e.to_string())
    }

    pub fn update_project(&self, id: &str, name: Option<&str>, description: Option<&str>, root_piece_id: Option<Option<&str>>, settings: Option<&ProjectSettings>) -> Result<Project, String> {
        debug!(project_id = id, "Updating project");
        let now = chrono::Utc::now().to_rfc3339();

        if let Some(name) = name {
            self.conn.execute("UPDATE projects SET name = ?1, updated_at = ?2 WHERE id = ?3", params![name, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(desc) = description {
            self.conn.execute("UPDATE projects SET description = ?1, updated_at = ?2 WHERE id = ?3", params![desc, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(root_id) = root_piece_id {
            self.conn.execute("UPDATE projects SET root_piece_id = ?1, updated_at = ?2 WHERE id = ?3", params![root_id, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(s) = settings {
            let json = serde_json::to_string(s).map_err(|e| e.to_string())?;
            self.conn.execute("UPDATE projects SET settings_json = ?1, updated_at = ?2 WHERE id = ?3", params![json, now, id]).map_err(|e| e.to_string())?;
        }

        self.get_project(id)
    }

    pub fn list_projects(&self) -> Result<Vec<Project>, String> {
        debug!("Listing projects");
        let mut stmt = self.conn
            .prepare("SELECT id, name, description, root_piece_id, settings_json, created_at, updated_at FROM projects ORDER BY updated_at DESC")
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                let settings_json: String = row.get(4)?;
                Ok(Project {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    root_piece_id: row.get(3)?,
                    settings: parse_json_logged(&settings_json, "project_settings"),
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn delete_project(&self, id: &str) -> Result<(), String> {
        debug!(project_id = id, "Deleting project");
        self.conn.execute("DELETE FROM connections WHERE project_id = ?1", params![id]).map_err(|e| e.to_string())?;
        self.conn.execute("DELETE FROM pieces WHERE project_id = ?1", params![id]).map_err(|e| e.to_string())?;
        self.conn.execute("DELETE FROM projects WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── Pieces ────────────────────────────────────────────────

    pub fn create_piece(&self, project_id: &str, parent_id: Option<&str>, name: &str, position_x: f64, position_y: f64) -> Result<Piece, String> {
        debug!(project_id, name, "Creating piece");
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        // New pieces default to the single-role flow (implementation only).
        // Users opt in to Testing / Review per piece via the PieceEditor UI.
        // This keeps token costs predictable for users who don't want the
        // three-agent pass. A future migration or project-level default can
        // flip this later.
        let mut agent_config = AgentConfig::default();
        agent_config.active_agents = vec!["implementation".to_string()];
        let agent_config_json = serde_json::to_string(&agent_config).map_err(|e| e.to_string())?;

        self.conn
            .execute(
                "INSERT INTO pieces (id, project_id, parent_id, name, agent_config_json, position_x, position_y, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![id, project_id, parent_id, name, agent_config_json, position_x, position_y, now, now],
            )
            .map_err(|e| e.to_string())?;

        Ok(Piece {
            id,
            project_id: project_id.to_string(),
            parent_id: parent_id.map(String::from),
            name: name.to_string(),
            piece_type: String::new(),
            color: None,
            icon: None,
            responsibilities: String::new(),
            interfaces: vec![],
            constraints: vec![],
            notes: String::new(),
            agent_prompt: String::new(),
            agent_config,
            output_mode: OutputMode::Both,
            phase: Phase::Design,
            position_x,
            position_y,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn get_piece(&self, id: &str) -> Result<Piece, String> {
        debug!(piece_id = id, "Getting piece");
        self.conn
            .query_row(
                "SELECT id, project_id, parent_id, name, piece_type, color, icon, responsibilities, interfaces_json, constraints_json, notes, agent_prompt, agent_config_json, output_mode, phase, position_x, position_y, created_at, updated_at FROM pieces WHERE id = ?1",
                params![id],
                |row| {
                    let interfaces_json: String = row.get(8)?;
                    let constraints_json: String = row.get(9)?;
                    let agent_config_json: String = row.get(12)?;
                    let output_mode_str: String = row.get(13)?;
                    let phase_str: String = row.get(14)?;

                    Ok(Piece {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        parent_id: row.get(2)?,
                        name: row.get(3)?,
                        piece_type: row.get(4)?,
                        color: row.get(5)?,
                        icon: row.get(6)?,
                        responsibilities: row.get(7)?,
                        interfaces: parse_json_logged(&interfaces_json, "interfaces"),
                        constraints: parse_json_logged(&constraints_json, "constraints"),
                        notes: row.get(10)?,
                        agent_prompt: row.get(11)?,
                        agent_config: parse_json_logged(&agent_config_json, "agent_config"),
                        output_mode: serde_json::from_str(&format!("\"{}\"", output_mode_str)).unwrap_or(OutputMode::Both),
                        phase: serde_json::from_str(&format!("\"{}\"", phase_str)).unwrap_or(Phase::Design),
                        position_x: row.get(15)?,
                        position_y: row.get(16)?,
                        created_at: row.get(17)?,
                        updated_at: row.get(18)?,
                    })
                },
            )
            .map_err(|e| e.to_string())
    }

    pub fn update_piece(&self, id: &str, updates: &PieceUpdate) -> Result<Piece, String> {
        debug!(piece_id = id, "Updating piece");
        let now = chrono::Utc::now().to_rfc3339();

        if let Some(ref name) = updates.name {
            self.conn.execute("UPDATE pieces SET name = ?1, updated_at = ?2 WHERE id = ?3", params![name, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref piece_type) = updates.piece_type {
            self.conn.execute("UPDATE pieces SET piece_type = ?1, updated_at = ?2 WHERE id = ?3", params![piece_type, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref color) = updates.color {
            self.conn.execute("UPDATE pieces SET color = ?1, updated_at = ?2 WHERE id = ?3", params![color, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref icon) = updates.icon {
            self.conn.execute("UPDATE pieces SET icon = ?1, updated_at = ?2 WHERE id = ?3", params![icon, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref resp) = updates.responsibilities {
            self.conn.execute("UPDATE pieces SET responsibilities = ?1, updated_at = ?2 WHERE id = ?3", params![resp, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref interfaces) = updates.interfaces {
            let json = serde_json::to_string(interfaces).map_err(|e| e.to_string())?;
            self.conn.execute("UPDATE pieces SET interfaces_json = ?1, updated_at = ?2 WHERE id = ?3", params![json, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref constraints) = updates.constraints {
            let json = serde_json::to_string(constraints).map_err(|e| e.to_string())?;
            self.conn.execute("UPDATE pieces SET constraints_json = ?1, updated_at = ?2 WHERE id = ?3", params![json, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref notes) = updates.notes {
            self.conn.execute("UPDATE pieces SET notes = ?1, updated_at = ?2 WHERE id = ?3", params![notes, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref prompt) = updates.agent_prompt {
            self.conn.execute("UPDATE pieces SET agent_prompt = ?1, updated_at = ?2 WHERE id = ?3", params![prompt, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref config) = updates.agent_config {
            let json = serde_json::to_string(config).map_err(|e| e.to_string())?;
            self.conn.execute("UPDATE pieces SET agent_config_json = ?1, updated_at = ?2 WHERE id = ?3", params![json, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref mode) = updates.output_mode {
            let json = serde_json::to_string(mode).map_err(|e| e.to_string())?;
            let val = json.trim_matches('"');
            self.conn.execute("UPDATE pieces SET output_mode = ?1, updated_at = ?2 WHERE id = ?3", params![val, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref phase) = updates.phase {
            let json = serde_json::to_string(phase).map_err(|e| e.to_string())?;
            let val = json.trim_matches('"');
            self.conn.execute("UPDATE pieces SET phase = ?1, updated_at = ?2 WHERE id = ?3", params![val, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(x) = updates.position_x {
            self.conn.execute("UPDATE pieces SET position_x = ?1, updated_at = ?2 WHERE id = ?3", params![x, now, id]).map_err(|e| e.to_string())?;
        }
        if let Some(y) = updates.position_y {
            self.conn.execute("UPDATE pieces SET position_y = ?1, updated_at = ?2 WHERE id = ?3", params![y, now, id]).map_err(|e| e.to_string())?;
        }

        self.get_piece(id)
    }

    pub fn delete_piece(&self, id: &str) -> Result<(), String> {
        debug!(piece_id = id, "Deleting piece");
        self.conn.execute("DELETE FROM connections WHERE source_piece_id = ?1 OR target_piece_id = ?1", params![id]).map_err(|e| e.to_string())?;
        // Reparent children to deleted piece's parent
        let parent_id: Option<String> = self.conn.query_row("SELECT parent_id FROM pieces WHERE id = ?1", params![id], |row| row.get(0)).map_err(|e| e.to_string())?;
        self.conn.execute("UPDATE pieces SET parent_id = ?1 WHERE parent_id = ?2", params![parent_id, id]).map_err(|e| e.to_string())?;
        self.conn.execute("DELETE FROM pieces WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_pieces(&self, project_id: &str) -> Result<Vec<Piece>, String> {
        debug!(project_id, "Listing pieces");
        let mut stmt = self.conn
            .prepare("SELECT id FROM pieces WHERE project_id = ?1")
            .map_err(|e| e.to_string())?;

        let ids: Vec<String> = stmt
            .query_map(params![project_id], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        ids.iter().map(|id| self.get_piece(id)).collect()
    }

    /// Distinct normalized team names across all pieces in a project.
    /// Powers the PieceEditor team autocomplete and the ProjectStatusBar
    /// team chip. Pulled via SQLite `json_extract` so we don't have to
    /// deserialize every agent_config row just to gather team tags.
    pub fn list_teams_for_project(&self, project_id: &str) -> Result<Vec<String>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT DISTINCT json_extract(agent_config_json, '$.team') AS team \
                 FROM pieces \
                 WHERE project_id = ?1 \
                   AND team IS NOT NULL \
                   AND team != '' \
                 ORDER BY team ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![project_id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut teams = Vec::new();
        for row in rows {
            match row {
                Ok(team) => teams.push(team),
                // Skip rows where json_extract returned NULL (already filtered)
                // or non-string values; don't fail the whole call.
                Err(_) => continue,
            }
        }
        Ok(teams)
    }

    pub fn list_children(&self, piece_id: &str) -> Result<Vec<Piece>, String> {
        let mut stmt = self.conn
            .prepare("SELECT id FROM pieces WHERE parent_id = ?1")
            .map_err(|e| e.to_string())?;

        let ids: Vec<String> = stmt
            .query_map(params![piece_id], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        ids.iter().map(|id| self.get_piece(id)).collect()
    }

    // ── Connections ───────────────────────────────────────────

    pub fn create_connection(&self, project_id: &str, source_piece_id: &str, target_piece_id: &str, label: &str) -> Result<Connection, String> {
        debug!(project_id, source = source_piece_id, target = target_piece_id, label, "Creating connection");
        let source_piece = self.get_piece(source_piece_id).map_err(|_| "Source piece not found".to_string())?;
        let target_piece = self.get_piece(target_piece_id).map_err(|_| "Target piece not found".to_string())?;

        if source_piece.project_id != project_id || target_piece.project_id != project_id {
            return Err("Pieces must belong to the current project".to_string());
        }

        let id = uuid::Uuid::new_v4().to_string();

        self.conn
            .execute(
                "INSERT INTO connections (id, project_id, source_piece_id, target_piece_id, label) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, project_id, source_piece_id, target_piece_id, label],
            )
            .map_err(|e| e.to_string())?;

        Ok(Connection {
            id,
            project_id: project_id.to_string(),
            source_piece_id: source_piece_id.to_string(),
            target_piece_id: target_piece_id.to_string(),
            direction: Direction::Unidirectional,
            label: label.to_string(),
            data_type: None,
            protocol: None,
            constraints: vec![],
            notes: String::new(),
            metadata: HashMap::new(),
        })
    }

    pub fn get_connection(&self, id: &str) -> Result<Connection, String> {
        debug!(connection_id = id, "Getting connection");
        self.conn
            .query_row(
                "SELECT id, project_id, source_piece_id, target_piece_id, direction, label, data_type, protocol, constraints_json, notes, metadata_json FROM connections WHERE id = ?1",
                params![id],
                |row| {
                    let direction_str: String = row.get(4)?;
                    let constraints_json: String = row.get(8)?;
                    let metadata_json: String = row.get(10)?;

                    Ok(Connection {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        source_piece_id: row.get(2)?,
                        target_piece_id: row.get(3)?,
                        direction: serde_json::from_str(&format!("\"{}\"", direction_str)).unwrap_or(Direction::Unidirectional),
                        label: row.get(5)?,
                        data_type: row.get(6)?,
                        protocol: row.get(7)?,
                        constraints: parse_json_logged(&constraints_json, "connection_constraints"),
                        notes: row.get(9)?,
                        metadata: parse_json_logged(&metadata_json, "connection_metadata"),
                    })
                },
            )
            .map_err(|e| e.to_string())
    }

    pub fn update_connection(&self, id: &str, updates: &ConnectionUpdate) -> Result<Connection, String> {
        debug!(connection_id = id, "Updating connection");
        if let Some(ref label) = updates.label {
            self.conn.execute("UPDATE connections SET label = ?1 WHERE id = ?2", params![label, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref direction) = updates.direction {
            let json = serde_json::to_string(direction).map_err(|e| e.to_string())?;
            let val = json.trim_matches('"');
            self.conn.execute("UPDATE connections SET direction = ?1 WHERE id = ?2", params![val, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref data_type) = updates.data_type {
            self.conn.execute("UPDATE connections SET data_type = ?1 WHERE id = ?2", params![data_type, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref protocol) = updates.protocol {
            self.conn.execute("UPDATE connections SET protocol = ?1 WHERE id = ?2", params![protocol, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref constraints) = updates.constraints {
            let json = serde_json::to_string(constraints).map_err(|e| e.to_string())?;
            self.conn.execute("UPDATE connections SET constraints_json = ?1 WHERE id = ?2", params![json, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref notes) = updates.notes {
            self.conn.execute("UPDATE connections SET notes = ?1 WHERE id = ?2", params![notes, id]).map_err(|e| e.to_string())?;
        }
        if let Some(ref metadata) = updates.metadata {
            let json = serde_json::to_string(metadata).map_err(|e| e.to_string())?;
            self.conn.execute("UPDATE connections SET metadata_json = ?1 WHERE id = ?2", params![json, id]).map_err(|e| e.to_string())?;
        }

        self.get_connection(id)
    }

    pub fn delete_connection(&self, id: &str) -> Result<(), String> {
        debug!(connection_id = id, "Deleting connection");
        self.conn.execute("DELETE FROM connections WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_connections(&self, project_id: &str) -> Result<Vec<Connection>, String> {
        debug!(project_id, "Listing connections");
        let mut stmt = self.conn
            .prepare("SELECT id FROM connections WHERE project_id = ?1")
            .map_err(|e| e.to_string())?;

        let ids: Vec<String> = stmt
            .query_map(params![project_id], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        ids.iter().map(|id| self.get_connection(id)).collect()
    }

    // ── Project File I/O ─────────────────────────────────────

    pub fn export_project(&self, project_id: &str) -> Result<ProjectFile, String> {
        debug!(project_id, "Exporting project");
        let project = self.get_project(project_id)?;
        let pieces = self.list_pieces(project_id)?;
        let connections = self.list_connections(project_id)?;

        Ok(ProjectFile {
            project,
            pieces,
            connections,
        })
    }

    pub fn import_project(&self, file: &ProjectFile) -> Result<Project, String> {
        debug!(project_id = %file.project.id, name = %file.project.name, pieces = file.pieces.len(), connections = file.connections.len(), "Importing project");
        // Insert the project
        let now = chrono::Utc::now().to_rfc3339();
        let settings_json = serde_json::to_string(&file.project.settings).map_err(|e| e.to_string())?;

        self.conn.execute(
            "INSERT OR REPLACE INTO projects (id, name, description, root_piece_id, settings_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![file.project.id, file.project.name, file.project.description, file.project.root_piece_id, settings_json, file.project.created_at, now],
        ).map_err(|e| e.to_string())?;

        // Insert pieces
        for piece in &file.pieces {
            let interfaces_json = serde_json::to_string(&piece.interfaces).map_err(|e| e.to_string())?;
            let constraints_json = serde_json::to_string(&piece.constraints).map_err(|e| e.to_string())?;
            let agent_config_json = serde_json::to_string(&piece.agent_config).map_err(|e| e.to_string())?;
            let output_mode_json = serde_json::to_string(&piece.output_mode).map_err(|e| e.to_string())?;
            let output_mode_val = output_mode_json.trim_matches('"');
            let phase_json = serde_json::to_string(&piece.phase).map_err(|e| e.to_string())?;
            let phase_val = phase_json.trim_matches('"');

            self.conn.execute(
                "INSERT OR REPLACE INTO pieces (id, project_id, parent_id, name, piece_type, color, icon, responsibilities, interfaces_json, constraints_json, notes, agent_prompt, agent_config_json, output_mode, phase, position_x, position_y, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
                params![piece.id, piece.project_id, piece.parent_id, piece.name, piece.piece_type, piece.color, piece.icon, piece.responsibilities, interfaces_json, constraints_json, piece.notes, piece.agent_prompt, agent_config_json, output_mode_val, phase_val, piece.position_x, piece.position_y, piece.created_at, now],
            ).map_err(|e| e.to_string())?;
        }

        // Insert connections
        for conn in &file.connections {
            let constraints_json = serde_json::to_string(&conn.constraints).map_err(|e| e.to_string())?;
            let metadata_json = serde_json::to_string(&conn.metadata).map_err(|e| e.to_string())?;
            let direction_json = serde_json::to_string(&conn.direction).map_err(|e| e.to_string())?;
            let direction_val = direction_json.trim_matches('"');

            self.conn.execute(
                "INSERT OR REPLACE INTO connections (id, project_id, source_piece_id, target_piece_id, direction, label, data_type, protocol, constraints_json, notes, metadata_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![conn.id, conn.project_id, conn.source_piece_id, conn.target_piece_id, direction_val, conn.label, conn.data_type, conn.protocol, constraints_json, conn.notes, metadata_json],
            ).map_err(|e| e.to_string())?;
        }

        self.get_project(&file.project.id)
    }
}

/// Update structs used by commands
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PieceUpdate {
    pub name: Option<String>,
    pub piece_type: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub responsibilities: Option<String>,
    pub interfaces: Option<Vec<Interface>>,
    pub constraints: Option<Vec<Constraint>>,
    pub notes: Option<String>,
    pub agent_prompt: Option<String>,
    pub agent_config: Option<AgentConfig>,
    pub output_mode: Option<OutputMode>,
    pub phase: Option<Phase>,
    pub position_x: Option<f64>,
    pub position_y: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionUpdate {
    pub label: Option<String>,
    pub direction: Option<Direction>,
    pub data_type: Option<String>,
    pub protocol: Option<String>,
    pub constraints: Option<Vec<Constraint>>,
    pub notes: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
}
