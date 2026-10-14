//! A connection with an entry the user chose (spec 052 US1, T010): tested with the entry's values,
//! linked without a new entry, nothing linked after a failed test, an entry that is not there or
//! does not fit refused, an SFTP key entry signs in, and a new link replaces the old one only after
//! a passed test.

// These tests count rows directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use super::credentials;
use super::model::{ConnectionInput, CredentialsInput};
use super::protocol::ProtocolStore;
use super::service::StorageService;
use super::service_tests::{count, entries, input, ops, setup, typed};
use super::sftp::{keys, test_server, SftpStore};
use super::test_support::{resolver, vault, FakeStore, Op};
use super::{Addressing, CredentialsState, Protocol, ProviderKind, StorageError, TestOutcome};
use crate::error::HolziError;
use crate::passwords::access::Caller;
use crate::passwords::model::{ItemInput, KeyValueInput, Target, TargetKind};
use crate::passwords::service::PasswordsService;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

const SECRET: &str = "placeholder-secret-of-link-tests";

/// A new entry of the user with an S3 key (user name and password).
async fn chosen(db: &VaultDb, title: &str) -> String {
    PasswordsService::new(db.clone())
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some(title.to_owned()),
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

fn linked(db: &VaultDb) -> String {
    db.read_blocking(|q| {
        Ok(q.query_row(
            "SELECT credentials_item_id FROM haex_storage_connections",
            &[],
            |r| r.get::<_, String>(0),
        )?
        .unwrap_or_default())
    })
    .expect("link")
}

const OWNED: &str = "SELECT COUNT(*) FROM haex_passwords_item_details WHERE owner = 'storage'";

#[tokio::test]
async fn a_chosen_entry_makes_a_connection_without_a_new_entry() {
    let s = setup();
    let id = chosen(&s.db, "IDrive Zugang").await;
    let view = s
        .service
        .save_connection(input(None, link(&id)))
        .await
        .expect("save");
    assert_eq!(view.credentials, CredentialsState::Present);
    assert_eq!(ops(&s.fake), 4, "tested before saving");
    assert_eq!(entries(&s.db), 1, "SC-002: no copy of the entry");
    assert_eq!(linked(&s.db), id);
    let tags =
        s.db.read_blocking(|q| crate::passwords::tags::names_of_item(q, &id).map_err(Into::into))
            .expect("tags");
    assert!(
        tags.iter()
            .any(|tag| tag.eq_ignore_ascii_case(credentials::TAG)),
        "SC-003: linking marks the chosen entry"
    );
    let Ok(super::Credentials::S3(read)) = credentials::read(
        &PasswordsService::new(s.db.clone()),
        &s.db,
        &id,
        Protocol::S3,
    )
    .await
    else {
        panic!("the link reads the entry");
    };
    assert_eq!(read.access_key_id, "AKIDCHOSEN");
    let text = serde_json::to_string(&view).expect("json");
    assert!(!text.contains(SECRET));
}

#[tokio::test]
async fn a_failed_test_with_a_chosen_entry_saves_nothing() {
    let s = setup();
    let id = chosen(&s.db, "IDrive").await;
    s.fake.fail(Op::Put, StorageError::AccessDenied);
    let error = s
        .service
        .save_connection(input(None, link(&id)))
        .await
        .expect_err("refused");
    assert!(matches!(
        error,
        HolziError::StorageTestFailed {
            outcome: TestOutcome::AccessDenied,
            ..
        }
    ));
    assert!(!format!("{error:?}").contains(SECRET));
    assert_eq!(
        count(&s.db, "SELECT COUNT(*) FROM haex_storage_connections"),
        0
    );
    assert_eq!(entries(&s.db), 1, "the chosen entry stays as it was");
}

#[tokio::test]
async fn an_entry_that_is_not_there_or_does_not_fit_is_refused_before_any_test() {
    let s = setup();
    let unavailable = |result: Result<_, HolziError>| match result {
        Err(HolziError::StorageCredentialsUnavailable { state }) => state,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(
        unavailable(s.service.save_connection(input(None, link("nope"))).await),
        CredentialsState::Syncing
    );
    let trashed = chosen(&s.db, "Alt").await;
    PasswordsService::new(s.db.clone())
        .trash_targets(
            &Caller::User,
            vec![Target {
                kind: TargetKind::Item,
                id: trashed.clone(),
            }],
        )
        .await
        .expect("trash");
    assert_eq!(
        unavailable(s.service.save_connection(input(None, link(&trashed))).await),
        CredentialsState::Missing
    );
    let without_password = PasswordsService::new(s.db.clone())
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                username: Some("AKIDCHOSEN".to_owned()),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("entry");
    let refused = s
        .service
        .save_connection(input(None, link(&without_password)))
        .await;
    assert!(
        matches!(refused, Err(HolziError::StorageInvalid { ref field }) if field == "credentials"),
        "{refused:?}"
    );
    assert_eq!(ops(&s.fake), 0, "no test without credentials");
    assert_eq!(
        count(&s.db, "SELECT COUNT(*) FROM haex_storage_connections"),
        0
    );
}

#[tokio::test]
async fn a_new_link_replaces_the_old_one_only_after_a_passed_test() {
    let s = setup();
    let saved = s
        .service
        .save_connection(input(None, Some(typed())))
        .await
        .expect("typed");
    let own = linked(&s.db);
    let id = chosen(&s.db, "IDrive").await;

    s.fake.fail(Op::Put, StorageError::AccessDenied);
    s.service
        .save_connection(input(Some(saved.id.clone()), link(&id)))
        .await
        .expect_err("refused");
    assert_eq!(linked(&s.db), own, "the old link stays");
    assert_eq!(count(&s.db, OWNED), 1);

    s.fake.heal(Op::Put);
    s.service
        .save_connection(input(Some(saved.id.clone()), link(&id)))
        .await
        .expect("linked");
    assert_eq!(linked(&s.db), id);
    assert_eq!(count(&s.db, OWNED), 0, "holzi's unused entry is gone");
    assert_eq!(entries(&s.db), 1);

    s.service
        .save_connection(input(Some(saved.id), link(&id)))
        .await
        .expect("the same entry again");
    assert_eq!(entries(&s.db), 1, "the chosen entry is never deleted");
}

#[tokio::test]
async fn an_entry_holzi_created_for_one_connection_serves_another() {
    let s = setup();
    s.service
        .save_connection(input(None, Some(typed())))
        .await
        .expect("typed");
    let own = linked(&s.db);
    s.service
        .save_connection(input(None, link(&own)))
        .await
        .expect("linked");
    assert_eq!(count(&s.db, OWNED), 1, "no second entry");
    assert_eq!(
        count(
            &s.db,
            &format!(
                "SELECT COUNT(*) FROM haex_storage_connections WHERE credentials_item_id = '{own}'"
            )
        ),
        2
    );
}

#[tokio::test]
async fn an_sftp_key_entry_signs_in() {
    let (private, public) = keys::generate().expect("generate");
    let mut options = test_server::Options::new(31);
    options.password = None;
    options.public_key = Some(keys::parse_host_key(&public).expect("public key"));
    let server = test_server::start(options).await;
    std::fs::create_dir(server.path("Ablage")).expect("folder");
    let (_dir, db) = vault();
    let service = StorageService::new(
        db.clone(),
        PasswordsService::new(db.clone()),
        std::sync::Arc::new(ProtocolStore::new(
            std::sync::Arc::new(FakeStore::new()),
            None,
            Some(std::sync::Arc::new(SftpStore::new(resolver(&[])))),
        )),
        resolver(&[]),
    );
    let id = PasswordsService::new(db.clone())
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("NAS".to_owned()),
                username: Some("anna".to_owned()),
                key_values: vec![KeyValueInput {
                    key: credentials::PRIVATE_KEY.to_owned(),
                    value: Some(private.to_string()),
                }],
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("entry");
    let view = service
        .save_connection(ConnectionInput {
            id: None,
            protocol: Protocol::Sftp,
            provider_name: "NAS".to_owned(),
            provider_kind: ProviderKind::Other,
            endpoint: Some(server.endpoint()),
            region: String::new(),
            addressing: Addressing::Path,
            credentials: link(&id),
            bucket_for_test: "Ablage".to_owned(),
            create_folder: false,
            host_key: Some(server.host_key.clone()),
        })
        .await
        .expect("saved");
    assert_eq!(view.credentials, CredentialsState::Present);
    assert_eq!(linked(&db), id);
}

fn tags_of(db: &VaultDb, id: &str) -> Vec<String> {
    let id = id.to_owned();
    db.read_blocking(move |q| crate::passwords::tags::names_of_item(q, &id).map_err(Into::into))
        .expect("tags")
}

#[tokio::test]
async fn a_chosen_entry_gets_the_tag_only_after_the_save_and_keeps_its_place() {
    let s = setup();
    let passwords = PasswordsService::new(s.db.clone());
    let folder = passwords
        .create_group(&Caller::User, "Cloud".to_owned(), None, None, None, None)
        .await
        .expect("folder");
    let id = passwords
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("IDrive".to_owned()),
                username: Some("AKIDCHOSEN".to_owned()),
                password: Some(SECRET.to_owned()),
                tags: vec!["Arbeit".to_owned()],
                ..ItemInput::default()
            },
            Some(folder.clone()),
        )
        .await
        .expect("entry");

    s.fake.fail(Op::Put, StorageError::AccessDenied);
    s.service
        .save_connection(input(None, link(&id)))
        .await
        .expect_err("refused");
    assert_eq!(
        tags_of(&s.db, &id),
        ["Arbeit"],
        "a failed test tags nothing"
    );

    s.fake.heal(Op::Put);
    s.service
        .save_connection(input(None, link(&id)))
        .await
        .expect("saved");
    let mut tags = tags_of(&s.db, &id);
    tags.sort();
    assert_eq!(tags, ["Arbeit", credentials::TAG], "FR-006: tag added");
    let header = passwords
        .get_item(&Caller::User, id)
        .await
        .expect("entry")
        .header;
    assert_eq!(header.owner, None, "FR-008: still the user's entry");
    assert_eq!(header.group_id.as_deref(), Some(folder.as_str()));
}

