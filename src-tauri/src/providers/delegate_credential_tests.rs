//! The delegate credential entry (spec 096, T010): save, read and delete by scope, duplicates in one
//! scope, another device's entry, and who may see the entry at all.

use std::sync::Arc;

use super::*;
use crate::passwords::items;
use crate::passwords::model::ItemHeader;
use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::VaultGate;

const SECRET: &str = "placeholder-device-code-of-delegate-credential-tests";
const VENDOR: &str = "abacusai";

fn vault() -> (tempfile::TempDir, VaultDb) {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    (dir, vault)
}

/// The entries of the password manager the user sees (the user sees every owned entry).
async fn headers_of_user(db: &VaultDb) -> Vec<ItemHeader> {
    db.read(|q| items::headers_in_scope(q, &Scope::All, &Caller::User).map_err(Into::into))
        .await
        .expect("headers")
}

async fn secret_of(db: &VaultDb, scope: &str) -> Option<String> {
    match read(db, VENDOR, scope).await.expect("read") {
        Read::Ready(credential) => {
            assert_eq!(credential.kind(), KIND_DEVICE_CODE);
            Some(credential.secret().to_owned())
        }
        Read::Missing => None,
    }
}

#[tokio::test]
async fn a_saved_credential_reads_back_in_its_scope_only() {
    let (_dir, db) = vault();
    assert_eq!(secret_of(&db, VAULT_SCOPE).await, None);

    save(&db, VENDOR, KIND_DEVICE_CODE, SECRET, VAULT_SCOPE)
        .await
        .expect("save");

    assert_eq!(secret_of(&db, VAULT_SCOPE).await.as_deref(), Some(SECRET));
    assert_eq!(secret_of(&db, "installation-a").await, None);
}

#[tokio::test]
async fn the_entry_is_titled_tagged_and_owned_by_the_delegate_feature() {
    let (_dir, db) = vault();
    save(&db, VENDOR, KIND_DEVICE_CODE, SECRET, VAULT_SCOPE)
        .await
        .expect("save");

    let headers = headers_of_user(&db).await;
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0].title.as_deref(), Some("Delegate: Abacus AI"));
    assert_eq!(headers[0].owner.as_deref(), Some(FEATURE));
    assert!(headers[0]
        .tags
        .iter()
        .any(|tag| tag.name == "haex-delegate-abacusai"));
    assert_eq!(tag_of(VENDOR), "haex-delegate-abacusai");
}

#[tokio::test]
async fn saving_again_in_the_same_scope_keeps_one_entry_with_the_new_secret() {
    let (_dir, db) = vault();
    save(&db, VENDOR, KIND_DEVICE_CODE, "first-code", VAULT_SCOPE)
        .await
        .expect("first");
    save(&db, VENDOR, KIND_DEVICE_CODE, SECRET, VAULT_SCOPE)
        .await
        .expect("second");

    assert_eq!(headers_of_user(&db).await.len(), 1);
    assert_eq!(secret_of(&db, VAULT_SCOPE).await.as_deref(), Some(SECRET));
}

#[tokio::test]
async fn duplicates_of_one_scope_resolve_to_the_newest_and_the_next_save_removes_the_older() {
    let (_dir, db) = vault();
    // Two entries of the same scope, as two devices saving at once leave them after a sync.
    let passwords = PasswordsService::new(db.clone());
    for secret in ["older-code", "newer-code"] {
        let input = ItemInput {
            title: Some("Delegate: Abacus AI".to_owned()),
            password: Some(secret.to_owned()),
            tags: vec![tag_of(VENDOR)],
            key_values: vec![
                KeyValueInput {
                    key: "kind".to_owned(),
                    value: Some(KIND_DEVICE_CODE.to_owned()),
                },
                KeyValueInput {
                    key: "device".to_owned(),
                    value: Some(VAULT_SCOPE.to_owned()),
                },
            ],
            ..ItemInput::default()
        };
        passwords.create_owned_item(&CALLER, input).await.unwrap();
        // The entries must differ in `updated_at`, which has a resolution of a second.
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    }
    assert_eq!(headers_of_user(&db).await.len(), 2);
    assert_eq!(
        secret_of(&db, VAULT_SCOPE).await.as_deref(),
        Some("newer-code")
    );

    save(&db, VENDOR, KIND_DEVICE_CODE, SECRET, VAULT_SCOPE)
        .await
        .expect("save");

    assert_eq!(headers_of_user(&db).await.len(), 1);
    assert_eq!(secret_of(&db, VAULT_SCOPE).await.as_deref(), Some(SECRET));
}

#[tokio::test]
async fn a_save_on_one_device_preserves_the_entry_of_another() {
    let (_dir, db) = vault();
    save(&db, VENDOR, KIND_DEVICE_CODE, "code-of-a", "installation-a")
        .await
        .expect("a");
    save(&db, VENDOR, KIND_DEVICE_CODE, "code-of-b", "installation-b")
        .await
        .expect("b");

    assert_eq!(headers_of_user(&db).await.len(), 2);
    assert_eq!(
        secret_of(&db, "installation-a").await.as_deref(),
        Some("code-of-a")
    );
    assert_eq!(
        secret_of(&db, "installation-b").await.as_deref(),
        Some("code-of-b")
    );

    // Saving on A again replaces A's entry and leaves B's.
    save(
        &db,
        VENDOR,
        KIND_DEVICE_CODE,
        "new-code-of-a",
        "installation-a",
    )
    .await
    .expect("a again");
    assert_eq!(headers_of_user(&db).await.len(), 2);
    assert_eq!(
        secret_of(&db, "installation-a").await.as_deref(),
        Some("new-code-of-a")
    );
    assert_eq!(
        secret_of(&db, "installation-b").await.as_deref(),
        Some("code-of-b")
    );
}

