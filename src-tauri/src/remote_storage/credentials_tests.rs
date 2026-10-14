//! The credentials entry of a connection (038 data-model.md, T023, T026; 050 data-model.md): created
//! owned by `storage`, read back as the connection's credentials per protocol, its state
//! `present`/`missing`/`syncing`, and the warning of the password manager before the user deletes it.

use std::sync::Arc;

use zeroize::Zeroizing;

use super::credentials::{self, StorageUsage, FEATURE};
use super::test_support::{credentials as placeholder, vault};
use super::{
    store, Addressing, ConnectionRow, Credentials, CredentialsState, EndpointScope,
    PasswordCredentials, Protocol, ProviderKind, SshKeyCredentials,
};
use crate::error::HolziError;
use crate::passwords::access::Caller;
use crate::passwords::model::{Target, TargetKind};
use crate::passwords::service::PasswordsService;
use crate::passwords::usage::UsageRegistry;

#[tokio::test]
async fn new_credentials_become_an_owned_entry_that_reads_back() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db.clone());
    let Credentials::S3(mut typed) = placeholder() else {
        panic!("the placeholder is S3");
    };
    typed.session_token = Some(Zeroizing::new("token-placeholder".to_owned()));
    let id = credentials::create(
        &passwords,
        Protocol::S3,
        " RustFS ",
        "http://127.0.0.1:9000",
        &Credentials::S3(typed.clone()),
    )
    .await
    .expect("create");

    let header = passwords
        .get_item(&Caller::User, id.clone())
        .await
        .expect("the user sees it")
        .header;
    assert_eq!(header.title.as_deref(), Some("S3: RustFS"));
    assert_eq!(header.owner.as_deref(), Some(FEATURE));

    let Credentials::S3(read) = credentials::read(&passwords, &db, &id, Protocol::S3)
        .await
        .expect("read")
    else {
        panic!("S3 credentials");
    };
    assert_eq!(read.access_key_id, typed.access_key_id);
    assert_eq!(
        read.secret_access_key.as_str(),
        typed.secret_access_key.as_str()
    );
    assert_eq!(
        read.session_token.as_deref().map(String::as_str),
        Some("token-placeholder")
    );
    assert!(!format!("{read:?}").contains("placeholder-secret"));
    assert_eq!(
        credentials::state(&db, &id, Protocol::S3)
            .await
            .expect("state"),
        CredentialsState::Present
    );
}

#[tokio::test]
async fn an_entry_the_user_deleted_is_missing_and_one_not_yet_arrived_is_syncing() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db.clone());
    let unavailable = |result| match result {
        Err(HolziError::StorageCredentialsUnavailable { state }) => state,
        other => panic!("unexpected {other:?}"),
    };

    assert_eq!(
        unavailable(credentials::read(&passwords, &db, "not-arrived", Protocol::S3).await),
        CredentialsState::Syncing
    );

    let trashed = credentials::create(&passwords, Protocol::S3, "A", "", &placeholder())
        .await
        .expect("create");
    passwords
        .trash_targets(
            &Caller::User,
            vec![Target {
                kind: TargetKind::Item,
                id: trashed.clone(),
            }],
        )
        .await
        .expect("the user moves it to the trash");
    assert_eq!(
        unavailable(credentials::read(&passwords, &db, &trashed, Protocol::S3).await),
        CredentialsState::Missing
    );

    let purged = credentials::create(&passwords, Protocol::S3, "B", "", &placeholder())
        .await
        .expect("create");
    credentials::delete(&passwords, &purged)
        .await
        .expect("delete");
    assert_eq!(
        unavailable(credentials::read(&passwords, &db, &purged, Protocol::S3).await),
        CredentialsState::Missing,
        "the delete marker says it was deleted, not that it is on its way"
    );
    credentials::delete(&passwords, &purged)
        .await
        .expect("deleting it again is fine");
}