#[tokio::test]
async fn the_overview_names_the_entry_of_each_connection_while_it_is_there() {
    let s = setup();
    let typed_view = s
        .service
        .save_connection(input(None, Some(typed())))
        .await
        .expect("typed");
    let own = typed_view.credentials_entry.clone().expect("entry shown");
    assert!(own.owned);
    assert_eq!(own.title.as_deref(), Some("S3: RustFS"));
    let id = chosen(&s.db, "IDrive").await;
    let chosen_view = s
        .service
        .save_connection(input(None, link(&id)))
        .await
        .expect("chosen");
    assert_eq!(
        chosen_view
            .credentials_entry
            .clone()
            .map(|e| (e.id, e.title, e.owned)),
        Some((id.clone(), Some("IDrive".to_owned()), false))
    );
    let overview = s.service.overview().await.expect("overview");
    assert_eq!(overview.connections.len(), 2);
    assert!(overview
        .connections
        .iter()
        .all(|c| c.credentials_entry.is_some()));
    let text = serde_json::to_string(&overview).expect("json");
    assert!(!text.contains(SECRET), "a title, never a value");

    PasswordsService::new(s.db.clone())
        .trash_targets(
            &Caller::User,
            vec![Target {
                kind: TargetKind::Item,
                id: id.clone(),
            }],
        )
        .await
        .expect("trash");
    let gone = s
        .service
        .overview()
        .await
        .expect("overview")
        .connections
        .into_iter()
        .find(|c| c.id == chosen_view.id)
        .expect("connection");
    assert_eq!(gone.credentials, CredentialsState::Missing);
    assert_eq!(gone.credentials_entry, None);
}
