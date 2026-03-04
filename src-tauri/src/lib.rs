mod commands;
mod db;
pub mod models;

use db::Database;
use std::sync::Mutex;

pub struct AppState {
    pub db: Mutex<Database>,
}

pub fn run() {
    let database = Database::new().expect("Failed to initialize database");

    tauri::Builder::default()
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
