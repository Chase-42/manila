use crate::crypto::CryptoError;

const SERVICE: &str = "com.chasecollins.manila";
const ACCOUNT: &str = "biometric-vault-secret";

/// Returns `true` when biometric unlock hardware and APIs are present.
#[cfg(target_os = "macos")]
pub fn is_biometric_available() -> bool {
    use security_framework::access_control::{ProtectionMode, SecAccessControl};
    use security_framework::passwords::AccessControlOptions;
    SecAccessControl::create_with_protection(
        Some(ProtectionMode::AccessibleWhenUnlocked),
        AccessControlOptions::BIOMETRY_ANY.bits(),
    )
    .is_ok()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn is_biometric_available() -> bool {
    false
}

/// Stores `secret` in the OS secure store protected by biometric access control.
///
/// On macOS the keychain item is OS-enforced with `kSecAttrAccessControl +
/// BIOMETRY_ANY`; Touch ID is required to retrieve it. The item is deleted first
/// to avoid `errSecDuplicateItem` on a re-enable after a prior entry was left.
#[cfg(target_os = "macos")]
pub fn store_vault_secret(secret: &[u8; 32]) -> Result<(), CryptoError> {
    use security_framework::access_control::{ProtectionMode, SecAccessControl};
    use security_framework::passwords::{set_generic_password_options, AccessControlOptions, PasswordOptions};

    // Idempotent delete first so SecItemAdd never hits errSecDuplicateItem.
    delete_vault_secret()?;

    let access_control = SecAccessControl::create_with_protection(
        Some(ProtectionMode::AccessibleWhenUnlocked),
        AccessControlOptions::BIOMETRY_ANY.bits(),
    )
    .map_err(|_| CryptoError::BiometricStoreFailed)?;

    let mut opts = PasswordOptions::new_generic_password(SERVICE, ACCOUNT);
    opts.set_access_control(access_control);

    set_generic_password_options(secret, opts).map_err(|_| CryptoError::BiometricStoreFailed)
}

/// Retrieves the vault secret from the OS secure store.
///
/// On macOS this triggers the Touch ID dialog. Returns an error if biometric
/// authentication fails, the user cancels, or no item is stored.
#[cfg(target_os = "macos")]
pub fn retrieve_vault_secret() -> Result<[u8; 32], CryptoError> {
    use security_framework::passwords::{generic_password, PasswordOptions};

    let opts = PasswordOptions::new_generic_password(SERVICE, ACCOUNT);
    let bytes = generic_password(opts).map_err(|_| CryptoError::BiometricUnlockFailed)?;
    bytes
        .try_into()
        .map_err(|_| CryptoError::BiometricUnlockFailed)
}

/// Deletes the stored vault secret. Succeeds even when no item exists.
#[cfg(target_os = "macos")]
pub fn delete_vault_secret() -> Result<(), CryptoError> {
    use security_framework::passwords::{delete_generic_password_options, PasswordOptions};

    let opts = PasswordOptions::new_generic_password(SERVICE, ACCOUNT);
    match delete_generic_password_options(opts) {
        Ok(()) => Ok(()),
        Err(e) if e.code() == -25300 => Ok(()), // errSecItemNotFound — already gone
        Err(_) => Err(CryptoError::BiometricStoreFailed),
    }
}

// ── Windows path via keyring ──────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub fn is_biometric_available() -> bool {
    // Windows Credential Manager provides no biometric gate. Feature is hidden
    // until a UserConsentVerifier / Windows Hello integration ships.
    false
}

#[cfg(target_os = "windows")]
pub fn store_vault_secret(secret: &[u8; 32]) -> Result<(), CryptoError> {
    use keyring::Entry;
    let entry = Entry::new(SERVICE, ACCOUNT).map_err(|_| CryptoError::BiometricStoreFailed)?;
    entry
        .set_secret(secret)
        .map_err(|_| CryptoError::BiometricStoreFailed)
}

#[cfg(target_os = "windows")]
pub fn retrieve_vault_secret() -> Result<[u8; 32], CryptoError> {
    use keyring::Entry;
    let entry = Entry::new(SERVICE, ACCOUNT).map_err(|_| CryptoError::BiometricUnlockFailed)?;
    let bytes = entry
        .get_secret()
        .map_err(|_| CryptoError::BiometricUnlockFailed)?;
    bytes
        .try_into()
        .map_err(|_| CryptoError::BiometricUnlockFailed)
}

#[cfg(target_os = "windows")]
pub fn delete_vault_secret() -> Result<(), CryptoError> {
    use keyring::{Entry, Error as KeyringError};
    let entry = Entry::new(SERVICE, ACCOUNT).map_err(|_| CryptoError::BiometricStoreFailed)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(KeyringError::NoEntry) => Ok(()), // idempotent
        Err(_) => Err(CryptoError::BiometricStoreFailed),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Retrieve when nothing stored returns an error (no biometric prompt on
    // macOS because the item does not exist).
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn retrieve_when_no_item_returns_error() {
        // Clean up any leftover item from a prior test run.
        let _ = delete_vault_secret();
        let result = retrieve_vault_secret();
        assert!(
            result.is_err(),
            "retrieve_vault_secret should fail when no item is stored"
        );
    }

    // Delete when nothing stored must succeed (idempotent).
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn delete_when_nothing_stored_is_ok() {
        // Ensure the item is absent before we test idempotency.
        let _ = delete_vault_secret();
        assert!(
            delete_vault_secret().is_ok(),
            "delete_vault_secret should be idempotent"
        );
    }

    // Full round-trip requires Touch ID on macOS. Windows path is currently
    // unreachable (is_biometric_available returns false); enable when a Windows
    // Hello integration ships and update the #[ignore] rationale.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[ignore]
    #[test]
    fn store_retrieve_delete_roundtrip_requires_biometric() {
        let secret: [u8; 32] = core::array::from_fn(|i| i as u8);
        store_vault_secret(&secret).expect("store_vault_secret");
        let got = retrieve_vault_secret().expect("retrieve_vault_secret");
        assert_eq!(secret, got);
        delete_vault_secret().expect("delete_vault_secret");
        assert!(retrieve_vault_secret().is_err());
    }
}
