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

pub struct VaultState(pub Mutex<Option<VaultKeys>>);

// Holds the vault secret transiently during the recovery phrase ceremony.
// Set by create_vault, cleared by acknowledge_recovery_phrase.
// Zeroizing ensures the bytes are wiped on drop.
pub struct OnboardingState(pub Mutex<Option<Zeroizing<[u8; 32]>>>);
