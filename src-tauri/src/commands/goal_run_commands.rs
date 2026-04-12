use crate::db::Database;
use crate::models::{GoalRun, GoalRunUpdate};
use crate::AppState;
use tauri::State;

pub(crate) fn create_goal_run_impl(
    db: &std::sync::Mutex<Database>,
    project_id: String,
    prompt: String,
) -> Result<GoalRun, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.create_goal_run(&project_id, &prompt)
}

pub(crate) fn update_goal_run_impl(
    db: &std::sync::Mutex<Database>,
    goal_run_id: String,
    updates: GoalRunUpdate,
) -> Result<GoalRun, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.update_goal_run(&goal_run_id, &updates)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn create_goal_run(
    state: State<'_, AppState>,
    project_id: String,
    prompt: String,
) -> Result<GoalRun, String> {
    create_goal_run_impl(&state.db, project_id, prompt)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn get_goal_run(state: State<'_, AppState>, goal_run_id: String) -> Result<GoalRun, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_goal_run(&goal_run_id)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn list_goal_runs(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<GoalRun>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_goal_runs(&project_id)
}

#[tracing::instrument(skip(state))]
#[tauri::command]
pub fn update_goal_run(
    state: State<'_, AppState>,
    goal_run_id: String,
    updates: GoalRunUpdate,
) -> Result<GoalRun, String> {
    update_goal_run_impl(&state.db, goal_run_id, updates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::{GoalRunPhase, GoalRunStatus, GoalRunUpdate};
    use std::sync::Mutex;

    #[test]
    fn create_update_and_list_goal_runs_via_command_helpers() {
        let db_path = std::env::temp_dir().join(format!(
            "project-builder-goal-run-command-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&db_path).expect("create temp directory");
        let sqlite_path = db_path.join("data.db");
        let db = Database::new_at_path(&sqlite_path).expect("open db");
        let state = Mutex::new(db);

        let project = {
            let db = state.lock().expect("lock db");
            db.create_project("Command project", "Testing goal run commands")
                .expect("create project")
        };

        let created = create_goal_run_impl(
            &state,
            project.id.clone(),
            "Build a todo app".to_string(),
        )
        .expect("create goal run");
        assert_eq!(created.project_id, project.id);
        assert_eq!(created.phase, GoalRunPhase::PromptReceived);
        assert_eq!(created.status, GoalRunStatus::Running);

        let updated = update_goal_run_impl(
            &state,
            created.id.clone(),
            GoalRunUpdate {
                phase: Some(GoalRunPhase::Planning),
                status: Some(GoalRunStatus::Blocked),
                blocker_reason: Some(Some("Waiting on plan".to_string())),
                retry_count: Some(1),
                ..Default::default()
            },
        )
        .expect("update goal run");
        assert_eq!(updated.phase, GoalRunPhase::Planning);
        assert_eq!(updated.status, GoalRunStatus::Blocked);
        assert_eq!(updated.retry_count, 1);
        assert_eq!(updated.blocker_reason.as_deref(), Some("Waiting on plan"));

        let listed = {
            let db = state.lock().expect("lock db");
            db.list_goal_runs(&project.id).expect("list goal runs")
        };
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);

        let _ = std::fs::remove_dir_all(&db_path);
    }
}

