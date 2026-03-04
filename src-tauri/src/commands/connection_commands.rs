use crate::db::ConnectionUpdate;
use crate::models::Connection;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub fn create_connection(
    state: State<AppState>,
    project_id: String,
    source_piece_id: String,
    target_piece_id: String,
    label: String,
) -> Result<Connection, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.create_connection(&project_id, &source_piece_id, &target_piece_id, &label)
}

#[tauri::command]
pub fn get_connection(state: State<AppState>, id: String) -> Result<Connection, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_connection(&id)
}

#[tauri::command]
pub fn update_connection(state: State<AppState>, id: String, updates: ConnectionUpdate) -> Result<Connection, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.update_connection(&id, &updates)
}

#[tauri::command]
pub fn delete_connection(state: State<AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_connection(&id)
}

#[tauri::command]
pub fn list_connections(state: State<AppState>, project_id: String) -> Result<Vec<Connection>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_connections(&project_id)
}
