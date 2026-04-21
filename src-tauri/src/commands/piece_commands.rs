use crate::agent::validate_phase_transition;
use crate::db::PieceUpdate;
use crate::models::Piece;
use crate::AppState;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn create_piece(
    state: State<AppState>,
    project_id: String,
    parent_id: Option<String>,
    name: String,
    position_x: f64,
    position_y: f64,
    updates: Option<crate::db::PieceUpdate>,
) -> Result<Piece, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let piece = db.create_piece(&project_id, parent_id.as_deref(), &name, position_x, position_y)?;
    if let Some(updates) = updates {
        db.update_piece(&piece.id, &updates)
    } else {
        Ok(piece)
    }
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn get_piece(state: State<AppState>, id: String) -> Result<Piece, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_piece(&id)
}

#[tracing::instrument(skip(state, app_handle, updates), fields(piece_id = %id))]
#[tauri::command]
pub fn update_piece(
    state: State<AppState>,
    app_handle: AppHandle,
    id: String,
    updates: PieceUpdate,
) -> Result<Piece, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;

    // Soft validation: warn if phase transition skips steps
    if let Some(ref new_phase) = updates.phase {
        let old_piece = db.get_piece(&id)?;
        if let Some(warning) = validate_phase_transition(&old_piece.phase, new_phase) {
            let _ = app_handle.emit(
                "phase-warning",
                json!({ "pieceId": id, "warning": warning }),
            );
        }
    }

    db.update_piece(&id, &updates)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn delete_piece(state: State<AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_piece(&id)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn list_pieces(state: State<AppState>, project_id: String) -> Result<Vec<Piece>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_pieces(&project_id)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn list_children(state: State<AppState>, piece_id: String) -> Result<Vec<Piece>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_children(&piece_id)
}

/// List distinct team names in a project. Empty result = no teams configured.
#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn list_teams_for_project(
    state: State<AppState>,
    project_id: String,
) -> Result<Vec<String>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_teams_for_project(&project_id)
}

/// List every team brief for a project, newest first. Powers the debug
/// report and the ProjectStatusBar teams chip.
#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn list_team_briefs(
    state: State<AppState>,
    project_id: String,
) -> Result<Vec<crate::models::TeamBrief>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_team_briefs_for_project(&project_id, None)
}

#[cfg(test)]
mod tests {
    use crate::db::{Database, PieceUpdate};
    use crate::models::{OutputMode, Phase};
    use std::sync::Mutex;

    #[test]
    fn create_piece_applies_initial_updates() {
        let db_path = std::env::temp_dir().join(format!(
            "project-builder-piece-command-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&db_path).expect("create temp directory");
        let sqlite_path = db_path.join("data.db");
        let db = Database::new_at_path(&sqlite_path).expect("open db");
        let state = Mutex::new(db);

        let project = {
            let db = state.lock().expect("lock db");
            db.create_project("Command project", "Testing piece commands")
                .expect("create project")
        };

        let piece = {
            let db = state.lock().expect("lock db");
            let piece = db
                .create_piece(&project.id, None, "Todo App", 10.0, 20.0)
                .expect("create piece");
            db.update_piece(
                &piece.id,
                &PieceUpdate {
                    piece_type: Some("web-app".to_string()),
                    responsibilities: Some("Build the todo app".to_string()),
                    agent_prompt: Some("Write the project files".to_string()),
                    output_mode: Some(OutputMode::CodeOnly),
                    phase: Some(Phase::Approved),
                    ..Default::default()
                },
            )
            .expect("update piece")
        };

        assert_eq!(piece.piece_type, "web-app");
        assert_eq!(piece.responsibilities, "Build the todo app");
        assert_eq!(piece.agent_prompt, "Write the project files");
        assert!(matches!(piece.output_mode, OutputMode::CodeOnly));
        assert!(matches!(piece.phase, Phase::Approved));

        let _ = std::fs::remove_dir_all(&db_path);
    }
}
