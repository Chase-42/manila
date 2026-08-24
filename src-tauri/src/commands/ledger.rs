use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::storage::{db::open_connection, migrations::run_migrations};

/// Open the database, run pending migrations, and register the connection in
/// Tauri state. Must be called once at app startup before any other command
/// that needs the database. Future commands extract it via State<Mutex<Connection>>.
#[tauri::command]
pub fn init_db(app: AppHandle) -> Result<(), String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;

    let db_path = app_dir.join("manila.db");
    let path_str = db_path.to_str().ok_or("app data path is not valid UTF-8")?;

    let mut conn = open_connection(path_str).map_err(|e| e.to_string())?;
    run_migrations(&mut conn).map_err(|e| e.to_string())?;
    crate::storage::seed::seed_categories(&conn).map_err(|e| e.to_string())?;

    app.manage(Mutex::new(conn));
    Ok(())
}
