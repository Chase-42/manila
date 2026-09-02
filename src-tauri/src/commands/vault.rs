use crate::crypto::{
    kdf::derive_master_key,
    keys::derive_keys,
    phrase::{decode_phrase, derive_phrase_verifier, encode_phrase},
    vault::{decrypt_vault_secret, encrypt_vault_secret},
    OnboardingState, VaultKeys, VaultState,
};
use chrono::Utc;
use rand::{rngs::OsRng, RngCore};
use rusqlite::Connection;
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;
use ts_rs::TS;
use zeroize::Zeroizing;

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/VaultStatus.ts")]
pub struct VaultStatus {
    pub initialized: bool,
    pub unlocked: bool,
}

pub(crate) fn create_vault_inner(
    conn: &Connection,
    keys_mutex: &Mutex<Option<VaultKeys>>,
    onboarding: &Mutex<Option<Zeroizing<[u8; 32]>>>,
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
    let verifier = derive_phrase_verifier(&vault_secret);
    let created_at = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO vault_config (salt, encrypted_vault_secret, phrase_verifier, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![salt.as_ref(), blob.as_slice(), verifier.as_ref(), created_at],
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

    // Store vault_secret for the upcoming phrase ceremony.
    let mut ob_guard = onboarding
        .lock()
        .map_err(|_| "onboarding lock poisoned".to_string())?;
    *ob_guard = Some(Zeroizing::new(vault_secret));

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

pub(crate) fn restore_from_phrase_inner(
    conn: &Connection,
    keys_mutex: &Mutex<Option<VaultKeys>>,
    phrase: &str,
    new_password: &str,
) -> Result<(), String> {
    let candidate_secret =
        decode_phrase(phrase).map_err(|_| "recovery phrase not recognized".to_string())?;

    // Verify against stored check value if one exists (legacy vaults have NULL).
    let stored_verifier: Option<Vec<u8>> = conn
        .query_row("SELECT phrase_verifier FROM vault_config", [], |row| {
            row.get(0)
        })
        .map_err(|_| "vault not initialized".to_string())?;

    // A NULL verifier means no phrase ceremony was completed; no valid phrase exists.
    let verifier_bytes =
        stored_verifier.ok_or_else(|| "recovery phrase not recognized".to_string())?;
    let stored: [u8; 32] = verifier_bytes
        .try_into()
        .map_err(|_| "recovery phrase not recognized".to_string())?;
    let candidate_verifier = derive_phrase_verifier(&candidate_secret);
    if candidate_verifier != stored {
        return Err("recovery phrase not recognized".to_string());
    }

    let mut new_salt = [0u8; 32];
    OsRng.fill_bytes(&mut new_salt);
    let new_master_key = derive_master_key(new_password.as_bytes(), &new_salt);
    let new_blob = encrypt_vault_secret(&new_master_key, &candidate_secret);
    let new_verifier = derive_phrase_verifier(&candidate_secret);

    conn.execute(
        "UPDATE vault_config SET salt = ?1, encrypted_vault_secret = ?2, phrase_verifier = ?3",
        rusqlite::params![
            new_salt.as_ref(),
            new_blob.as_slice(),
            new_verifier.as_ref()
        ],
    )
    .map_err(|e| e.to_string())?;

    let derived = derive_keys(&candidate_secret);
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
    onboarding: State<'_, OnboardingState>,
    password: String,
) -> Result<(), String> {
    let conn = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    create_vault_inner(&conn, &vault_state.0, &onboarding.0, &password)
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

#[tauri::command]
pub fn generate_recovery_phrase(
    vault: State<'_, VaultState>,
    onboarding: State<'_, OnboardingState>,
) -> Result<Vec<String>, String> {
    crate::commands::require_unlocked(&vault)?;

    let guard = onboarding
        .0
        .lock()
        .map_err(|_| "onboarding lock poisoned".to_string())?;
    let secret = guard.as_ref().ok_or(
        "no pending recovery phrase - ceremony already completed or vault was not freshly created"
            .to_string(),
    )?;

    Ok(encode_phrase(secret))
}

#[tauri::command]
pub fn acknowledge_recovery_phrase(onboarding: State<'_, OnboardingState>) -> Result<(), String> {
    let mut guard = onboarding
        .0
        .lock()
        .map_err(|_| "onboarding lock poisoned".to_string())?;
    *guard = None;
    Ok(())
}

#[tauri::command]
pub fn restore_from_phrase(
    db: State<'_, Mutex<Connection>>,
    vault_state: State<'_, VaultState>,
    phrase: String,
    new_password: String,
) -> Result<(), String> {
    let conn = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    restore_from_phrase_inner(&conn, &vault_state.0, &phrase, &new_password)
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

    fn locked_keys() -> Mutex<Option<VaultKeys>> {
        Mutex::new(None)
    }

    fn fresh_onboarding() -> Mutex<Option<Zeroizing<[u8; 32]>>> {
        Mutex::new(None)
    }

    #[test]
    fn create_then_lock_then_unlock_with_correct_password() {
        let conn = setup();
        let keys = locked_keys();
        let ob = fresh_onboarding();

        create_vault_inner(&conn, &keys, &ob, "correct-password").unwrap();
        assert!(keys.lock().unwrap().is_some(), "unlocked after create");
        assert!(ob.lock().unwrap().is_some(), "onboarding secret populated");

        *keys.lock().unwrap() = None;
        unlock_vault_inner(&conn, &keys, "correct-password").unwrap();
        assert!(
            keys.lock().unwrap().is_some(),
            "unlocked after correct unlock"
        );
    }

    #[test]
    fn unlock_with_wrong_password_returns_err() {
        let conn = setup();
        let keys = locked_keys();
        let ob = fresh_onboarding();

        create_vault_inner(&conn, &keys, &ob, "correct-password").unwrap();
        *keys.lock().unwrap() = None;

        let result = unlock_vault_inner(&conn, &keys, "wrong-password");
        assert!(result.is_err());
        assert!(
            keys.lock().unwrap().is_none(),
            "still locked after failed unlock"
        );
    }

    #[test]
    fn create_vault_rejects_second_call() {
        let conn = setup();
        let keys = locked_keys();
        let ob = fresh_onboarding();

        create_vault_inner(&conn, &keys, &ob, "password").unwrap();
        let result = create_vault_inner(&conn, &keys, &ob, "password2");
        assert!(result.is_err());
    }

    #[test]
    fn create_vault_stores_phrase_verifier() {
        let conn = setup();
        let keys = locked_keys();
        let ob = fresh_onboarding();

        create_vault_inner(&conn, &keys, &ob, "password").unwrap();

        let verifier: Option<Vec<u8>> = conn
            .query_row("SELECT phrase_verifier FROM vault_config", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(
            verifier.is_some(),
            "phrase_verifier should be stored on create"
        );
        assert_eq!(verifier.unwrap().len(), 32, "verifier should be 32 bytes");
    }

    #[test]
    fn restore_with_correct_phrase_and_new_password() {
        let conn = setup();
        let keys = locked_keys();
        let ob = fresh_onboarding();

        create_vault_inner(&conn, &keys, &ob, "original-password").unwrap();

        let phrase = {
            let guard = ob.lock().unwrap();
            let secret = guard.as_ref().unwrap();
            encode_phrase(secret).join(" ")
        };

        *keys.lock().unwrap() = None;

        restore_from_phrase_inner(&conn, &keys, &phrase, "new-password").unwrap();
        assert!(keys.lock().unwrap().is_some(), "unlocked after restore");

        *keys.lock().unwrap() = None;
        unlock_vault_inner(&conn, &keys, "new-password").unwrap();
        assert!(
            keys.lock().unwrap().is_some(),
            "unlocked with new password after restore"
        );
    }

    #[test]
    fn restore_without_verifier_returns_err() {
        let conn = setup();
        let keys = locked_keys();
        let ob = fresh_onboarding();

        create_vault_inner(&conn, &keys, &ob, "password").unwrap();

        // Simulate a legacy vault created before migration 013.
        conn.execute("UPDATE vault_config SET phrase_verifier = NULL", [])
            .unwrap();

        let secret = ob.lock().unwrap();
        let phrase = encode_phrase(secret.as_ref().unwrap()).join(" ");
        drop(secret);

        let result = restore_from_phrase_inner(&conn, &keys, &phrase, "new-password");
        assert!(
            result.is_err(),
            "restore must be rejected when no verifier is stored"
        );
        assert_eq!(result.unwrap_err(), "recovery phrase not recognized");
    }

    #[test]
    fn restore_with_wrong_phrase_returns_err() {
        let conn = setup();
        let keys = locked_keys();
        let ob = fresh_onboarding();

        create_vault_inner(&conn, &keys, &ob, "password").unwrap();

        let wrong_secret = [0xffu8; 32];
        let wrong_phrase = encode_phrase(&wrong_secret).join(" ");

        let result = restore_from_phrase_inner(&conn, &keys, &wrong_phrase, "new-password");
        assert!(result.is_err(), "wrong phrase should be rejected");
    }
}
