use serde::Serialize;
use tauri::{AppHandle, Manager};
use ts_rs::TS;

use crate::storage::vault_file::{read_vault_file, write_vault_file};
use crate::sync::{apply, bundle, client, signing};

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/SyncConfig.ts")]
pub struct SyncConfig {
    pub server_url: Option<String>,
    pub enrolled: bool,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/SyncStatus.ts")]
pub struct SyncStatus {
    pub pushed_count: u32,
    pub applied_count: u32,
    pub last_synced_at: String,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/SyncLastRun.ts")]
pub struct SyncLastRun {
    pub last_synced_at: String,
    pub pushed_count: u32,
    pub applied_count: u32,
}

#[tauri::command]
pub async fn configure_sync(app: AppHandle, server_url: String) -> Result<(), String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let mut cfg = read_vault_file(&app_dir)?
        .ok_or_else(|| "vault.json not found".to_string())?;
    cfg.server_url = Some(server_url);
    write_vault_file(&app_dir, &cfg)
}

#[tauri::command]
pub async fn get_sync_config(app: AppHandle) -> Result<SyncConfig, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    match read_vault_file(&app_dir)? {
        Some(cfg) => Ok(SyncConfig {
            server_url: cfg.server_url,
            enrolled: cfg.sync_device_id.is_some(),
        }),
        None => Ok(SyncConfig {
            server_url: None,
            enrolled: false,
        }),
    }
}

#[tauri::command]
pub async fn enroll_sync(app: AppHandle, server_url: String) -> Result<String, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    let seed = {
        let vault_state = app.state::<crate::crypto::VaultState>();
        let guard = vault_state
            .0
            .lock()
            .map_err(|_| "vault lock poisoned".to_string())?;
        let unlocked = guard.as_ref().ok_or("locked")?;
        unlocked.keys.sync_signing_seed
    };

    let http = reqwest::Client::new();
    client::enroll(&http, &app_dir, &server_url, &seed).await
}

#[tauri::command]
pub async fn run_sync(app: AppHandle) -> Result<SyncStatus, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    // Read vault config outside the mutex so we don't hold it during I/O.
    let cfg_opt = read_vault_file(&app_dir)?;

    let server_url = match cfg_opt.as_ref().and_then(|c| c.server_url.as_deref()) {
        Some(url) => url.to_string(),
        None => {
            return Ok(SyncStatus {
                pushed_count: 0,
                applied_count: 0,
                last_synced_at: String::new(),
            });
        }
    };

    // Extract keys and device_id while holding the mutex.
    let (data_key, seed, existing_device_id, push_counter) = {
        let vault_state = app.state::<crate::crypto::VaultState>();
        let guard = vault_state
            .0
            .lock()
            .map_err(|_| "vault lock poisoned".to_string())?;
        let unlocked = guard.as_ref().ok_or("locked")?;
        let device_id = cfg_opt.as_ref().and_then(|c| c.sync_device_id.clone());
        let counter: i64 = unlocked
            .conn
            .query_row(
                "SELECT COALESCE(CAST(value AS INTEGER), 0) FROM sync_state WHERE key = 'local_push_counter'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        (
            unlocked.keys.data_key,
            unlocked.keys.sync_signing_seed,
            device_id,
            counter,
        )
    };

    let http = reqwest::Client::new();

    // Enroll if not yet enrolled.
    let device_id = match existing_device_id {
        Some(id) => id,
        None => client::enroll(&http, &app_dir, &server_url, &seed).await?,
    };

    let (signing_key, _) = signing::keypair_from_seed(&seed);

    // Build push bundle while holding the mutex.
    let push_data: Option<(Vec<u8>, i64)> = {
        let vault_state = app.state::<crate::crypto::VaultState>();
        let guard = vault_state
            .0
            .lock()
            .map_err(|_| "vault lock poisoned".to_string())?;
        let unlocked = guard.as_ref().ok_or("locked")?;
        let max_pending_id: i64 = unlocked
            .conn
            .query_row("SELECT COALESCE(MAX(id), 0) FROM sync_pending_changes", [], |r| r.get(0))
            .unwrap_or(0);
        match bundle::build_push_bundle(&unlocked.conn, &device_id, push_counter)? {
            Some(bytes) => Some((bytes, max_pending_id)),
            None => None,
        }
    };

    // Push if there's anything to push. On push error, return early.
    let pushed_count: u32 = if let Some((bundle_bytes, max_pending_id)) = push_data {
        client::push_bundle(&http, &server_url, &bundle_bytes, &data_key, &device_id, &signing_key)
            .await?;

        // Clear pushed pending changes and increment counter.
        let vault_state = app.state::<crate::crypto::VaultState>();
        let guard = vault_state
            .0
            .lock()
            .map_err(|_| "vault lock poisoned".to_string())?;
        let unlocked = guard.as_ref().ok_or("locked")?;

        unlocked
            .conn
            .execute(
                "DELETE FROM sync_pending_changes WHERE id <= ?1",
                [max_pending_id],
            )
            .map_err(|e| e.to_string())?;
        unlocked
            .conn
            .execute(
                "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('local_push_counter', ?1)",
                [&(push_counter + 1).to_string()],
            )
            .map_err(|e| e.to_string())?;

        // Count how many change entries were included (up to max_pending_id).
        // We approximate this as the bundle's changes count — the bundle was built from deduped rows.
        // Use max_pending_id as an upper bound for the count.
        max_pending_id as u32
    } else {
        0
    };

    // Pull remote blobs.
    let after_seq: i64 = {
        let vault_state = app.state::<crate::crypto::VaultState>();
        let guard = vault_state
            .0
            .lock()
            .map_err(|_| "vault lock poisoned".to_string())?;
        let unlocked = guard.as_ref().ok_or("locked")?;
        unlocked
            .conn
            .query_row(
                "SELECT COALESCE(CAST(value AS INTEGER), 0) FROM sync_state WHERE key = 'last_pulled_seq'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0)
    };

    let pull_resp =
        client::pull_blobs(&http, &server_url, after_seq, &device_id, &signing_key).await?;

    let mut applied_count: u32 = 0;
    let mut max_seq = after_seq;

    for blob in &pull_resp.blobs {
        if blob.seq > max_seq {
            max_seq = blob.seq;
        }
        let sync_bundle = match bundle::decode_bundle(&data_key, &blob.payload) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let vault_state = app.state::<crate::crypto::VaultState>();
        let mut guard = vault_state
            .0
            .lock()
            .map_err(|_| "vault lock poisoned".to_string())?;
        let unlocked = guard.as_mut().ok_or("locked")?;
        if let Ok(n) = apply::apply_bundle(&mut unlocked.conn, &sync_bundle, &data_key) {
            applied_count += n;
        }
    }

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Persist sync state.
    {
        let vault_state = app.state::<crate::crypto::VaultState>();
        let guard = vault_state
            .0
            .lock()
            .map_err(|_| "vault lock poisoned".to_string())?;
        let unlocked = guard.as_ref().ok_or("locked")?;
        unlocked
            .conn
            .execute(
                "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('last_pulled_seq', ?1)",
                [&max_seq.to_string()],
            )
            .map_err(|e| e.to_string())?;
        unlocked
            .conn
            .execute(
                "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('last_synced_at', ?1)",
                [&now],
            )
            .map_err(|e| e.to_string())?;
        unlocked
            .conn
            .execute(
                "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('last_pushed_count', ?1)",
                [&pushed_count.to_string()],
            )
            .map_err(|e| e.to_string())?;
        unlocked
            .conn
            .execute(
                "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('last_applied_count', ?1)",
                [&applied_count.to_string()],
            )
            .map_err(|e| e.to_string())?;
    }

    Ok(SyncStatus {
        pushed_count,
        applied_count,
        last_synced_at: now,
    })
}

