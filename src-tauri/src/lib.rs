mod commands;
mod crypto;
mod import;
mod ledger;
mod storage;

use crypto::VaultState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(debug_assertions)]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .plugin(tauri_plugin_opener::init())
        .manage(VaultState(std::sync::Mutex::new(None)))
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
            commands::transactions::export_transactions_csv,
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
            commands::budget::get_home_view,
            commands::budget::set_allocation,
            commands::budget::reallocate,
            commands::budget::close_month,
            commands::search::search_transactions,
            commands::reports::get_spending_by_category,
            commands::reports::get_monthly_spend_trend,
            commands::goals::list_goals_with_progress,
            commands::goals::create_goal,
            commands::goals::update_goal,
            commands::goals::delete_goal,
            commands::vault::create_vault,
            commands::vault::vault_status,
            commands::vault::unlock_vault,
            commands::vault::lock_vault,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
