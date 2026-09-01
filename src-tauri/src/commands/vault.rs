use crate::crypto::{
    kdf::derive_master_key,
    keys::derive_keys,
    vault::{decrypt_vault_secret, encrypt_vault_secret},
    VaultKeys, VaultState,
};
use chrono::Utc;
use rand::{rngs::OsRng, RngCore};
use rusqlite::Connection;
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;
use ts_rs::TS;

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/VaultStatus.ts")]
pub struct VaultStatus {
    pub initialized: bool,
    pub unlocked: bool,
}

pub(crate) fn create_vault_inner(
    conn: &Connection,
    keys_mutex: &Mutex<Option<VaultKeys>>,
    password: &str,
) -> Result<(), String> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM vault_config", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if count > 0 {
        return Err("vault already initialized".to_string());
    }

    let mut vault_secret = [0u8; 32];
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut vault_secret);
    OsRng.fill_bytes(&mut salt);

    let master_key = derive_master_key(password.as_bytes(), &salt);
    let blob = encrypt_vault_secret(&master_key, &vault_secret);
    let created_at = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO vault_config (salt, encrypted_vault_secret, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![salt.as_ref(), blob.as_slice(), created_at],
    )
    .map_err(|e| e.to_string())?;

    let derived = derive_keys(&vault_secret);
    let mut guard = keys_mutex
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;
    *guard = Some(VaultKeys {
        data_key: derived.data_key,
        ingest_secret: derived.ingest_secret,
        sync_signing_seed: derived.sync_signing_seed,
    });

    Ok(())
}

pub(crate) fn unlock_vault_inner(
    conn: &Connection,
    keys_mutex: &Mutex<Option<VaultKeys>>,
    password: &str,
) -> Result<(), String> {
    let (salt_blob, encrypted_blob): (Vec<u8>, Vec<u8>) = conn
        .query_row(
            "SELECT salt, encrypted_vault_secret FROM vault_config",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "invalid password".to_string())?;

    let salt: [u8; 32] = salt_blob
        .try_into()
        .map_err(|_| "invalid password".to_string())?;

    let master_key = derive_master_key(password.as_bytes(), &salt);
    let vault_secret = decrypt_vault_secret(&master_key, &encrypted_blob)
        .map_err(|_| "invalid password".to_string())?;

    let derived = derive_keys(&vault_secret);
    let mut guard = keys_mutex
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;
    *guard = Some(VaultKeys {
        data_key: derived.data_key,
        ingest_secret: derived.ingest_secret,
        sync_signing_seed: derived.sync_signing_seed,
    });

    Ok(())
}

#[tauri::command]
pub fn create_vault(
    db: State<'_, Mutex<Connection>>,
    vault_state: State<'_, VaultState>,
    password: String,
) -> Result<(), String> {
    let conn = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    create_vault_inner(&conn, &vault_state.0, &password)
}

#[tauri::command]
pub fn vault_status(
    db: State<'_, Mutex<Connection>>,
    vault_state: State<'_, VaultState>,
) -> Result<VaultStatus, String> {
    let conn = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM vault_config", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let guard = vault_state
        .0
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;

    Ok(VaultStatus {
        initialized: count > 0,
        unlocked: guard.is_some(),
    })
}

#[tauri::command]
pub fn unlock_vault(
    db: State<'_, Mutex<Connection>>,
    vault_state: State<'_, VaultState>,
    password: String,
) -> Result<(), String> {
    let conn = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    unlock_vault_inner(&conn, &vault_state.0, &password)
}

#[tauri::command]
pub fn lock_vault(vault_state: State<'_, VaultState>) -> Result<(), String> {
    let mut guard = vault_state
        .0
        .lock()
        .map_err(|_| "lock poisoned".to_string())?;
    *guard = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{db::open_connection, migrations::run_migrations};

    fn setup() -> Connection {
        let mut conn = open_connection(":memory:").unwrap();
        run_migrations(&mut conn).unwrap();
        conn
    }

    #[test]
    fn create_then_lock_then_unlock_with_correct_password() {
        let conn = setup();
        let state: Mutex<Option<VaultKeys>> = Mutex::new(None);

        create_vault_inner(&conn, &state, "correct-password").unwrap();
        assert!(
            state.lock().unwrap().is_some(),
            "should be unlocked after create"
        );

        *state.lock().unwrap() = None;
        assert!(
            state.lock().unwrap().is_none(),
            "should be locked after clearing"
        );

        unlock_vault_inner(&conn, &state, "correct-password").unwrap();
        assert!(
            state.lock().unwrap().is_some(),
            "should be unlocked after correct unlock"
        );
    }

    #[test]
    fn unlock_with_wrong_password_returns_err() {
        let conn = setup();
        let state: Mutex<Option<VaultKeys>> = Mutex::new(None);

        create_vault_inner(&conn, &state, "correct-password").unwrap();
        *state.lock().unwrap() = None;

        let result = unlock_vault_inner(&conn, &state, "wrong-password");
        assert!(result.is_err(), "unlock with wrong password should fail");
        assert!(
            state.lock().unwrap().is_none(),
            "state should remain locked after failed unlock"
        );
    }

    #[test]
    fn create_vault_rejects_second_call() {
        let conn = setup();
        let state: Mutex<Option<VaultKeys>> = Mutex::new(None);

        create_vault_inner(&conn, &state, "password").unwrap();
        let result = create_vault_inner(&conn, &state, "password2");
        assert!(result.is_err(), "second create_vault should fail");
    }
}
