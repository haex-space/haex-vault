use std::io;

use haex_crdt::db::error::DatabaseError;
use haex_crdt::Error as CrdtError;

use super::HaexError;

#[test]
fn held_vault_lock_maps_to_vault_already_open_elsewhere() {
    let err = CrdtError::Database(DatabaseError::VaultAlreadyOpenElsewhere {
        path: "/vault.db".to_string(),
        source: io::Error::new(io::ErrorKind::WouldBlock, "locked"),
    });

    assert!(matches!(
        HaexError::from(err),
        HaexError::VaultAlreadyOpenElsewhere
    ));
}

#[test]
fn oversized_transaction_keeps_its_sizes() {
    let err = CrdtError::Database(DatabaseError::TransactionTooLarge {
        bytes: 11,
        limit: 10,
    });

    assert!(matches!(
        HaexError::from(err),
        HaexError::TransactionTooLarge {
            bytes: 11,
            limit: 10
        }
    ));
}

#[test]
fn haex_error_survives_the_consumer_round_trip() {
    let err = CrdtError::from(HaexError::NotFound {
        name: "work".to_string(),
    });

    assert!(matches!(
        HaexError::from(err),
        HaexError::NotFound { name } if name == "work"
    ));
}

#[test]
fn foreign_consumer_error_falls_back_to_crdt_init_with_its_text() {
    let err = CrdtError::consumer(io::Error::other("hook failed"));

    assert!(matches!(
        HaexError::from(err),
        HaexError::CrdtInit { reason } if reason == "hook failed"
    ));
}

/// Spec 034 (contracts/tauri-commands.md §Fehlerarten): the password manager's own kinds
/// serialise as `{ kind, ...fields }` with the `Passwords` prefix and never carry a value.
#[test]
fn password_manager_errors_serialise_with_their_kind_and_fields() {
    let cases = [
        (
            HaexError::PasswordsNotFound,
            serde_json::json!({ "kind": "PasswordsNotFound" }),
        ),
        (
            HaexError::PasswordsForbidden,
            serde_json::json!({ "kind": "PasswordsForbidden" }),
        ),
        (
            HaexError::PasswordsConflict {
                reason: "changed".to_string(),
            },
            serde_json::json!({ "kind": "PasswordsConflict", "reason": "changed" }),
        ),
        (
            HaexError::PasswordsAttachmentTooLarge {
                bytes: 27_000_000,
                limit: 26_214_400,
            },
            serde_json::json!({
                "kind": "PasswordsAttachmentTooLarge",
                "bytes": 27_000_000,
                "limit": 26_214_400,
            }),
        ),
        (
            HaexError::PasswordsImportFailed {
                reason: "wrong_credentials".to_string(),
            },
            serde_json::json!({
                "kind": "PasswordsImportFailed",
                "reason": "wrong_credentials",
            }),
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(serde_json::to_value(&error).expect("serialise"), expected);
    }
}

#[test]
fn password_manager_errors_survive_the_consumer_round_trip() {
    let err = CrdtError::from(HaexError::PasswordsConflict {
        reason: "deleted".to_string(),
    });

    assert!(matches!(
        HaexError::from(err),
        HaexError::PasswordsConflict { reason } if reason == "deleted"
    ));
}

/// Spec 017 (contracts/tauri-commands.md): the extension host's own kinds serialise as
/// `{ kind, ...fields }` with the `Extension` prefix and never carry content of an extension.
#[test]
fn extension_errors_serialise_with_their_kind_and_fields() {
    let cases = [
        (
            HaexError::ExtensionInstall {
                reason: "signature_invalid".to_string(),
            },
            serde_json::json!({ "kind": "ExtensionInstall", "reason": "signature_invalid" }),
        ),
        (
            HaexError::ExtensionNotFound,
            serde_json::json!({ "kind": "ExtensionNotFound" }),
        ),
        (
            HaexError::Marketplace {
                reason: "marketplace_hash_mismatch".to_string(),
            },
            serde_json::json!({ "kind": "Marketplace", "reason": "marketplace_hash_mismatch" }),
        ),
        (
            HaexError::ExtensionNotReady {
                status: "migration_failed".to_string(),
            },
            serde_json::json!({ "kind": "ExtensionNotReady", "status": "migration_failed" }),
        ),
        (
            HaexError::ExtensionDisabled,
            serde_json::json!({ "kind": "ExtensionDisabled" }),
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(serde_json::to_value(&error).expect("serialise"), expected);
    }
}
