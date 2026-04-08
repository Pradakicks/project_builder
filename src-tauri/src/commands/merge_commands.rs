use crate::agent::merge;
use crate::AppState;
use tauri::{AppHandle, State};
use tracing::info;

#[tracing::instrument(skip(state, app_handle))]
#[tauri::command]
pub async fn merge_plan_branches(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    plan_id: String,
) -> Result<merge::MergeSummary, String> {
    info!("IPC: merge_plan_branches");
    merge::merge_plan_branches(&plan_id, &state.db, &app_handle).await
}

#[tracing::instrument(skip(state, app_handle))]
#[tauri::command]
pub async fn resolve_merge_conflict(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    plan_id: String,
    piece_id: String,
) -> Result<(), String> {
    info!("IPC: resolve_merge_conflict");
    merge::resolve_merge_conflict(&plan_id, &piece_id, &state.db, &app_handle).await
}

#[tracing::instrument(skip(state, app_handle))]
#[tauri::command]
pub async fn run_integration_review(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    plan_id: String,
) -> Result<(), String> {
    info!("IPC: run_integration_review");
    merge::run_integration_review(&plan_id, &state.db, &app_handle).await?;
    Ok(())
}
