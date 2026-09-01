use argon2::{Algorithm, Argon2, Params, Version};

pub fn derive_master_key(password: &[u8], salt: &[u8; 32]) -> [u8; 32] {
    let params = Params::new(
        65_536, // 64 MiB
        3,      // iterations
        1,      // parallelism
        Some(32),
    )
    .expect("argon2 params are valid constants");

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut output = [0u8; 32];
    argon2
        .hash_password_into(password, salt, &mut output)
        .expect("argon2 hash into fixed-size output never fails on valid params");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference vector: Argon2id, m=65536, t=3, p=1, password=b"password", salt=[0u8; 32], len=32.
    // Generated independently via the argon2 CLI and cross-checked against the crate.
    const EXPECTED: [u8; 32] = [
        59, 34, 87, 79, 104, 60, 194, 242, 252, 62, 245, 156, 108, 134, 52, 42, 213, 144, 226, 45,
        47, 2, 243, 249, 77, 27, 254, 154, 50, 226, 170, 4,
    ];

    #[test]
    fn fixed_input_produces_expected_output() {
        let result = derive_master_key(b"password", &[0u8; 32]);
        assert_eq!(result, EXPECTED);
    }

    #[test]
    fn wrong_password_produces_different_key() {
        let a = derive_master_key(b"password", &[0u8; 32]);
        let b = derive_master_key(b"wrongpassword", &[0u8; 32]);
        assert_ne!(a, b);
    }
}
