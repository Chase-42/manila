use crate::crypto::{
    kdf::derive_master_key,
    keys::derive_keys,
    phrase::{decode_phrase, derive_phrase_verifier, encode_phrase},
    vault::{decrypt_vault_secret, encrypt_vault_secret},
    OnboardingState, UnlockedVault, VaultKeys, VaultState,
};
use crate::storage::{
    db::open_connection,
    migrations::run_migrations,
    seed::{seed_categories, seed_category_groups, seed_income_categories},
    vault_file::{read_vault_file, write_vault_file, VaultFileConfig},
};
use chrono::Utc;
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use tauri::{Manager, State};
use ts_rs::TS;
use zeroize::Zeroizing;

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/VaultStatus.ts")]
pub struct VaultStatus {
    pub initialized: bool,
    pub unlocked: bool,
}

fn db_path(app_dir: &Path) -> Result<String, String> {
    app_dir
        .join("manila.db")
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "app data directory path is not valid UTF-8".to_string())
}

fn open_app_db(app_dir: &Path, data_key: &[u8; 32]) -> Result<rusqlite::Connection, String> {
    let path = db_path(app_dir)?;
    let mut conn = open_connection(&path, Some(data_key)).map_err(|e| e.to_string())?;
    run_migrations(&mut conn).map_err(|e| e.to_string())?;
    seed_categories(&conn).map_err(|e| e.to_string())?;
    seed_category_groups(&conn).map_err(|e| e.to_string())?;
    seed_income_categories(&conn).map_err(|e| e.to_string())?;
    Ok(conn)
}

pub(crate) fn create_vault_inner(
    app_dir: &Path,
    vault_mutex: &Mutex<Option<UnlockedVault>>,
    onboarding: &Mutex<Option<Zeroizing<[u8; 32]>>>,
    password: &str,
) -> Result<(), String> {
    if read_vault_file(app_dir)
        .map(|v| v.is_some())
        .unwrap_or(false)
    {
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

    let cfg = VaultFileConfig::new(&salt, &blob, Some(&verifier), &created_at);
    write_vault_file(app_dir, &cfg)?;

    let derived = derive_keys(&vault_secret);
    let conn = open_app_db(app_dir, &derived.data_key)?;

    let mut guard = vault_mutex
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;
    *guard = Some(UnlockedVault {
        keys: VaultKeys {
            data_key: derived.data_key,
            ingest_secret: derived.ingest_secret,
            sync_signing_seed: derived.sync_signing_seed,
        },
        conn,
    });

    let mut ob_guard = onboarding
        .lock()
        .map_err(|_| "onboarding lock poisoned".to_string())?;
    *ob_guard = Some(Zeroizing::new(vault_secret));

    Ok(())
}

pub(crate) fn unlock_vault_inner(
    app_dir: &Path,
    vault_mutex: &Mutex<Option<UnlockedVault>>,
    password: &str,
) -> Result<(), String> {
    let cfg = read_vault_file(app_dir)?.ok_or_else(|| "invalid password".to_string())?;

    let salt_bytes = cfg.salt_bytes()?;
    let salt: [u8; 32] = salt_bytes
        .try_into()
        .map_err(|_| "invalid password".to_string())?;

    let blob = cfg.encrypted_vault_secret_bytes()?;
    let master_key = derive_master_key(password.as_bytes(), &salt);
    let vault_secret =
        decrypt_vault_secret(&master_key, &blob).map_err(|_| "invalid password".to_string())?;

    let derived = derive_keys(&vault_secret);
    let conn = open_app_db(app_dir, &derived.data_key)?;

    let mut guard = vault_mutex
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;
    *guard = Some(UnlockedVault {
        keys: VaultKeys {
            data_key: derived.data_key,
            ingest_secret: derived.ingest_secret,
            sync_signing_seed: derived.sync_signing_seed,
        },
        conn,
    });

    Ok(())
}

pub(crate) fn restore_from_phrase_inner(
    app_dir: &Path,
    vault_mutex: &Mutex<Option<UnlockedVault>>,
    phrase: &str,
    new_password: &str,
) -> Result<(), String> {
    let cfg = read_vault_file(app_dir)?.ok_or_else(|| "vault not initialized".to_string())?;

    let candidate_secret =
        decode_phrase(phrase).map_err(|_| "recovery phrase not recognized".to_string())?;

    let verifier_opt = cfg.phrase_verifier_bytes()?;
    let verifier_bytes =
        verifier_opt.ok_or_else(|| "recovery phrase not recognized".to_string())?;
    let stored: [u8; 32] = verifier_bytes
        .try_into()
        .map_err(|_| "recovery phrase not recognized".to_string())?;
    let candidate_verifier = derive_phrase_verifier(&candidate_secret);
    if candidate_verifier != stored {
        return Err("recovery phrase not recognized".to_string());
    }

    // data_key is derived from the vault secret, which never changes on password reset.
    // Only the master-key-wrapped blob and salt change; no PRAGMA rekey needed.
    let derived = derive_keys(&candidate_secret);
    let conn = open_app_db(app_dir, &derived.data_key)?;

    let mut new_salt = [0u8; 32];
    OsRng.fill_bytes(&mut new_salt);
    let new_master_key = derive_master_key(new_password.as_bytes(), &new_salt);
    let new_blob = encrypt_vault_secret(&new_master_key, &candidate_secret);
    let new_verifier = derive_phrase_verifier(&candidate_secret);
    let new_cfg = VaultFileConfig::new(
        &new_salt,
        &new_blob,
        Some(&new_verifier),
        &Utc::now().to_rfc3339(),
    );
    write_vault_file(app_dir, &new_cfg)?;

    let mut guard = vault_mutex
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;
    *guard = Some(UnlockedVault {
        keys: VaultKeys {
            data_key: derived.data_key,
            ingest_secret: derived.ingest_secret,
            sync_signing_seed: derived.sync_signing_seed,
        },
        conn,
    });

    Ok(())
}

#[tauri::command]
pub fn create_vault(
    app: tauri::AppHandle,
    vault_state: State<'_, VaultState>,
    onboarding: State<'_, OnboardingState>,
    password: String,
) -> Result<(), String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    create_vault_inner(&app_dir, &vault_state.0, &onboarding.0, &password)
}

