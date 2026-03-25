use crate::agent;
use crate::models::{PlanStatus, TaskStatus, WorkPlan, WorkPlanUpdate};
use crate::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn generate_work_plan(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    project_id: String,
    user_guidance: String,
) -> Result<WorkPlan, String> {
    let db = &state.db;
    agent::runner::run_leader_agent(&project_id, &user_guidance, db, &app_handle).await
}

#[tauri::command]
pub fn get_work_plan(
    state: State<'_, AppState>,
    plan_id: String,
) -> Result<WorkPlan, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_work_plan(&plan_id)
}

#[tauri::command]
pub fn list_work_plans(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<WorkPlan>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_work_plans(&project_id)
}

#[tauri::command]
pub fn update_plan_status(
    state: State<'_, AppState>,
    plan_id: String,
    status: PlanStatus,
) -> Result<WorkPlan, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.update_work_plan(
        &plan_id,
        &WorkPlanUpdate {
            status: Some(status),
            ..Default::default()
        },
    )
}

#[tauri::command]
pub fn update_plan_task_status(
    state: State<'_, AppState>,
    plan_id: String,
    task_id: String,
    status: TaskStatus,
) -> Result<WorkPlan, String> {
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
