mod commands;
mod import;
mod ledger;
mod storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::ledger::init_db,
            commands::accounts::list_accounts,
            commands::accounts::create_account,
            commands::accounts::update_account,
            commands::import::parse_csv_preview,
            commands::import::preview_csv_import,
            commands::import::preview_ofx_import,
            commands::import::import_csv,
            commands::import::import_ofx,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
