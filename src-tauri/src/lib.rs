mod agent;
mod commands;
mod db;
mod llm;
pub mod models;
#[cfg(test)]
mod test_support;

use db::Database;
use std::collections::HashSet;
use std::sync::Mutex;
use tracing::info;

pub struct AppState {
    pub db: Mutex<Database>,
    /// Tracks which pieces currently have an agent running (prevents double-runs).
    pub running_pieces: Mutex<HashSet<String>>,
    pub running_goal_runs: Mutex<HashSet<String>>,
    pub runtime_sessions: Mutex<commands::runtime_commands::RuntimeSessions>,
}

pub fn run() {
    // Initialize development logging (compiled out in release builds)
    #[cfg(debug_assertions)]
    {
        use tracing_subscriber::{fmt, EnvFilter};
        fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                    EnvFilter::new("project_builder_dashboard_lib=debug,project_builder_dashboard_lib::agent=trace,project_builder_dashboard_lib::llm=trace")
                }),
            )
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .init();
        tracing::info!("Development logging initialized");
    }

    let database = Database::new().expect("Failed to initialize database");

    // On startup, mark any goal runs that were mid-execution as interrupted.
    // This covers the case where the app was force-quit while the autopilot was running.
    {
        let count = database.mark_all_interrupted_runs().unwrap_or(0);
        if count > 0 {
            info!(count, "Marked interrupted goal runs on startup");
        }
        let runtime_count = database.mark_runtime_sessions_interrupted().unwrap_or(0);
        if runtime_count > 0 {
            info!(runtime_count, "Marked stale runtime sessions on startup");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            db: Mutex::new(database),
            running_pieces: Mutex::new(HashSet::new()),
            running_goal_runs: Mutex::new(HashSet::new()),
            runtime_sessions: Mutex::new(std::collections::HashMap::new()),
        })
        .invoke_handler(tauri::generate_handler![
            commands::project_commands::create_project,
            commands::project_commands::get_project,
            commands::project_commands::update_project,
            commands::project_commands::list_projects,
            commands::project_commands::delete_project,
            commands::project_commands::save_project_to_file,
            commands::project_commands::load_project_from_file,
            commands::piece_commands::create_piece,
            commands::piece_commands::get_piece,
            commands::piece_commands::update_piece,
            commands::piece_commands::delete_piece,
            commands::piece_commands::list_pieces,
            commands::piece_commands::list_children,
            commands::connection_commands::create_connection,
            commands::connection_commands::get_connection,
            commands::connection_commands::update_connection,
            commands::connection_commands::delete_connection,
            commands::connection_commands::list_connections,
            commands::goal_run_commands::create_goal_run,
            commands::goal_run_commands::start_goal_run,
            commands::goal_run_commands::get_goal_run,
            commands::goal_run_commands::list_goal_runs,
            commands::goal_run_commands::update_goal_run,
            commands::goal_run_commands::get_goal_run_delivery_snapshot,
            commands::goal_run_commands::resume_goal_run,
            commands::goal_run_commands::stop_goal_run,
            commands::goal_run_commands::get_goal_run_events,
            commands::agent_commands::run_piece_agent,
            commands::agent_commands::get_agent_history,
            commands::agent_commands::chat_with_cto,
            commands::agent_commands::get_git_status,
            commands::agent_commands::list_artifacts,
            commands::cto_action_engine::review_cto_actions,
            commands::cto_action_engine::execute_cto_actions,
            commands::agent_commands::log_cto_decision,
            commands::agent_commands::list_cto_decisions,
            commands::agent_commands::rollback_cto_decision,
            commands::settings_commands::get_api_key,
            commands::settings_commands::set_api_key,
            commands::settings_commands::delete_api_key,
            commands::settings_commands::update_project_settings,
            commands::settings_commands::validate_working_directory,
            commands::runtime_commands::configure_runtime,
            commands::runtime_commands::get_runtime_status,
            commands::runtime_commands::detect_runtime,
            commands::runtime_commands::detect_runtime_with_agent,
            commands::runtime_commands::start_runtime,
            commands::runtime_commands::stop_runtime,
            commands::runtime_commands::tail_runtime_logs,
            commands::runtime_commands::verify_runtime,
            commands::plan_commands::generate_work_plan,
            commands::plan_commands::get_work_plan,
            commands::plan_commands::list_work_plans,
            commands::plan_commands::update_plan_status,
            commands::plan_commands::update_plan_task_status,
            commands::plan_commands::run_all_plan_tasks,
            commands::merge_commands::merge_plan_branches,
            commands::merge_commands::resolve_merge_conflict,
            commands::merge_commands::run_integration_review,
            commands::debug_commands::get_debug_session_info,
            commands::debug_commands::record_debug_scenario,
            commands::debug_commands::get_last_debug_scenario,
            commands::debug_commands::read_debug_log_tail,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
