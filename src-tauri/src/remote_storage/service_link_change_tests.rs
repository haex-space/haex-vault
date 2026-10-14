//! A linked entry is the only source of the credentials (spec 052 US3, T032): a changed value
//! applies on the next call, removing a connection never deletes an entry the user chose and
//! deletes holzi's own only when no other connection uses it, and an entry in the trash or deleted
//! for good leaves the connection asking for new credentials without naming any value.

// These tests count rows directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use super::model::{CredentialsInput, StorageInput};
use super::service_tests::{count, entries, input, setup, typed, Setup};
use super::{Credentials, CredentialsState};
use crate::error::HolziError;
use crate::passwords::access::Caller;
use crate::passwords::model::{ItemInput, ItemPatch, Patch, Target, TargetKind};
use crate::passwords::service::PasswordsService;

const SECRET: &str = "placeholder-secret-of-change-tests";
const OWNED: &str = "SELECT COUNT(*) FROM haex_passwords_item_details WHERE owner = 'storage'";

async fn chosen(s: &Setup) -> String {
    PasswordsService::new(s.db.clone())
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("IDrive".to_owned()),
                username: Some("AKIDCHOSEN".to_owned()),
                password: Some(SECRET.to_owned()),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("entry")
}

fn link(item_id: &str) -> Option<CredentialsInput> {
    Some(CredentialsInput::Entry {
        item_id: item_id.to_owned(),
    })
}

/// A connection on `credentials` with a storage on its tested bucket; the ids of both.
async fn connect(s: &Setup, credentials: Option<CredentialsInput>) -> (String, String) {
    let connection = s
        .service
        .save_connection(input(None, credentials))
        .await
        .expect("connection");
    let storage = s
        .service
        .save_storage(StorageInput {
            id: None,
            connection_id: connection.id.clone(),
            name: "Ablage".to_owned(),
            bucket: "holzi-test".to_owned(),
            create_folder: false,
        })
        .await
        .expect("storage");
    (connection.id, storage.id)
}

async fn secret_of(s: &Setup, storage_id: &str) -> String {
    match s
        .service
        .access_of(storage_id)
        .await
        .expect("access")
        .credentials
    {
        Credentials::S3(s3) => s3.secret_access_key.to_string(),
        other => panic!("S3 credentials, not {other:?}"),
    }
}

#[tokio::test]
async fn a_changed_value_of_the_entry_applies_on_the_next_call() {
    let s = setup();
    let id = chosen(&s).await;
    let (_, storage) = connect(&s, link(&id)).await;
    assert_eq!(secret_of(&s, &storage).await, SECRET);

    let passwords = PasswordsService::new(s.db.clone());
    let token = passwords
        .get_item(&Caller::User, id.clone())
        .await
        .expect("entry")
        .header
        .updated_at
        .unwrap_or_default();
    passwords
        .update_item(
            &Caller::User,
            &[],
            id,
            token,
            ItemPatch {
                password: Patch::Set("placeholder-new-secret".to_owned()),
                ..ItemPatch::default()
            },
        )
        .await
        .expect("update");
    assert_eq!(
        secret_of(&s, &storage).await,
        "placeholder-new-secret",
        "SC-004: no copy, the entry is read on every call"
    );
}

#[tokio::test]
async fn removing_connections_keeps_a_chosen_entry_and_takes_holzis_own_with_the_last_one() {
    let s = setup();
    let id = chosen(&s).await;
    let (first, _) = connect(&s, link(&id)).await;
    let (second, _) = connect(&s, link(&id)).await;
    s.service.remove_connection(&first).await.expect("remove");
    s.service.remove_connection(&second).await.expect("remove");
    assert_eq!(entries(&s.db), 1, "FR-012: the chosen entry stays");

    let (typed_connection, _) = connect(&s, Some(typed())).await;
    let own = s.service.overview().await.expect("overview").connections[0]
        .credentials_entry
        .clone()
        .expect("entry")
        .id;
    let (linked, _) = connect(&s, link(&own)).await;
    s.service
        .remove_connection(&typed_connection)
        .await
        .expect("remove");
    assert_eq!(count(&s.db, OWNED), 1, "another connection still uses it");
    s.service.remove_connection(&linked).await.expect("remove");
    assert_eq!(count(&s.db, OWNED), 0, "gone with the last connection");
    assert_eq!(entries(&s.db), 1);
}

#[tokio::test]
async fn an_entry_in_the_trash_or_deleted_asks_for_new_credentials_without_a_value() {
    let s = setup();
    let id = chosen(&s).await;
    let (connection, storage) = connect(&s, link(&id)).await;
    let passwords = PasswordsService::new(s.db.clone());
    passwords
        .trash_targets(
            &Caller::User,
            vec![Target {
                kind: TargetKind::Item,
                id: id.clone(),
            }],
        )
        .await
        .expect("trash");
    let missing = || async {
        let overview = s.service.overview().await.expect("overview");
        let view = overview
            .connections
            .into_iter()
            .find(|c| c.id == connection)
            .expect("connection");
        assert_eq!(view.credentials, CredentialsState::Missing);
        let error = s.service.access_of(&storage).await.expect_err("no access");
        assert!(matches!(
            error,
            HolziError::StorageCredentialsUnavailable {
                state: CredentialsState::Missing
            }
        ));
        assert!(!format!("{error:?} {error}").contains(SECRET));
    };
    missing().await;
    passwords
        .empty_trash(&Caller::User, false)
        .await
        .expect("delete for good");
    missing().await;
}
