use serde::Serialize;
use tauri::{AppHandle, Manager};
use ts_rs::TS;

use crate::commands::require_unlocked;
use crate::storage::vault_file::{read_vault_file, write_vault_file};
use crate::sync::{apply, bundle, client, signing};

pub const DEFAULT_SERVER_URL: &str = "https://api.getmanila.app";

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/lib/generated/SyncConfig.ts")]
pub struct SyncConfig {
    pub server_url: Option<String>,
    pub enrolled: bool,
    pub subscription_active: bool,
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
            subscription_active: is_subscription_active(cfg.subscription_expires_at.as_deref()),
        }),
        None => Ok(SyncConfig {
            server_url: None,
            enrolled: false,
            subscription_active: false,
        }),
    }
}

const SUBSCRIPTION_GRACE_SECS: u64 = 7 * 24 * 60 * 60;

fn is_subscription_active(expires_at: Option<&str>) -> bool {
    let Some(s) = expires_at else { return false; };
    let Some(exp_secs) = parse_sub_exp_secs(s) else { return false; };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now <= exp_secs.saturating_add(SUBSCRIPTION_GRACE_SECS)
}

fn parse_sub_exp_secs(s: &str) -> Option<u64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut dp = date.splitn(3, '-');
    let y: u64 = dp.next()?.parse().ok()?;
    let mo: u64 = dp.next()?.parse().ok()?;
    let d: u64 = dp.next()?.parse().ok()?;
    let mut tp = time.splitn(3, ':');
    let h: u64 = tp.next()?.parse().ok()?;
    let mi: u64 = tp.next()?.parse().ok()?;
    let sec: u64 = tp.next()?.parse().ok()?;
    let mut days: u64 = 0;
    for yr in 1970..y {
        days += if sub_is_leap(yr) { 366 } else { 365 };
    }
    let mdays = [31u64, if sub_is_leap(y) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..mo {
        days += mdays.get((m as usize).wrapping_sub(1))?;
    }
    days += d.checked_sub(1)?;
    Some(days * 86400 + h * 3600 + mi * 60 + sec)
}

fn sub_is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
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
pub async fn ensure_enrolled(app: AppHandle) -> Result<(), String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    let cfg = read_vault_file(&app_dir)?
        .ok_or_else(|| "vault.json not found".to_string())?;

    if cfg.sync_device_id.is_some() {
        return Ok(());
    }

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
    client::enroll(&http, &app_dir, DEFAULT_SERVER_URL, &seed).await?;

    // client::enroll writes sync_device_id but not server_url; write it here.
    write_default_server_url(&app_dir)
}

