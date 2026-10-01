use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
pub struct VaultFileConfig {
    pub salt: String,
    pub encrypted_vault_secret: String,
    pub phrase_verifier: Option<String>,
    pub created_at: String,
}

impl VaultFileConfig {
    pub fn new(
        salt: &[u8],
        encrypted_vault_secret: &[u8],
        phrase_verifier: Option<&[u8]>,
        created_at: &str,
    ) -> Self {
        Self {
            salt: B64.encode(salt),
            encrypted_vault_secret: B64.encode(encrypted_vault_secret),
            phrase_verifier: phrase_verifier.map(|v| B64.encode(v)),
            created_at: created_at.to_string(),
        }
    }

    pub fn salt_bytes(&self) -> Result<Vec<u8>, String> {
        B64.decode(&self.salt).map_err(|e| e.to_string())
    }

    pub fn encrypted_vault_secret_bytes(&self) -> Result<Vec<u8>, String> {
        B64.decode(&self.encrypted_vault_secret)
            .map_err(|e| e.to_string())
    }

    pub fn phrase_verifier_bytes(&self) -> Result<Option<Vec<u8>>, String> {
        self.phrase_verifier
            .as_deref()
            .map(|s| B64.decode(s).map_err(|e| e.to_string()))
            .transpose()
    }
}

pub fn read_vault_file(app_dir: &Path) -> Result<Option<VaultFileConfig>, String> {
    let path = app_dir.join("vault.json");
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let cfg: VaultFileConfig =
        serde_json::from_str(&content).map_err(|e| format!("vault.json is corrupted: {e}"))?;
    Ok(Some(cfg))
}

pub fn write_vault_file(app_dir: &Path, cfg: &VaultFileConfig) -> Result<(), String> {
    let path = app_dir.join("vault.json");
    let content = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    #[test]
    fn round_trip_with_verifier() {
        let dir = temp_dir();
        let salt = [1u8; 32];
        let blob = [2u8; 48];
        let verifier = [3u8; 32];
        let cfg = VaultFileConfig::new(&salt, &blob, Some(&verifier), "2025-01-01T00:00:00Z");
        write_vault_file(dir.path(), &cfg).unwrap();

        let loaded = read_vault_file(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.salt_bytes().unwrap(), salt);
        assert_eq!(loaded.encrypted_vault_secret_bytes().unwrap(), blob);
        assert_eq!(
            loaded.phrase_verifier_bytes().unwrap(),
            Some(verifier.to_vec())
        );
    }

    #[test]
    fn round_trip_without_verifier() {
        let dir = temp_dir();
        let cfg = VaultFileConfig::new(&[0u8; 32], &[0u8; 48], None, "2025-01-01T00:00:00Z");
        write_vault_file(dir.path(), &cfg).unwrap();

        let loaded = read_vault_file(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.phrase_verifier_bytes().unwrap(), None);
    }

    #[test]
    fn missing_file_returns_none() {
        let dir = temp_dir();
        assert!(read_vault_file(dir.path()).unwrap().is_none());
    }

    #[test]
    fn corrupted_file_returns_err() {
        let dir = temp_dir();
        fs::write(dir.path().join("vault.json"), "not json").unwrap();
        assert!(read_vault_file(dir.path()).is_err());
    }
}
