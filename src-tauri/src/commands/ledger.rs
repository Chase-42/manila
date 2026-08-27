use rusqlite::Connection;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

use crate::ledger::engine::{self, PostingInput};
use crate::storage::{db::open_connection, migrations::run_migrations};

/// Open the database, run pending migrations, and register the connection in
/// Tauri state. Must be called once at app startup before any other command
/// that needs the database. Future commands extract it via State<Mutex<Connection>>.
#[tauri::command]
pub fn init_db(app: AppHandle) -> Result<(), String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;

    std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;

    let db_path = app_dir.join("manila.db");
    let path_str = db_path.to_str().ok_or("app data path is not valid UTF-8")?;

    let mut conn = open_connection(path_str).map_err(|e| e.to_string())?;
    run_migrations(&mut conn).map_err(|e| e.to_string())?;
    crate::storage::seed::seed_categories(&conn).map_err(|e| e.to_string())?;
    crate::storage::seed::seed_category_groups(&conn).map_err(|e| e.to_string())?;
    crate::storage::seed::seed_income_categories(&conn).map_err(|e| e.to_string())?;

    app.manage(Mutex::new(conn));
    Ok(())
}

/// Create an account-to-account transfer (e.g. credit card payment).
///
/// amount_cents is the positive amount being moved; sign is applied internally:
/// outflow from from_account, inflow to to_account.
#[tauri::command]
pub fn create_transfer(
    db: State<Mutex<Connection>>,
    from_account_id: String,
    to_account_id: String,
    date: String,
    amount_cents: i64,
    description: String,
) -> Result<String, String> {
    let mut conn = db.lock().map_err(|e| e.to_string())?;
    let postings = [
        PostingInput {
            account_id: from_account_id.clone(),
            amount_cents: -amount_cents,
        },
        PostingInput {
            account_id: to_account_id,
            amount_cents,
        },
    ];
    engine::create_transfer(
        &mut conn,
        &from_account_id,
        &date,
        -amount_cents,
        &description,
        &postings,
    )
    .map_err(|e| e.to_string())
}