#[tokio::test]
async fn deleting_the_credentials_of_a_connection_warns_in_the_password_manager() {
    let (_dir, db) = vault();
    let usage = Arc::new(UsageRegistry::new());
    let handle = db.clone();
    usage.register(Arc::new(StorageUsage::new(move || Some(handle.clone()))));
    let passwords = PasswordsService::with_usage(db.clone(), usage);
    let id = credentials::create(&passwords, Protocol::S3, "RustFS", "", &placeholder())
        .await
        .expect("create");
    assert!(passwords
        .item_usage(&Caller::User, &id)
        .expect("usage")
        .is_empty());

    let row = ConnectionRow {
        id: "c1".to_owned(),
        protocol: Protocol::S3,
        provider_name: "RustFS".to_owned(),
        provider_kind: ProviderKind::Rustfs,
        endpoint: "http://127.0.0.1:9000".to_owned(),
        endpoint_scope: EndpointScope::Local,
        region: "us-east-1".to_owned(),
        addressing: Addressing::Path,
        host_key: None,
        host_key_confirmed_at: None,
        credentials_item_id: id.clone(),
        created_at: "t".to_owned(),
        updated_at: "t".to_owned(),
    };
    db.write(move |tx| Ok(store::put_connection(tx, &row)?))
        .await
        .expect("connection");
    let check = id.clone();
    let features = tokio::task::spawn_blocking(move || {
        passwords.item_usage(&Caller::User, &check).expect("usage")
    })
    .await
    .expect("join");
    assert_eq!(features, [FEATURE]);
}

#[tokio::test]
async fn webdav_and_sftp_credentials_read_back_per_protocol() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db.clone());
    let login = Credentials::Password(PasswordCredentials {
        username: "u123".to_owned(),
        password: Zeroizing::new("placeholder-password".to_owned()),
    });
    let id = credentials::create(&passwords, Protocol::WebDav, "Box", "https://box/", &login)
        .await
        .expect("create");
    let header = passwords
        .get_item(&Caller::User, id.clone())
        .await
        .expect("the user sees it")
        .header;
    assert_eq!(header.title.as_deref(), Some("WebDAV: Box"));
    let Credentials::Password(read) = credentials::read(&passwords, &db, &id, Protocol::WebDav)
        .await
        .expect("read")
    else {
        panic!("a password login");
    };
    assert_eq!(read.username, "u123");
    assert_eq!(read.password.as_str(), "placeholder-password");

    let key = Credentials::SshKey(SshKeyCredentials {
        username: "u123".to_owned(),
        private_key: Zeroizing::new("-----BEGIN OPENSSH PRIVATE KEY-----\nplaceholder".to_owned()),
        passphrase: Some(Zeroizing::new("placeholder-passphrase".to_owned())),
    });
    let id = credentials::create(&passwords, Protocol::Sftp, "Box", "sftp://box:23", &key)
        .await
        .expect("create");
    let Credentials::SshKey(read) = credentials::read(&passwords, &db, &id, Protocol::Sftp)
        .await
        .expect("read")
    else {
        panic!("a key login");
    };
    assert_eq!(read.username, "u123");
    assert!(read
        .private_key
        .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
    assert_eq!(
        read.passphrase.as_deref().map(String::as_str),
        Some("placeholder-passphrase")
    );
    let shown = format!("{read:?}");
    assert!(!shown.contains("placeholder-passphrase") && !shown.contains("BEGIN OPENSSH"));
}

#[tokio::test]
async fn an_entry_without_the_fields_of_the_protocol_is_missing() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db.clone());
    let key = Credentials::SshKey(SshKeyCredentials {
        username: "u123".to_owned(),
        private_key: Zeroizing::new("placeholder-key".to_owned()),
        passphrase: None,
    });
    let id = credentials::create(&passwords, Protocol::Sftp, "Box", "sftp://box", &key)
        .await
        .expect("create");
    // A key login has no password, which WebDAV needs.
    match credentials::read(&passwords, &db, &id, Protocol::WebDav).await {
        Err(HolziError::StorageCredentialsUnavailable { state }) => {
            assert_eq!(state, CredentialsState::Missing);
        }
        other => panic!("unexpected {other:?}"),
    }
}