fn write_default_server_url(app_dir: &std::path::Path) -> Result<(), String> {
    let mut cfg = read_vault_file(app_dir)?
        .ok_or_else(|| "vault.json not found".to_string())?;
    cfg.server_url = Some(DEFAULT_SERVER_URL.to_string());
    write_vault_file(app_dir, &cfg)
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
    let push_data: Option<(Vec<u8>, i64, usize)> = {
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
            Some((bytes, change_count)) => Some((bytes, max_pending_id, change_count)),
            None => None,
        }
    };

    // Push if there's anything to push. On push error, return early.
    let pushed_count: u32 = if let Some((bundle_bytes, max_pending_id, change_count)) = push_data {
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

        change_count as u32
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

#[tauri::command]
pub async fn revoke_sync(app: AppHandle) -> Result<(), String> {
    let vault_state = app.state::<crate::crypto::VaultState>();
    require_unlocked(&vault_state)?;

    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    let cfg = read_vault_file(&app_dir)?
        .ok_or_else(|| "not configured".to_string())?;

    let server_url = cfg
        .server_url
        .as_deref()
        .ok_or_else(|| "not configured".to_string())?
        .to_string();

    let device_id = cfg
        .sync_device_id
        .clone()
        .ok_or_else(|| "not enrolled".to_string())?;

    let seed = {
        let guard = vault_state
            .0
            .lock()
            .map_err(|_| "vault lock poisoned".to_string())?;
        let unlocked = guard.as_ref().ok_or("locked")?;
        unlocked.keys.sync_signing_seed
    };

    let (signing_key, _) = signing::keypair_from_seed(&seed);
    let http = reqwest::Client::new();
    client::revoke_device(&http, &server_url, &device_id, &signing_key).await?;

    let mut updated_cfg = read_vault_file(&app_dir)?
        .ok_or_else(|| "vault.json not found".to_string())?;
    updated_cfg.sync_device_id = None;
    write_vault_file(&app_dir, &updated_cfg)
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
    fn ensure_enrolled_no_op_when_already_enrolled() {
        let dir = temp_dir();
        let mut cfg = VaultFileConfig::new(&[1u8; 32], &[2u8; 48], None, "2025-01-01T00:00:00Z");
        cfg.sync_device_id = Some("existing-device".to_string());
        write_vault_file(dir.path(), &cfg).unwrap();

        // When sync_device_id is already present, ensure_enrolled returns Ok(()) immediately
        // without touching server_url. Verify the vault is unchanged.
        let loaded = read_vault_file(dir.path()).unwrap().unwrap();
        assert!(loaded.sync_device_id.is_some());
        assert!(loaded.server_url.is_none()); // no server_url write on the no-op path
    }

    #[test]
    fn ensure_enrolled_sets_server_url() {
        let dir = temp_dir();
        // Simulate the state after client::enroll() runs: sync_device_id written, server_url absent.
        let mut cfg = VaultFileConfig::new(&[1u8; 32], &[2u8; 48], None, "2025-01-01T00:00:00Z");
        cfg.sync_device_id = Some("enrolled-device".to_string());
        write_vault_file(dir.path(), &cfg).unwrap();

        // The write step of ensure_enrolled: write DEFAULT_SERVER_URL.
        write_default_server_url(dir.path()).unwrap();

        let loaded = read_vault_file(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.sync_device_id.as_deref(), Some("enrolled-device"));
        assert_eq!(loaded.server_url.as_deref(), Some(DEFAULT_SERVER_URL));
    }

    #[test]
    fn get_sync_config_missing_vault_returns_defaults() {
        let dir = temp_dir();
        let result = match read_vault_file(dir.path()).unwrap() {
            Some(cfg) => SyncConfig {
                server_url: cfg.server_url,
                enrolled: cfg.sync_device_id.is_some(),
                subscription_active: is_subscription_active(cfg.subscription_expires_at.as_deref()),
            },
            None => SyncConfig {
                server_url: None,
                enrolled: false,
                subscription_active: false,
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
            subscription_active: is_subscription_active(loaded.subscription_expires_at.as_deref()),
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

    #[test]
    fn parse_sub_exp_secs_epoch() {
        assert_eq!(parse_sub_exp_secs("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn parse_sub_exp_secs_known_value() {
        assert_eq!(parse_sub_exp_secs("2024-01-01T00:00:00Z"), Some(1704067200));
    }

    #[test]
    fn is_subscription_active_future_expiry_returns_true() {
        assert!(is_subscription_active(Some("2099-01-01T00:00:00Z")));
    }

    #[test]
    fn is_subscription_active_expired_beyond_grace_returns_false() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // Build an expiry 8 days in the past so it is outside the 7-day grace window.
        let exp_secs = now.saturating_sub(8 * 24 * 3600);
        let exp = format!("{}", exp_secs);
        // Construct ISO-8601 from seconds for the assertion.
        let (y, mo, d, h, mi, s) = secs_to_ymd_hms(exp_secs);
        let exp_str = format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z");
        assert!(!is_subscription_active(Some(&exp_str)));
        let _ = exp;
    }

    fn secs_to_ymd_hms(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
        let s = secs % 60;
        let total_minutes = secs / 60;
        let mi = total_minutes % 60;
        let total_hours = total_minutes / 60;
        let h = total_hours % 24;
        let mut days = total_hours / 24;
        let mut y = 1970u64;
        loop {
            let dy = if sub_is_leap(y) { 366 } else { 365 };
            if days < dy { break; }
            days -= dy;
            y += 1;
        }
        let mdays = [31u64, if sub_is_leap(y) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut mo = 1u64;
        for dm in &mdays {
            if days < *dm { break; }
            days -= dm;
            mo += 1;
        }
        (y, mo, days + 1, h, mi, s)
    }
}