#[tokio::test]
async fn disconnecting_removes_every_scope_and_a_missing_entry_is_not_an_error() {
    let (_dir, db) = vault();
    delete(&db, VENDOR).await.expect("nothing to delete");

    save(&db, VENDOR, KIND_DEVICE_CODE, "code-of-a", "installation-a")
        .await
        .unwrap();
    save(&db, VENDOR, KIND_DEVICE_CODE, SECRET, VAULT_SCOPE)
        .await
        .unwrap();
    delete(&db, VENDOR).await.expect("delete");

    assert!(headers_of_user(&db).await.is_empty());
    assert_eq!(secret_of(&db, VAULT_SCOPE).await, None);
    assert_eq!(secret_of(&db, "installation-a").await, None);
}

#[tokio::test]
async fn the_entry_of_one_vendor_is_not_the_entry_of_another() {
    let (_dir, db) = vault();
    save(&db, VENDOR, KIND_DEVICE_CODE, SECRET, VAULT_SCOPE)
        .await
        .unwrap();
    save(&db, "claude", "oauth_token", "other-secret", VAULT_SCOPE)
        .await
        .unwrap();

    delete(&db, VENDOR).await.unwrap();

    assert_eq!(secret_of(&db, VAULT_SCOPE).await, None);
    let headers = headers_of_user(&db).await;
    assert_eq!(headers.len(), 1);
    assert!(headers[0]
        .tags
        .iter()
        .any(|tag| tag.name == "haex-delegate-claude"));
}

#[tokio::test]
async fn the_entry_is_invisible_to_the_built_in_agent_and_visible_to_the_user() {
    let (_dir, db) = vault();
    save(&db, VENDOR, KIND_DEVICE_CODE, SECRET, VAULT_SCOPE)
        .await
        .unwrap();
    let passwords = PasswordsService::new(db.clone());

    let agent = passwords
        .list_headers(&Caller::BuiltinAgent, &[])
        .await
        .expect("the agent's list");
    match agent {
        Headers::Agent(rows) => assert!(rows.is_empty(), "the agent sees {} entries", rows.len()),
        Headers::Items(rows) => assert!(rows.is_empty(), "the agent sees {} entries", rows.len()),
    }
    assert_eq!(headers_of_user(&db).await.len(), 1);
}

#[tokio::test]
async fn an_empty_secret_kind_or_scope_is_refused_and_nothing_is_written() {
    let (_dir, db) = vault();
    for (kind, secret, scope) in [
        ("", SECRET, VAULT_SCOPE),
        (KIND_DEVICE_CODE, "", VAULT_SCOPE),
        (KIND_DEVICE_CODE, SECRET, ""),
    ] {
        let result = save(&db, VENDOR, kind, secret, scope).await;
        assert!(
            matches!(result, Err(HaexError::InvalidInput { .. })),
            "{kind:?} {scope:?}"
        );
    }
    assert!(headers_of_user(&db).await.is_empty());
}

#[tokio::test]
async fn an_entry_without_a_secret_or_kind_reads_as_missing() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db.clone());
    let input = ItemInput {
        title: Some("Delegate: Abacus AI".to_owned()),
        tags: vec![tag_of(VENDOR)],
        key_values: vec![KeyValueInput {
            key: "device".to_owned(),
            value: Some(VAULT_SCOPE.to_owned()),
        }],
        ..ItemInput::default()
    };
    passwords.create_owned_item(&CALLER, input).await.unwrap();

    assert_eq!(secret_of(&db, VAULT_SCOPE).await, None);
}

#[test]
fn a_credential_prints_no_secret() {
    let credential = Credential {
        kind: KIND_DEVICE_CODE.to_owned(),
        secret: Zeroizing::new(SECRET.to_owned()),
    };
    let alone = format!("{credential:?}");
    let wrapped = format!("{:?}", Read::Ready(credential));
    assert!(!alone.contains(SECRET), "{alone}");
    assert!(!wrapped.contains(SECRET), "{wrapped}");
}

#[tokio::test]
async fn deleting_one_scope_leaves_the_others() {
    let (_dir, db) = vault();
    save(&db, VENDOR, KIND_DEVICE_CODE, "code-of-a", "installation-a")
        .await
        .unwrap();
    save(&db, VENDOR, KIND_DEVICE_CODE, SECRET, VAULT_SCOPE)
        .await
        .unwrap();

    delete_scope(&db, VENDOR, "installation-a").await.unwrap();
    delete_scope(&db, VENDOR, "installation-b")
        .await
        .expect("nothing to delete");

    assert_eq!(secret_of(&db, "installation-a").await, None);
    assert_eq!(secret_of(&db, VAULT_SCOPE).await.as_deref(), Some(SECRET));
}
