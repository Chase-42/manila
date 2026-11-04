use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use rand::rngs::OsRng;
use rand::RngCore;

use crate::crypto::CryptoError;

const PREFIX: &str = "v1:";

// encrypt only fails on wrong key/nonce sizes, both type-guaranteed at [u8; 32]/[u8; 24].
#[allow(clippy::expect_used)]
pub fn seal_field(key: &[u8; 32], plaintext: &str) -> String {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; 24];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .expect("XChaCha20Poly1305 encrypt never fails with a valid key and nonce");
    let mut blob = Vec::with_capacity(24 + ciphertext.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);
    format!("{PREFIX}{}", B64.encode(blob))
}

pub fn open_field(key: &[u8; 32], sealed: &str) -> Result<String, CryptoError> {
    let Some(encoded) = sealed.strip_prefix(PREFIX) else {
        // Legacy plaintext row: return as-is.
        return Ok(sealed.to_string());
    };
    let blob = B64
        .decode(encoded)
        .map_err(|_| CryptoError::DecryptionFailed)?;
    if blob.len() < 24 {
        return Err(CryptoError::InvalidBlobLength);
    }
    let (nonce_bytes, ciphertext) = blob.split_at(24);
    let nonce = XNonce::from_slice(nonce_bytes);
    let cipher = XChaCha20Poly1305::new(key.into());
    let plaintext_bytes = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::DecryptionFailed)?;
    String::from_utf8(plaintext_bytes).map_err(|_| CryptoError::DecryptionFailed)
}

// Encrypts arbitrary bytes; layout is identical to seal_field: [24-byte nonce | ciphertext].
// No v1: prefix and no base64 — callers encode the result themselves.
#[allow(clippy::expect_used)]
pub fn seal_bytes(key: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; 24];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .expect("XChaCha20Poly1305 encrypt never fails with a valid key and nonce");
    let mut blob = Vec::with_capacity(24 + ciphertext.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);
    blob
}

pub fn open_bytes(key: &[u8; 32], blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < 24 {
        return Err(CryptoError::InvalidBlobLength);
    }
    let (nonce_bytes, ciphertext) = blob.split_at(24);
    let nonce = XNonce::from_slice(nonce_bytes);
    let cipher = XChaCha20Poly1305::new(key.into());
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::DecryptionFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_recovers_plaintext() {
        let key = [0x42u8; 32];
        let sealed = seal_field(&key, "Whole Foods Market");
        let recovered = open_field(&key, &sealed).unwrap();
        assert_eq!(recovered, "Whole Foods Market");
    }

    #[test]
    fn wrong_key_returns_decryption_failed() {
        let key = [0x11u8; 32];
        let sealed = seal_field(&key, "Coffee Shop");
        let wrong_key = [0x22u8; 32];
        let result = open_field(&wrong_key, &sealed);
        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn legacy_passthrough_returns_string_as_is() {
        let key = [0x99u8; 32];
        let result = open_field(&key, "Starbucks").unwrap();
        assert_eq!(result, "Starbucks");
    }

    #[test]
    fn seal_produces_v1_prefix() {
        let key = [0x55u8; 32];
        let sealed = seal_field(&key, "Amazon");
        assert!(
            sealed.starts_with("v1:"),
            "sealed value must start with v1:"
        );
    }

    #[test]
    fn two_seals_of_same_plaintext_produce_different_ciphertext() {
        let key = [0x77u8; 32];
        let a = seal_field(&key, "Trader Joe's");
        let b = seal_field(&key, "Trader Joe's");
        assert_ne!(a, b, "each seal uses a fresh nonce");
    }

    #[test]
    fn empty_string_round_trips() {
        let key = [0xabu8; 32];
        let sealed = seal_field(&key, "");
        let recovered = open_field(&key, &sealed).unwrap();
        assert_eq!(recovered, "");
    }

    #[test]
    fn seal_bytes_round_trip() {
        let key = [0xabu8; 32];
        let plaintext = b"hello sync world";
        let blob = seal_bytes(&key, plaintext);
        let recovered = open_bytes(&key, &blob).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn seal_bytes_empty_round_trips() {
        let key = [0x01u8; 32];
        let blob = seal_bytes(&key, b"");
        let recovered = open_bytes(&key, &blob).unwrap();
        assert_eq!(recovered, b"");
    }

    #[test]
    fn open_bytes_tampered_returns_decryption_failed() {
        let key = [0x33u8; 32];
        let mut blob = seal_bytes(&key, b"sensitive data");
        blob[30] ^= 0xff;
        let result = open_bytes(&key, &blob);
        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn open_bytes_too_short_returns_invalid_blob_length() {
        let key = [0x44u8; 32];
        let short = vec![0u8; 23];
        let result = open_bytes(&key, &short);
        assert!(matches!(result, Err(CryptoError::InvalidBlobLength)));
    }

    #[test]
    fn tampered_ciphertext_returns_decryption_failed() {
        let key = [0x33u8; 32];
        let mut sealed = seal_field(&key, "Target");
        // Flip a byte in the base64 payload after the prefix.
        let flip_pos = PREFIX.len() + 10;
        let mut chars: Vec<char> = sealed.chars().collect();
        if chars[flip_pos] == 'A' {
            chars[flip_pos] = 'B';
        } else {
            chars[flip_pos] = 'A';
        }
        sealed = chars.into_iter().collect();
        let result = open_field(&key, &sealed);
        assert!(matches!(
            result,
            Err(CryptoError::DecryptionFailed | CryptoError::InvalidBlobLength)
        ));
    }
}
