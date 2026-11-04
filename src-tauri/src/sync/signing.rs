use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

pub fn keypair_from_seed(seed: &[u8; 32]) -> (SigningKey, VerifyingKey) {
    let signing_key = SigningKey::from_bytes(seed);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

pub fn public_key_hex(vk: &VerifyingKey) -> String {
    vk.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sign_request(
    signing_key: &SigningKey,
    method: &str,
    path_and_query: &str,
    timestamp_secs: i64,
) -> String {
    let message = format!("{method}\n{path_and_query}\n{timestamp_secs}");
    let sig: Signature = signing_key.sign(message.as_bytes());
    sig.to_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Verifier;

    #[test]
    fn keypair_from_seed_is_deterministic() {
        let seed = [0x42u8; 32];
        let (sk1, vk1) = keypair_from_seed(&seed);
        let (sk2, vk2) = keypair_from_seed(&seed);
        assert_eq!(sk1.to_bytes(), sk2.to_bytes());
        assert_eq!(vk1.as_bytes(), vk2.as_bytes());
    }

    #[test]
    fn different_seeds_produce_different_keypairs() {
        let seed_a = [0x01u8; 32];
        let seed_b = [0x02u8; 32];
        let (_, vk_a) = keypair_from_seed(&seed_a);
        let (_, vk_b) = keypair_from_seed(&seed_b);
        assert_ne!(vk_a.as_bytes(), vk_b.as_bytes());
    }

    #[test]
    fn public_key_hex_returns_64_lowercase_hex_chars() {
        let seed = [0x55u8; 32];
        let (_, vk) = keypair_from_seed(&seed);
        let hex = public_key_hex(&vk);
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn sign_request_returns_128_lowercase_hex_chars() {
        let seed = [0x77u8; 32];
        let (sk, _) = keypair_from_seed(&seed);
        let sig_hex = sign_request(&sk, "POST", "/sync/push", 1_700_000_000);
        assert_eq!(sig_hex.len(), 128);
        assert!(sig_hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn sign_request_verifies_with_verifying_key() {
        let seed = [0x99u8; 32];
        let (sk, vk) = keypair_from_seed(&seed);
        let method = "GET";
        let path = "/sync/pull?after=0";
        let ts: i64 = 1_700_000_001;

        let sig_hex = sign_request(&sk, method, path, ts);
        let sig_bytes: Vec<u8> = (0..sig_hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&sig_hex[i..i + 2], 16).unwrap())
            .collect();
        let sig = Signature::from_slice(&sig_bytes).unwrap();

        let message = format!("{method}\n{path}\n{ts}");
        vk.verify_strict(message.as_bytes(), &sig).unwrap();
    }
}
