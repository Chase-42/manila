use bip39::Mnemonic;
use hkdf::Hkdf;
use sha2::Sha256;

use crate::crypto::CryptoError;

pub fn encode_phrase(vault_secret: &[u8; 32]) -> Vec<String> {
    // Mnemonic::from_entropy only fails for invalid entropy length; 32 bytes is valid.
    #[allow(clippy::expect_used)]
    let mnemonic = Mnemonic::from_entropy(vault_secret)
        .expect("32-byte entropy is always a valid BIP39 input");
    mnemonic.words().map(str::to_string).collect()
}

pub fn decode_phrase(phrase: &str) -> Result<[u8; 32], CryptoError> {
    let mnemonic: Mnemonic = phrase.parse().map_err(|_| CryptoError::InvalidPhrase)?;
    let entropy = mnemonic.to_entropy();
    entropy.try_into().map_err(|_| CryptoError::InvalidPhrase)
}

// HKDF-derived check value stored alongside the encrypted vault secret.
// Lets restore_from_phrase reject a wrong phrase before re-encrypting.
// Distinct info string guarantees no overlap with data_key/ingest/sync_auth.
// expand only fails when output exceeds 255 * hash_len bytes; 32 bytes is always in range.
#[allow(clippy::expect_used)]
pub fn derive_phrase_verifier(vault_secret: &[u8; 32]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, vault_secret);
    let mut verifier = [0u8; 32];
    hk.expand(b"manila-phrase-verify-v1", &mut verifier)
        .expect("HKDF expand with 32-byte output never fails");
    verifier
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bip39_round_trip() {
        let secret = [0x42u8; 32];
        let words = encode_phrase(&secret);
        assert_eq!(words.len(), 24);
        let phrase = words.join(" ");
        let recovered = decode_phrase(&phrase).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn verifier_is_deterministic() {
        let secret = [0x11u8; 32];
        let a = derive_phrase_verifier(&secret);
        let b = derive_phrase_verifier(&secret);
        assert_eq!(a, b);
    }

    #[test]
    fn verifier_differs_from_derived_keys() {
        use crate::crypto::keys::derive_keys;
        let secret = [0x55u8; 32];
        let keys = derive_keys(&secret);
        let verifier = derive_phrase_verifier(&secret);
        assert_ne!(verifier, keys.data_key);
        assert_ne!(verifier, keys.ingest_secret);
        assert_ne!(verifier, keys.sync_signing_seed);
    }

    #[test]
    fn wrong_phrase_returns_err() {
        let result = decode_phrase("not a valid phrase at all foo bar baz");
        assert!(result.is_err());
    }
}
