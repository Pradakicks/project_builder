use crate::agent;
use crate::models::{PlanStatus, TaskStatus, WorkPlan, WorkPlanUpdate};
use crate::AppState;
use tauri::{AppHandle, State};
use tracing::{info, debug};

#[tracing::instrument(skip(state, app_handle), fields(project_id = %project_id))]
#[tauri::command]
pub async fn generate_work_plan(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    project_id: String,
    user_guidance: String,
) -> Result<WorkPlan, String> {
    info!(project_id = %project_id, "IPC: generate_work_plan");
    let db = &state.db;
    agent::runner::run_leader_agent(&project_id, &user_guidance, db, &app_handle).await
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn get_work_plan(
    state: State<'_, AppState>,
    plan_id: String,
) -> Result<WorkPlan, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_work_plan(&plan_id)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn list_work_plans(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<WorkPlan>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_work_plans(&project_id)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn update_plan_status(
    state: State<'_, AppState>,
    plan_id: String,
    status: PlanStatus,
) -> Result<WorkPlan, String> {
    info!(plan_id = %plan_id, status = ?status, "IPC: update_plan_status");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.update_work_plan(
        &plan_id,
        &WorkPlanUpdate {
            status: Some(status),
            ..Default::default()
        },
    )
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn update_plan_task_status(
    state: State<'_, AppState>,
    plan_id: String,
    task_id: String,
    status: TaskStatus,
) -> Result<WorkPlan, String> {
    debug!(plan_id = %plan_id, task_id = %task_id, status = ?status, "IPC: update_plan_task_status");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let plan = db.get_work_plan(&plan_id)?;

    let mut tasks = plan.tasks;
    if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
        task.status = status;
    } else {
        return Err(format!("Task '{}' not found in plan", task_id));
    }

    db.update_work_plan(
        &plan_id,
        &WorkPlanUpdate {
            tasks: Some(tasks),
            ..Default::default()
        },
    )
}

#[tracing::instrument(skip(state, app_handle))]
#[tauri::command]
pub async fn run_all_plan_tasks(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    plan_id: String,
) -> Result<(), String> {
    info!(plan_id = %plan_id, "IPC: run_all_plan_tasks");
    agent::runner::run_all_plan_tasks(
        &plan_id,
        None,
        &state.db,
        &state.running_pieces,
        &app_handle,
    )
    .await
}
