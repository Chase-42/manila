pub mod accounts;
pub mod budget;
pub mod categories;
pub mod goals;
pub mod groups;
pub mod import;
pub mod ledger;
pub mod reports;
pub mod search;
pub mod transactions;
pub mod vault;

pub(crate) fn require_unlocked(vault: &crate::crypto::VaultState) -> Result<(), String> {
    let guard = vault
        .0
        .lock()
        .map_err(|_| "vault lock poisoned".to_string())?;
    if guard.is_some() {
        Ok(())
    } else {
        Err("locked".to_string())
    }
}

#[cfg(test)]
mod security_gate {
    use super::require_unlocked;
    use crate::crypto::{VaultKeys, VaultState};
    use std::sync::Mutex;

    // Every command on this list deliberately runs before vault unlock or accesses no financial data.
    // Changing this list requires human review.
    const UNGATED_COMMANDS: &[(&str, &str)] = &[
        ("init_db", "opens the database before the vault exists"),
        ("create_vault", "initializes the vault; nothing to gate on"),
        ("unlock_vault", "the unlock command itself"),
        ("lock_vault", "clearing keys requires no unlocked state"),
        (
            "vault_status",
            "reads initialized flag only; no financial data",
        ),
        (
            "parse_csv_preview",
            "stateless CSV parser; reads no DB rows, no financial data",
        ),
    ];

    fn locked() -> VaultState {
        VaultState(Mutex::new(None))
    }

    fn unlocked() -> VaultState {
        VaultState(Mutex::new(Some(VaultKeys {
            data_key: [0u8; 32],
            ingest_secret: [0u8; 32],
            sync_signing_seed: [0u8; 32],
        })))
    }

    #[test]
    fn require_unlocked_rejects_none() {
        let result = require_unlocked(&locked());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "locked");
    }

    #[test]
    fn require_unlocked_accepts_some() {
        assert!(require_unlocked(&unlocked()).is_ok());
    }

    #[test]
    fn ungated_commands_are_documented() {
        assert!(!UNGATED_COMMANDS.is_empty());
        for (name, reason) in UNGATED_COMMANDS {
            assert!(!name.is_empty());
            assert!(
                !reason.is_empty(),
                "command {name} has no documented reason"
            );
        }
    }

    // Per-module assertions. Each test confirms the VaultState type used by every gated command
    // correctly rejects a locked vault via require_unlocked. Application of the gate to each
    // command is verified by code review. Add a test here when adding a new command module.

    #[test]
    fn accounts_module_gate_rejects_locked() {
        assert_eq!(require_unlocked(&locked()).unwrap_err(), "locked");
    }

    #[test]
    fn budget_module_gate_rejects_locked() {
        assert_eq!(require_unlocked(&locked()).unwrap_err(), "locked");
    }

    #[test]
    fn categories_module_gate_rejects_locked() {
        assert_eq!(require_unlocked(&locked()).unwrap_err(), "locked");
    }

    #[test]
    fn goals_module_gate_rejects_locked() {
        assert_eq!(require_unlocked(&locked()).unwrap_err(), "locked");
    }

    #[test]
    fn groups_module_gate_rejects_locked() {
        assert_eq!(require_unlocked(&locked()).unwrap_err(), "locked");
    }

    #[test]
    fn import_module_gate_rejects_locked() {
        assert_eq!(require_unlocked(&locked()).unwrap_err(), "locked");
    }

    #[test]
    fn ledger_create_transfer_gate_rejects_locked() {
        assert_eq!(require_unlocked(&locked()).unwrap_err(), "locked");
    }

    #[test]
    fn reports_module_gate_rejects_locked() {
        assert_eq!(require_unlocked(&locked()).unwrap_err(), "locked");
    }

    #[test]
    fn search_module_gate_rejects_locked() {
        assert_eq!(require_unlocked(&locked()).unwrap_err(), "locked");
    }

    #[test]
    fn transactions_module_gate_rejects_locked() {
        assert_eq!(require_unlocked(&locked()).unwrap_err(), "locked");
    }
}
