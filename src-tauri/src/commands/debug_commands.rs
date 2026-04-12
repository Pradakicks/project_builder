use crate::models::{
    DebugLogTail, DebugScenarioRecord, DebugScenarioRecordInput, DebugSessionInfo,
};
use std::fs;
use std::path::PathBuf;

const DEBUG_SESSION_DIR_ENV: &str = "PROJECT_BUILDER_DEBUG_SESSION_DIR";
const DEBUG_SESSION_ID_ENV: &str = "PROJECT_BUILDER_DEBUG_SESSION_ID";
const DEBUG_SESSION_STARTED_AT_ENV: &str = "PROJECT_BUILDER_DEBUG_SESSION_STARTED_AT";
const DEBUG_LOG_PATH_ENV: &str = "PROJECT_BUILDER_DEBUG_LOG_PATH";

fn debug_session_dir() -> Option<PathBuf> {
    std::env::var_os(DEBUG_SESSION_DIR_ENV).map(PathBuf::from)
}

fn debug_log_path() -> Option<PathBuf> {
    std::env::var_os(DEBUG_LOG_PATH_ENV).map(PathBuf::from)
}

fn latest_scenario_path() -> Option<PathBuf> {
    debug_session_dir().map(|dir| dir.join("latest-scenario.json"))
}

#[tracing::instrument]
#[tauri::command]
pub fn get_debug_session_info() -> DebugSessionInfo {
    let session_dir = debug_session_dir();
    let log_path = debug_log_path();
    DebugSessionInfo {
        enabled: cfg!(debug_assertions) && session_dir.is_some(),
        session_id: std::env::var(DEBUG_SESSION_ID_ENV).ok(),
        session_dir: session_dir.map(|path| path.display().to_string()),
        started_at: std::env::var(DEBUG_SESSION_STARTED_AT_ENV).ok(),
        log_path: log_path.map(|path| path.display().to_string()),
    }
}

#[tracing::instrument(skip(scenario))]
#[tauri::command]
pub fn record_debug_scenario(
    scenario: DebugScenarioRecordInput,
) -> Result<DebugScenarioRecord, String> {
    if !cfg!(debug_assertions) {
        return Ok(DebugScenarioRecord {
            scenario,
            path: None,
        });
    }

    let Some(path) = latest_scenario_path() else {
        return Ok(DebugScenarioRecord {
            scenario,
            path: None,
        });
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let json = serde_json::to_string_pretty(&scenario).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;

    Ok(DebugScenarioRecord {
        scenario,
        path: Some(path.display().to_string()),
    })
}

#[tracing::instrument]
#[tauri::command]
pub fn get_last_debug_scenario() -> Result<Option<DebugScenarioRecord>, String> {
    if !cfg!(debug_assertions) {
        return Ok(None);
    }

    let Some(path) = latest_scenario_path() else {
        return Ok(None);
    };

    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let scenario =
        serde_json::from_str::<DebugScenarioRecordInput>(&raw).map_err(|e| e.to_string())?;
    Ok(Some(DebugScenarioRecord {
        scenario,
        path: Some(path.display().to_string()),
    }))
}

#[tracing::instrument]
#[tauri::command]
pub fn read_debug_log_tail(limit: Option<usize>) -> Result<DebugLogTail, String> {
    if !cfg!(debug_assertions) {
        return Ok(DebugLogTail {
            path: None,
            lines: Vec::new(),
        });
    }

    let Some(path) = debug_log_path() else {
        return Ok(DebugLogTail {
            path: None,
            lines: Vec::new(),
        });
    };

    if !path.exists() {
        return Ok(DebugLogTail {
            path: Some(path.display().to_string()),
            lines: Vec::new(),
        });
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let requested = limit.unwrap_or(120).max(1);
    let lines = raw
        .lines()
        .rev()
        .take(requested)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(str::to_string)
        .collect();

    Ok(DebugLogTail {
        path: Some(path.display().to_string()),
        lines,
    })
}
