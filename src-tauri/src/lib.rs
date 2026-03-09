mod commands;
mod db;
pub mod models;
mod llm;
mod agent;

use db::Database;
use std::sync::Mutex;

pub struct AppState {
    pub db: Mutex<Database>,
}

pub fn run() {
    let database = Database::new().expect("Failed to initialize database");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            db: Mutex::new(database),
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
            commands::agent_commands::run_piece_agent,
            commands::agent_commands::get_agent_history,
            commands::agent_commands::chat_with_cto,
            commands::settings_commands::get_api_key,
            commands::settings_commands::set_api_key,
            commands::settings_commands::delete_api_key,
            commands::settings_commands::update_project_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
