//! The settings' side of storage connections against the fake provider (contracts/tauri-commands.md,
//! T025): saved only after a passed test, nothing kept after a failed one, no secret in any answer,
//! tests remembered, removal previewed and done with the credentials entry.

// These tests count rows directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use zeroize::Zeroizing;

use super::model::{ConnectionInput, CredentialsInput, RemovalTarget, StorageInput};
use super::service::StorageService;
use super::test_support::{grant_storage, resolver, vault, FakeStore, Op};
use super::{
    Addressing, CredentialsState, EndpointScope, Protocol, ProviderKind, StorageError, TestOutcome,
};
use crate::error::HolziError;
use crate::passwords::service::PasswordsService;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

const SECRET: &str = "placeholder-secret-of-service-tests";

pub(super) struct Setup {
    _dir: tempfile::TempDir,
    pub(super) db: VaultDb,
    pub(super) fake: Arc<FakeStore>,
    pub(super) service: StorageService,
}

pub(super) fn setup() -> Setup {
    let (dir, db) = vault();
    let fake = Arc::new(FakeStore::new());
    let names = resolver(&[
        ("s3.example.com", &["203.0.113.7", "2001:db8::7"]),
        ("rustfs.lan", &["192.168.1.5"]),
        ("mixed.example.com", &["203.0.113.7", "192.168.1.5"]),
        ("metadata.example.com", &["169.254.169.254"]),
    ]);
    let service = StorageService::new(
        db.clone(),
        PasswordsService::new(db.clone()),
        fake.clone(),
        names,
    );
    Setup {
        _dir: dir,
        db,
        fake,
        service,
    }
}

pub(super) fn typed() -> CredentialsInput {
    CredentialsInput::S3 {
        access_key_id: "AKIDEXAMPLE".to_owned(),
        secret_access_key: Zeroizing::new(SECRET.to_owned()),
        session_token: None,
    }
}

pub(super) fn input(id: Option<String>, credentials: Option<CredentialsInput>) -> ConnectionInput {
    ConnectionInput {
        id,
        protocol: Protocol::S3,
        provider_name: "RustFS".to_owned(),
        provider_kind: ProviderKind::Rustfs,
        endpoint: Some("http://127.0.0.1:9000".to_owned()),
        region: "us-east-1".to_owned(),
        addressing: Addressing::Path,
        credentials,
        bucket_for_test: "holzi-test".to_owned(),
        create_folder: false,
        host_key: None,
    }
}

pub(super) fn count(db: &VaultDb, sql: &str) -> i64 {
    let sql = sql.to_owned();
    db.read_blocking(move |q| Ok(q.query_row(&sql, &[], |r| r.get::<_, i64>(0))?.unwrap_or(0)))
        .expect("count")
}

pub(super) fn entries(db: &VaultDb) -> i64 {
    count(db, "SELECT COUNT(*) FROM haex_passwords_item_details")
}

pub(super) fn ops(fake: &FakeStore) -> usize {
    fake.calls().len()
}

#[tokio::test]
async fn a_new_connection_is_saved_after_a_passed_test_with_its_credentials_owned() {
    let s = setup();
    let view = s
        .service
        .save_connection(input(None, Some(typed())))
        .await
        .expect("save");
    assert_eq!(view.endpoint_scope, EndpointScope::Local);
    assert_eq!(view.credentials, CredentialsState::Present);
    assert!(view.insecure, "http is marked");
    assert_eq!(ops(&s.fake), 4, "tested before saving");
    assert_eq!(
        count(
            &s.db,
            "SELECT COUNT(*) FROM haex_passwords_item_details WHERE owner = 'storage'"
        ),
        1
    );
    let overview = s.service.overview().await.expect("overview");
    assert_eq!(overview.connections, vec![view]);
    let text = serde_json::to_string(&overview).expect("json");
    assert!(!text.contains(SECRET), "no answer carries the secret");
}

#[tokio::test]
async fn a_failed_test_saves_nothing_and_keeps_no_credentials() {
    let s = setup();
    s.fake.fail(Op::Put, StorageError::AccessDenied);
    let error = s
        .service
        .save_connection(input(None, Some(typed())))
        .await
        .expect_err("refused");
    assert!(matches!(
        error,
        HolziError::StorageTestFailed {
            outcome: TestOutcome::AccessDenied,
            leftover_key: None,
            ..
        }
    ));
    assert!(!format!("{error:?}").contains(SECRET));
    assert_eq!(
        count(&s.db, "SELECT COUNT(*) FROM haex_storage_connections"),
        0
    );
    assert_eq!(entries(&s.db), 0, "no credentials stored (SC-004)");
}

