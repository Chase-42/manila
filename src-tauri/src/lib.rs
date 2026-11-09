#![forbid(unsafe_code)]

mod commands;
mod crypto;
mod import;
mod ledger;
mod storage;
mod sync;
pub(crate) mod types;

use crypto::{OnboardingState, UnlockedVault, VaultState};
use tauri::Emitter;

// Tauri's Builder::run() returns Result but the entry point has nowhere to propagate it.
#[allow(clippy::expect_used)]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(debug_assertions)]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .manage(VaultState(std::sync::Mutex::new(None::<UnlockedVault>)))
        .manage(OnboardingState(std::sync::Mutex::new(None)))
        .setup(|app| {
            #[cfg(desktop)]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        let url_str = url.to_string();
                        handle
                            .emit("plaid-oauth-redirect", serde_json::json!({ "url": url_str }))
                            .ok();
                    }
                });
            }
            Ok(())
        })
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
            commands::plaid::list_plaid_connections,
            commands::plaid::upsert_plaid_connection,
            commands::plaid::delete_plaid_connection,
            commands::plaid::get_plaid_link_token,
            commands::plaid::exchange_plaid_token,
            commands::plaid_sync::sync_plaid_connection,
            commands::vault::create_vault,
            commands::vault::vault_status,
            commands::vault::unlock_vault,
            commands::vault::lock_vault,
            commands::vault::generate_recovery_phrase,
            commands::vault::acknowledge_recovery_phrase,
            commands::vault::restore_from_phrase,
            commands::vault::enroll_vault_from_phrase,
            commands::vault::get_enrollment_phrase,
            commands::vault::check_biometric_availability,
            commands::vault::biometric_label,
            commands::vault::enable_biometric_unlock,
            commands::vault::disable_biometric_unlock,
            commands::vault::biometric_unlock,
            commands::vault::change_password,
            commands::sync::configure_sync,
            commands::sync::get_sync_config,
            commands::sync::enroll_sync,
            commands::sync::run_sync,
            commands::sync::get_sync_status,
            commands::sync::revoke_sync,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
