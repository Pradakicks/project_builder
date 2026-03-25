use crate::agent::validate_phase_transition;
use crate::db::PieceUpdate;
use crate::models::Piece;
use crate::AppState;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn create_piece(
    state: State<AppState>,
    project_id: String,
    parent_id: Option<String>,
    name: String,
    position_x: f64,
    position_y: f64,
) -> Result<Piece, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.create_piece(&project_id, parent_id.as_deref(), &name, position_x, position_y)
}

#[tauri::command]
pub fn get_piece(state: State<AppState>, id: String) -> Result<Piece, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_piece(&id)
}

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

#[tauri::command]
pub fn delete_piece(state: State<AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_piece(&id)
}

#[tauri::command]
pub fn list_pieces(state: State<AppState>, project_id: String) -> Result<Vec<Piece>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_pieces(&project_id)
}

#[tauri::command]
pub fn list_children(state: State<AppState>, piece_id: String) -> Result<Vec<Piece>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_children(&piece_id)
}