#[tokio::test]
async fn a_new_connection_needs_credentials_and_an_allowed_endpoint() {
    let s = setup();
    let field = |error| match error {
        Err(HolziError::StorageInvalid { field }) => field,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(
        field(s.service.save_connection(input(None, None)).await),
        "credentials"
    );
    for endpoint in [
        "http://203.0.113.7:9000",
        "http://169.254.169.254",
        "ftp://x.example",
        "http://s3.example.com",
        "https://mixed.example.com",
        "https://metadata.example.com",
    ] {
        let mut bad = input(None, Some(typed()));
        bad.endpoint = Some(endpoint.to_owned());
        assert_eq!(
            field(s.service.save_connection(bad).await),
            "endpoint",
            "{endpoint}"
        );
    }
    let mut bad = input(None, Some(typed()));
    bad.bucket_for_test = "Not A Bucket".to_owned();
    assert_eq!(field(s.service.save_connection(bad).await), "bucket");
    assert_eq!(ops(&s.fake), 0, "nothing reached the provider");
}

#[tokio::test]
async fn a_change_is_tested_only_when_it_moves_the_connection_or_brings_credentials() {
    let s = setup();
    let first = s
        .service
        .save_connection(input(None, Some(typed())))
        .await
        .expect("save");
    let id = Some(first.id.clone());
    let before = ops(&s.fake);

    let mut renamed = input(id.clone(), None);
    renamed.provider_name = "Heim".to_owned();
    let view = s.service.save_connection(renamed).await.expect("rename");
    assert_eq!(view.provider_name, "Heim");
    assert_eq!(ops(&s.fake), before, "a new name needs no test");

    let mut moved = input(id.clone(), None);
    moved.region = "eu-central-1".to_owned();
    s.service.save_connection(moved).await.expect("new region");
    assert_eq!(
        ops(&s.fake),
        before + 4,
        "a new region is tested with the stored credentials"
    );

    s.service
        .save_connection(input(id, Some(typed())))
        .await
        .expect("new credentials");
    assert_eq!(
        entries(&s.db),
        1,
        "the old entry is deleted, the new one kept"
    );
}

#[tokio::test]
async fn storages_are_tested_when_new_and_remembered() {
    let s = setup();
    let connection = s
        .service
        .save_connection(input(None, Some(typed())))
        .await
        .expect("save");
    let before = ops(&s.fake);
    let storage = s
        .service
        .save_storage(StorageInput {
            id: None,
            connection_id: connection.id.clone(),
            name: "Fotos".to_owned(),
            bucket: "holzi-test".to_owned(),
            create_folder: false,
        })
        .await
        .expect("storage");
    assert_eq!(ops(&s.fake), before + 4);
    assert_eq!(
        storage.last_test.as_ref().map(|t| t.outcome),
        Some(TestOutcome::Passed)
    );

    let before = ops(&s.fake);
    let renamed = s
        .service
        .save_storage(StorageInput {
            id: Some(storage.id.clone()),
            connection_id: connection.id.clone(),
            name: "Bilder".to_owned(),
            bucket: "holzi-test".to_owned(),
            create_folder: false,
        })
        .await
        .expect("rename");
    assert_eq!(ops(&s.fake), before, "a new name needs no test");
    assert_eq!(renamed.last_test, storage.last_test);

    s.fake.fail(Op::Get, StorageError::Network);
    let result = s.service.test_storage(&storage.id).await.expect("test");
    assert_eq!(result.outcome, TestOutcome::Unreachable);
    let overview = s.service.overview().await.expect("overview");
    assert_eq!(
        overview.storages[0].last_test.as_ref().map(|t| t.outcome),
        Some(TestOutcome::Unreachable)
    );
}

#[tokio::test]
async fn a_removal_is_previewed_and_takes_the_credentials_with_it() {
    let s = setup();
    let connection = s
        .service
        .save_connection(input(None, Some(typed())))
        .await
        .expect("save");
    let mut ids = Vec::new();
    for name in ["Fotos", "Notizen"] {
        let storage = s
            .service
            .save_storage(StorageInput {
                id: None,
                connection_id: connection.id.clone(),
                name: name.to_owned(),
                bucket: "holzi-test".to_owned(),
                create_folder: false,
            })
            .await
            .expect("storage");
        ids.push(storage.id);
    }
    grant_storage(&s.db, "notes", &ids[1], "granted");

    let preview = s
        .service
        .removal_preview(RemovalTarget {
            connection_id: Some(connection.id.clone()),
            storage_id: None,
        })
        .await
        .expect("preview");
    assert_eq!(preview.storages, ["Fotos", "Notizen"]);
    assert_eq!(preview.extensions, ["notes"]);

    s.service.remove_storage(&ids[0]).await.expect("remove one");
    s.service
        .remove_connection(&connection.id)
        .await
        .expect("remove");
    assert_eq!(count(&s.db, "SELECT COUNT(*) FROM haex_storages"), 0);
    assert_eq!(
        count(&s.db, "SELECT COUNT(*) FROM extension_permissions"),
        0
    );
    assert_eq!(entries(&s.db), 0, "the credentials are gone for good");
}

#[tokio::test]
async fn a_test_without_credentials_says_why_and_is_remembered() {
    let s = setup();
    let connection = s
        .service
        .save_connection(input(None, Some(typed())))
        .await
        .expect("save");
    let storage = s
        .service
        .save_storage(StorageInput {
            id: None,
            connection_id: connection.id,
            name: "Fotos".to_owned(),
            bucket: "holzi-test".to_owned(),
            create_folder: false,
        })
        .await
        .expect("storage");
    s.db.write(|tx| {
        tx.execute("DELETE FROM haex_passwords_item_details", &[])?;
        Ok(())
    })
    .await
    .expect("the user deletes the entry");

    assert!(matches!(
        s.service.test_storage(&storage.id).await,
        Err(HolziError::StorageCredentialsUnavailable {
            state: CredentialsState::Missing
        })
    ));
    let overview = s.service.overview().await.expect("overview");
    assert_eq!(
        overview.connections[0].credentials,
        CredentialsState::Missing
    );
    assert_eq!(
        overview.storages[0].last_test.as_ref().map(|t| t.outcome),
        Some(TestOutcome::AccessDenied)
    );
}

#[tokio::test]
async fn a_test_without_synced_credentials_is_not_recorded_as_access_denied() {
    let s = setup();
    let connection = s
        .service
        .save_connection(input(None, Some(typed())))
        .await
        .expect("save");
    let storage = s
        .service
        .save_storage(StorageInput {
            id: None,
            connection_id: connection.id,
            name: "Fotos".to_owned(),
            bucket: "holzi-test".to_owned(),
            create_folder: false,
        })
        .await
        .expect("storage");
    s.db.write(|tx| {
        tx.execute("DELETE FROM haex_passwords_item_details", &[])?;
        tx.execute("DELETE FROM haex_deleted_rows", &[])?;
        Ok(())
    })
    .await
    .expect("the credentials are still syncing");

    assert!(matches!(
        s.service.test_storage(&storage.id).await,
        Err(HolziError::StorageCredentialsUnavailable {
            state: CredentialsState::Syncing
        })
    ));
    let overview = s.service.overview().await.expect("overview");
    assert_eq!(
        overview.storages[0]
            .last_test
            .as_ref()
            .map(|test| test.outcome),
        Some(TestOutcome::Passed),
        "syncing credentials must not look refused"
    );
}

#[tokio::test]
async fn a_changed_storage_is_tested_on_the_connection_it_belongs_to() {
    let s = setup();
    let connection = s
        .service
        .save_connection(input(None, Some(typed())))
        .await
        .expect("save");
    let storage = s
        .service
        .save_storage(StorageInput {
            id: None,
            connection_id: connection.id.clone(),
            name: "Fotos".to_owned(),
            bucket: "holzi-test".to_owned(),
            create_folder: false,
        })
        .await
        .expect("storage");
    let before = ops(&s.fake);
    let changed = s
        .service
        .save_storage(StorageInput {
            id: Some(storage.id.clone()),
            connection_id: "another-connection".to_owned(),
            name: "Fotos".to_owned(),
            bucket: "holzi-other".to_owned(),
            create_folder: false,
        })
        .await
        .expect("tested on the storage's own connection");
    assert_eq!(changed.connection_id, connection.id, "the connection stays");
    assert_eq!(ops(&s.fake), before + 4, "the new bucket is tested");
}
