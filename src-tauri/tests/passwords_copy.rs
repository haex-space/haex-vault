//! The copy through the public service (spec 036, FR-015, FR-022, `passwords_copy`): only the user
//! may copy; a copy of an entry with its password as a reference follows the original; a folder
//! arrives with its content in the target folder.

// These tests read raw vault state that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};

use holzi_lib::identity::{
    haex_migration_source, installation_id_path, HaexBootstrap, HAEX_TRIGGER_VERSION,
};
use holzi_lib::passwords::access::Caller;
use holzi_lib::passwords::copy::{CopyOptions, CopyTitle};
use holzi_lib::passwords::model::{ItemInput, ItemPatch, Patch, SecretField, Target, TargetKind};
use holzi_lib::passwords::service::PasswordsService;
use holzi_lib::vault_gate::VaultGate;
use holzi_lib::HaexError;

fn service() -> (tempfile::TempDir, Database, PasswordsService) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("passwords-copy"),
        create_if_missing: true,
        bootstrap: Arc::new(HaexBootstrap::new(installation_id_path(dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: haex_migration_source(),
        trigger_version: HAEX_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    let vault_db = VaultGate::new()
        .vault_db(Arc::new(db.clone()))
        .expect("open the gate");
    (dir, db, PasswordsService::new(vault_db))
}

fn options(title: CopyTitle) -> CopyOptions {
    CopyOptions {
        title,
        history: false,
        username_as_reference: false,
        password_as_reference: false,
        passkeys_as_links: None,
    }
}

#[tokio::test]
async fn only_the_user_may_copy() {
    let (_dir, _db, service) = service();
    let id = service
        .create_item(&Caller::User, &[], ItemInput::default(), None)
        .await
        .expect("create");
    let caller = Caller::Extension {
        id: "ext".to_string(),
    };
    let result = service
        .copy(
            &caller,
            vec![Target {
                kind: TargetKind::Item,
                id,
            }],
            None,
            options(CopyTitle::Suffix(" – Copy".to_string())),
        )
        .await;
    assert!(matches!(result, Err(HaexError::PasswordsForbidden)));
}

#[tokio::test]
async fn a_copy_with_the_password_as_reference_follows_the_original() {
    let (_dir, db, service) = service();
    let original = service
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("Konto".to_string()),
                password: Some("eins".to_string()),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("create");
    let folder = service
        .create_group(&Caller::User, "Ziel".to_string(), None, None, None, None)
        .await
        .expect("folder");
    let mut by_reference = options(CopyTitle::Exact("Konto – Kopie".to_string()));
    by_reference.password_as_reference = true;
    let report = service
        .copy(
            &Caller::User,
            vec![Target {
                kind: TargetKind::Item,
                id: original.clone(),
            }],
            Some(folder.clone()),
            by_reference,
        )
        .await
        .expect("copy");
    assert_eq!(report.items_created, 1);
    let copy_id: String = db
        .with_connection(|c| {
            Ok(c.query_row(
                "SELECT item_id FROM haex_passwords_group_items WHERE group_id = ?1",
                [&folder],
                |r| r.get(0),
            )?)
        })
        .expect("copy in the folder");
    let token = service
        .get_item(&Caller::User, original.clone())
        .await
        .expect("get")
        .header
        .updated_at
        .expect("token");
    service
        .update_item(
            &Caller::User,
            &[],
            original,
            token,
            ItemPatch {
                password: Patch::Set("zwei".to_string()),
                ..ItemPatch::default()
            },
        )
        .await
        .expect("update");
    let revealed = service
        .reveal(&Caller::User, copy_id, SecretField::Password)
        .await
        .expect("reveal");
    assert_eq!(revealed.value.as_str(), "zwei");
}
