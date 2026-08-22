mod commands;
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
