use std::sync::Mutex;
use thiserror::Error;
use zeroize::ZeroizeOnDrop;

pub mod kdf;
pub mod keys;
pub mod vault;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("invalid blob length")]
    InvalidBlobLength,
}

// In-memory keys derived from the vault secret. Never crosses IPC.
// Fields are zeroed on drop via ZeroizeOnDrop.
#[derive(ZeroizeOnDrop)]
pub struct VaultKeys {
    pub data_key: [u8; 32],
    pub ingest_secret: [u8; 32],
    pub sync_signing_seed: [u8; 32],
}

pub struct VaultState(pub Mutex<Option<VaultKeys>>);
