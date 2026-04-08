use crate::models::{Project, ProjectFile};
use crate::AppState;
use tauri::State;

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn create_project(state: State<AppState>, name: String, description: String) -> Result<Project, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.create_project(&name, &description)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn get_project(state: State<AppState>, id: String) -> Result<Project, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_project(&id)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn update_project(
    state: State<AppState>,
    id: String,
    name: Option<String>,
    description: Option<String>,
) -> Result<Project, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.update_project(&id, name.as_deref(), description.as_deref(), None, None)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn list_projects(state: State<AppState>) -> Result<Vec<Project>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_projects()
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn delete_project(state: State<AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_project(&id)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn save_project_to_file(state: State<AppState>, id: String, path: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let project_file = db.export_project(&id)?;
    let json = serde_json::to_string_pretty(&project_file).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn load_project_from_file(state: State<AppState>, path: String) -> Result<Project, String> {
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let project_file: ProjectFile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.import_project(&project_file)
}
