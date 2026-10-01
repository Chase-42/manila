use std::sync::Mutex;
use thiserror::Error;
use zeroize::{ZeroizeOnDrop, Zeroizing};

pub mod kdf;
pub mod keys;
pub mod phrase;
pub mod vault;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("invalid blob length")]
    InvalidBlobLength,
    #[error("invalid recovery phrase")]
    InvalidPhrase,
}

// In-memory keys derived from the vault secret. Never crosses IPC.
// Fields are zeroed on drop via ZeroizeOnDrop.
#[derive(ZeroizeOnDrop)]
pub struct VaultKeys {
    pub data_key: [u8; 32],
    pub ingest_secret: [u8; 32],
    pub sync_signing_seed: [u8; 32],
}

// Bundled vault keys + live database connection. Exists only while unlocked.
// Dropping this clears the keys (via ZeroizeOnDrop) and closes the DB connection.
pub struct UnlockedVault {
    // keys is retained for 14b field-level encryption; unused until then.
    #[allow(dead_code)]
    pub keys: VaultKeys,
    pub conn: rusqlite::Connection,
}

pub struct VaultState(pub Mutex<Option<UnlockedVault>>);

// Holds the vault secret transiently during the recovery phrase ceremony.
// Set by create_vault, cleared by acknowledge_recovery_phrase.
// Zeroizing ensures the bytes are wiped on drop.
pub struct OnboardingState(pub Mutex<Option<Zeroizing<[u8; 32]>>>);
