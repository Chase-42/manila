use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use rand::rngs::OsRng;
use rand::RngCore;

use crate::crypto::CryptoError;

pub fn encrypt_vault_secret(master_key: &[u8; 32], vault_secret: &[u8; 32]) -> Vec<u8> {
    let cipher = XChaCha20Poly1305::new(master_key.into());
    let mut nonce_bytes = [0u8; 24];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, vault_secret.as_ref())
        .expect("XChaCha20Poly1305 encrypt never fails with a valid key and nonce");
    let mut blob = Vec::with_capacity(24 + ciphertext.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);
    blob
}

pub fn decrypt_vault_secret(master_key: &[u8; 32], blob: &[u8]) -> Result<[u8; 32], CryptoError> {
    if blob.len() < 24 {
        return Err(CryptoError::InvalidBlobLength);
    }
    let (nonce_bytes, ciphertext) = blob.split_at(24);
    let nonce = XNonce::from_slice(nonce_bytes);
    let cipher = XChaCha20Poly1305::new(master_key.into());
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::DecryptionFailed)?;
    if plaintext.len() != 32 {
        return Err(CryptoError::InvalidBlobLength);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&plaintext);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_recovers_original_secret() {
        let master_key = [0x42u8; 32];
        let vault_secret = [0x7fu8; 32];
        let blob = encrypt_vault_secret(&master_key, &vault_secret);
        let recovered = decrypt_vault_secret(&master_key, &blob).unwrap();
        assert_eq!(recovered, vault_secret);
    }

    #[test]
    fn tampered_ciphertext_returns_err() {
        let master_key = [0x11u8; 32];
        let vault_secret = [0xabu8; 32];
        let mut blob = encrypt_vault_secret(&master_key, &vault_secret);
        blob[25] ^= 0xff;
        assert!(decrypt_vault_secret(&master_key, &blob).is_err());
    }
}
