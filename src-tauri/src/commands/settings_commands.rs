//! Settings commands: OS keychain API key management + project settings updates.
//! Keys are stored via the `keyring` crate (macOS Keychain, Windows Credential Manager,
//! Linux Secret Service). Agent code resolves keys via keyring first, env var fallback.

use crate::models::ProjectSettings;
use crate::AppState;
use tauri::State;

const SERVICE_NAME: &str = "project-builder-dashboard";

/// Read an API key from the OS keychain. Returns None if no key is stored.
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
#[tauri::command]
pub fn set_api_key(provider: String, key: String) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, &provider).map_err(|e| e.to_string())?;
    entry.set_password(&key).map_err(|e| e.to_string())
}

/// Remove an API key from the OS keychain. No-op if not present.
#[tauri::command]
pub fn delete_api_key(provider: String) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, &provider).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // already gone
        Err(e) => Err(e.to_string()),
    }
}

/// Persist project-level settings (token budget, phase control, LLM configs).
#[tauri::command]
pub fn update_project_settings(
    state: State<AppState>,
    id: String,
    settings: ProjectSettings,
) -> Result<crate::models::Project, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.update_project(&id, None, None, None, Some(&settings))
}
