use tauri::{AppHandle, Manager, State};

use crate::ledger::engine::{self, PostingInput};
use crate::types::Cents;

/// Create the app data directory and handle cold-start migration.
/// Does not open the encrypted database (that happens at vault unlock).
#[tauri::command]
pub fn init_db(app: AppHandle) -> Result<(), String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;

    // If manila.db exists but vault.json does not, this is a pre-encryption install.
    // Rename the plain db so the new encrypted db starts fresh.
    let db_path = app_dir.join("manila.db");
    let vault_path = app_dir.join("vault.json");
    if db_path.exists() && !vault_path.exists() {
        let backup = app_dir.join("manila_plaintext_backup.db");
        std::fs::rename(&db_path, &backup).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Create an account-to-account transfer (e.g. credit card payment).
///
/// amount_cents is the positive amount being moved; sign is applied internally:
/// outflow from from_account, inflow to to_account.
#[tauri::command]
pub fn create_transfer(
    vault: State<'_, crate::crypto::VaultState>,
    from_account_id: String,
    to_account_id: String,
    date: String,
    amount_cents: Cents,
    description: String,
) -> Result<String, String> {
    let mut guard = vault
        .0
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;
    let unlocked = guard.as_mut().ok_or("locked")?;
    let postings = [
        PostingInput {
            account_id: from_account_id.clone().into(),
            amount_cents: -amount_cents,
        },
        PostingInput {
            account_id: to_account_id.into(),
            amount_cents,
        },
    ];
    engine::create_transfer(
        &mut unlocked.conn,
        &from_account_id,
        &date,
        -amount_cents,
        &description,
        &postings,
    )
    .map(|id| id.0)
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn create_transfer_gate_rejects_locked() {
        use crate::crypto::VaultState;
        use std::sync::Mutex;
        let vault = VaultState(Mutex::new(None));
        assert_eq!(
            crate::commands::require_unlocked(&vault).unwrap_err(),
            "locked"
        );
    }
}
