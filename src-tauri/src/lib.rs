mod commands;
mod import;
mod ledger;
mod storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(debug_assertions)]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::ledger::init_db,
            commands::ledger::create_transfer,
            commands::accounts::list_accounts,
            commands::accounts::create_account,
            commands::accounts::update_account,
            commands::import::parse_csv_preview,
            commands::import::preview_csv_import,
            commands::import::preview_ofx_import,
            commands::import::import_csv,
            commands::import::import_ofx,
            commands::transactions::list_transactions,
            commands::transactions::upsert_transaction_meta,
            commands::categories::list_categories,
            commands::categories::create_category,
            commands::categories::update_category,
            commands::categories::upsert_split,
            commands::categories::list_income_categories,
            commands::categories::create_income_category,
            commands::categories::set_income_category_hidden,
            commands::groups::list_category_groups,
            commands::groups::create_category_group,
            commands::groups::update_category_group,
            commands::groups::delete_category_group,
            commands::groups::assign_category_to_group,
            commands::budget::get_budget_month,
            commands::budget::set_allocation,
            commands::budget::reallocate,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