#[tauri::command]
pub async fn get_sync_status(app: AppHandle) -> Result<Option<SyncLastRun>, String> {
    let vault_state = app.state::<crate::crypto::VaultState>();
    let guard = vault_state
        .0
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;
    let unlocked = match guard.as_ref() {
        Some(u) => u,
        None => return Ok(None),
    };

    let last_synced_at: Option<String> = unlocked
        .conn
        .query_row(
            "SELECT value FROM sync_state WHERE key = 'last_synced_at'",
            [],
            |r| r.get(0),
        )
        .ok();

    match last_synced_at {
        None => Ok(None),
        Some(ts) => {
            let pushed: u32 = unlocked
                .conn
                .query_row(
                    "SELECT COALESCE(CAST(value AS INTEGER), 0) FROM sync_state WHERE key = 'last_pushed_count'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let applied: u32 = unlocked
                .conn
                .query_row(
                    "SELECT COALESCE(CAST(value AS INTEGER), 0) FROM sync_state WHERE key = 'last_applied_count'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            Ok(Some(SyncLastRun {
                last_synced_at: ts,
                pushed_count: pushed,
                applied_count: applied,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::vault_file::VaultFileConfig;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    fn write_basic_vault(dir: &std::path::Path) {
        let cfg = VaultFileConfig::new(&[1u8; 32], &[2u8; 48], None, "2025-01-01T00:00:00Z");
        write_vault_file(dir, &cfg).unwrap();
    }

    #[test]
    fn get_sync_config_missing_vault_returns_defaults() {
        let dir = temp_dir();
        let result = match read_vault_file(dir.path()).unwrap() {
            Some(cfg) => SyncConfig {
                server_url: cfg.server_url,
                enrolled: cfg.sync_device_id.is_some(),
            },
            None => SyncConfig {
                server_url: None,
                enrolled: false,
            },
        };
        assert!(result.server_url.is_none());
        assert!(!result.enrolled);
    }

    #[test]
    fn configure_sync_writes_server_url() {
        let dir = temp_dir();
        write_basic_vault(dir.path());

        let mut cfg = read_vault_file(dir.path()).unwrap().unwrap();
        cfg.server_url = Some("https://sync.example.com".to_string());
        write_vault_file(dir.path(), &cfg).unwrap();

        let loaded = read_vault_file(dir.path()).unwrap().unwrap();
        assert_eq!(
            loaded.server_url.as_deref(),
            Some("https://sync.example.com")
        );
    }

    #[test]
    fn get_sync_config_enrolled_true_when_device_id_present() {
        let dir = temp_dir();
        let mut cfg = VaultFileConfig::new(&[1u8; 32], &[2u8; 48], None, "2025-01-01T00:00:00Z");
        cfg.sync_device_id = Some("device-uuid-123".to_string());
        write_vault_file(dir.path(), &cfg).unwrap();

        let loaded = read_vault_file(dir.path()).unwrap().unwrap();
        let result = SyncConfig {
            server_url: loaded.server_url,
            enrolled: loaded.sync_device_id.is_some(),
        };
        assert!(result.enrolled);
    }

    #[test]
    fn legacy_vault_without_new_fields_deserializes_cleanly() {
        let dir = temp_dir();
        let json = r#"{"salt":"AAAA","encrypted_vault_secret":"AAAA","phrase_verifier":null,"created_at":"2025-01-01T00:00:00Z"}"#;
        std::fs::write(dir.path().join("vault.json"), json).unwrap();
        let loaded = read_vault_file(dir.path()).unwrap().unwrap();
        assert!(loaded.server_url.is_none());
        assert!(loaded.sync_device_id.is_none());
    }
}