#[tauri::command]
pub fn vault_status(
    app: tauri::AppHandle,
    vault_state: State<'_, VaultState>,
) -> Result<VaultStatus, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let initialized = read_vault_file(&app_dir)
        .map(|opt| opt.is_some())
        .unwrap_or(false);

    let guard = vault_state
        .0
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;

    Ok(VaultStatus {
        initialized,
        unlocked: guard.is_some(),
    })
}

#[tauri::command]
pub fn unlock_vault(
    app: tauri::AppHandle,
    vault_state: State<'_, VaultState>,
    password: String,
) -> Result<(), String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    unlock_vault_inner(&app_dir, &vault_state.0, &password)
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
    app: tauri::AppHandle,
    vault_state: State<'_, VaultState>,
    phrase: String,
    new_password: String,
) -> Result<(), String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    restore_from_phrase_inner(&app_dir, &vault_state.0, &phrase, &new_password)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Mutex<Option<UnlockedVault>>) {
        let dir = tempfile::tempdir().expect("temp dir");
        let vault_mutex = Mutex::new(None);
        (dir, vault_mutex)
    }

    fn fresh_onboarding() -> Mutex<Option<Zeroizing<[u8; 32]>>> {
        Mutex::new(None)
    }

    #[test]
    fn create_then_lock_then_unlock_with_correct_password() {
        let (dir, vault_mutex) = setup();
        let ob = fresh_onboarding();

        create_vault_inner(dir.path(), &vault_mutex, &ob, "correct-password").unwrap();
        assert!(
            vault_mutex.lock().unwrap().is_some(),
            "unlocked after create"
        );
        assert!(ob.lock().unwrap().is_some(), "onboarding secret populated");

        *vault_mutex.lock().unwrap() = None;
        unlock_vault_inner(dir.path(), &vault_mutex, "correct-password").unwrap();
        assert!(
            vault_mutex.lock().unwrap().is_some(),
            "unlocked after correct unlock"
        );
    }

    #[test]
    fn unlock_with_wrong_password_returns_err() {
        let (dir, vault_mutex) = setup();
        let ob = fresh_onboarding();

        create_vault_inner(dir.path(), &vault_mutex, &ob, "correct-password").unwrap();
        *vault_mutex.lock().unwrap() = None;

        let result = unlock_vault_inner(dir.path(), &vault_mutex, "wrong-password");
        assert!(result.is_err());
        assert!(
            vault_mutex.lock().unwrap().is_none(),
            "still locked after failed unlock"
        );
    }

    #[test]
    fn create_vault_rejects_second_call() {
        let (dir, vault_mutex) = setup();
        let ob = fresh_onboarding();

        create_vault_inner(dir.path(), &vault_mutex, &ob, "password").unwrap();
        let result = create_vault_inner(dir.path(), &vault_mutex, &ob, "password2");
        assert!(result.is_err());
    }

    #[test]
    fn create_vault_stores_phrase_verifier() {
        let (dir, vault_mutex) = setup();
        let ob = fresh_onboarding();

        create_vault_inner(dir.path(), &vault_mutex, &ob, "password").unwrap();

        let cfg = read_vault_file(dir.path()).unwrap().unwrap();
        let verifier = cfg.phrase_verifier_bytes().unwrap();
        assert!(
            verifier.is_some(),
            "phrase_verifier should be stored on create"
        );
        assert_eq!(verifier.unwrap().len(), 32, "verifier should be 32 bytes");
    }

    #[test]
    fn restore_with_correct_phrase_and_new_password() {
        let (dir, vault_mutex) = setup();
        let ob = fresh_onboarding();

        create_vault_inner(dir.path(), &vault_mutex, &ob, "original-password").unwrap();

        let phrase = {
            let guard = ob.lock().unwrap();
            let secret = guard.as_ref().unwrap();
            encode_phrase(secret).join(" ")
        };

        *vault_mutex.lock().unwrap() = None;

        restore_from_phrase_inner(dir.path(), &vault_mutex, &phrase, "new-password").unwrap();
        assert!(
            vault_mutex.lock().unwrap().is_some(),
            "unlocked after restore"
        );

        *vault_mutex.lock().unwrap() = None;
        unlock_vault_inner(dir.path(), &vault_mutex, "new-password").unwrap();
        assert!(
            vault_mutex.lock().unwrap().is_some(),
            "unlocked with new password after restore"
        );
    }

    #[test]
    fn restore_without_verifier_returns_err() {
        let (dir, vault_mutex) = setup();
        let ob = fresh_onboarding();

        create_vault_inner(dir.path(), &vault_mutex, &ob, "password").unwrap();

        // Simulate a legacy vault with no phrase_verifier by writing a config without one.
        let cfg = read_vault_file(dir.path()).unwrap().unwrap();
        let no_verifier_cfg = VaultFileConfig::new(
            &cfg.salt_bytes().unwrap(),
            &cfg.encrypted_vault_secret_bytes().unwrap(),
            None,
            &cfg.created_at,
        );
        write_vault_file(dir.path(), &no_verifier_cfg).unwrap();

        let secret = ob.lock().unwrap();
        let phrase = encode_phrase(secret.as_ref().unwrap()).join(" ");
        drop(secret);

        let result = restore_from_phrase_inner(dir.path(), &vault_mutex, &phrase, "new-password");
        assert!(
            result.is_err(),
            "restore must be rejected when no verifier is stored"
        );
        assert_eq!(result.unwrap_err(), "recovery phrase not recognized");
    }

    #[test]
    fn restore_with_wrong_phrase_returns_err() {
        let (dir, vault_mutex) = setup();
        let ob = fresh_onboarding();

        create_vault_inner(dir.path(), &vault_mutex, &ob, "password").unwrap();

        let wrong_secret = [0xffu8; 32];
        let wrong_phrase = encode_phrase(&wrong_secret).join(" ");

        let result =
            restore_from_phrase_inner(dir.path(), &vault_mutex, &wrong_phrase, "new-password");
        assert!(result.is_err(), "wrong phrase should be rejected");
    }

    #[test]
    fn generate_recovery_phrase_gate_rejects_locked() {
        use crate::crypto::VaultState;
        let vault = VaultState(Mutex::new(None));
        assert_eq!(
            crate::commands::require_unlocked(&vault).unwrap_err(),
            "locked"
        );
    }
}
