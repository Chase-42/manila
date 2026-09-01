use hkdf::Hkdf;
use sha2::Sha256;

pub struct DerivedKeys {
    pub data_key: [u8; 32],
    pub ingest_secret: [u8; 32],
    pub sync_signing_seed: [u8; 32],
}

pub fn derive_keys(vault_secret: &[u8; 32]) -> DerivedKeys {
    let hk = Hkdf::<Sha256>::new(None, vault_secret);
    let mut data_key = [0u8; 32];
    let mut ingest_secret = [0u8; 32];
    let mut sync_signing_seed = [0u8; 32];
    hk.expand(b"manila-data-key-v1", &mut data_key)
        .expect("HKDF expand with 32-byte output never fails");
    hk.expand(b"manila-ingest-keypair-v1", &mut ingest_secret)
        .expect("HKDF expand with 32-byte output never fails");
    hk.expand(b"manila-sync-auth-key-v1", &mut sync_signing_seed)
        .expect("HKDF expand with 32-byte output never fails");
    DerivedKeys {
        data_key,
        ingest_secret,
        sync_signing_seed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_secret_produces_identical_keys() {
        let secret = [0x55u8; 32];
        let a = derive_keys(&secret);
        let b = derive_keys(&secret);
        assert_eq!(a.data_key, b.data_key);
        assert_eq!(a.ingest_secret, b.ingest_secret);
        assert_eq!(a.sync_signing_seed, b.sync_signing_seed);
    }

    #[test]
    fn three_keys_are_mutually_distinct() {
        let secret = [0x42u8; 32];
        let keys = derive_keys(&secret);
        assert_ne!(keys.data_key, keys.ingest_secret);
        assert_ne!(keys.data_key, keys.sync_signing_seed);
        assert_ne!(keys.ingest_secret, keys.sync_signing_seed);
    }
}
