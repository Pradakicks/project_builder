//! Settings commands: OS keychain API key management + project settings updates.
//! Keys are stored via the `keyring` crate (macOS Keychain, Windows Credential Manager,
//! Linux Secret Service). Agent code resolves keys via keyring first, env var fallback.

use crate::models::ProjectSettings;
use crate::AppState;
use tauri::State;
use tracing::{info, debug};

const SERVICE_NAME: &str = "project-builder-dashboard";

/// Read an API key from the OS keychain. Returns None if no key is stored.
#[tracing::instrument(fields(provider = %provider))]
#[tauri::command]
pub fn get_api_key(provider: String) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE_NAME, &provider).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Store an API key in the OS keychain.
#[tracing::instrument(skip(key), fields(provider = %provider))]
#[tauri::command]
pub fn set_api_key(provider: String, key: String) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, &provider).map_err(|e| e.to_string())?;
    entry.set_password(&key).map_err(|e| e.to_string())?;
    info!(provider = %provider, "API key saved to keyring");
    Ok(())
}

/// Remove an API key from the OS keychain. No-op if not present.
#[tracing::instrument(fields(provider = %provider))]
#[tauri::command]
pub fn delete_api_key(provider: String) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, &provider).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => {},
        Err(keyring::Error::NoEntry) => {}, // already gone
        Err(e) => return Err(e.to_string()),
    }
    info!(provider = %provider, "API key deleted from keyring");
    Ok(())
}

/// Validate that a path is an existing directory containing a .git folder.
#[tracing::instrument]
#[tauri::command]
pub fn validate_working_directory(path: String) -> Result<bool, String> {
    let p = std::path::Path::new(&path);
    if !p.exists() || !p.is_dir() {
        return Err("Directory does not exist".into());
    }
    if !p.join(".git").exists() {
        return Err("Not a git repository (no .git directory found)".into());
    }
    Ok(true)
}

/// Persist project-level settings (token budget, phase control, LLM configs).
#[tracing::instrument(skip(state, settings), fields(project_id = %id))]
#[tauri::command]
pub fn update_project_settings(
    state: State<AppState>,
    id: String,
    settings: ProjectSettings,
) -> Result<crate::models::Project, String> {
    debug!(project_id = %id, "Updating project settings");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.update_project(&id, None, None, None, Some(&settings))
}
