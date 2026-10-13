//! Connections across protocols in the settings (spec 050 FR-001, contracts/tauri-commands.md): the
//! protocol of a saved connection stays, and credentials must fit it before anything reaches a
//! provider.

use zeroize::Zeroizing;

use super::model::CredentialsInput;
use super::service_tests::{input, setup, typed};
use super::test_support::Op;
use super::Protocol;
use crate::error::HolziError;

fn field<T: std::fmt::Debug>(result: Result<T, HolziError>) -> String {
    match result {
        Err(HolziError::StorageInvalid { field }) => field,
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn the_protocol_of_a_saved_connection_stays() {
    let s = setup();
    let saved = s
        .service
        .save_connection(input(None, Some(typed())))
        .await
        .expect("an S3 connection");
    assert_eq!(saved.protocol, Protocol::S3);
    let mut moved = input(Some(saved.id), None);
    moved.protocol = Protocol::WebDav;
    assert_eq!(field(s.service.save_connection(moved).await), "protocol");
}

#[tokio::test]
async fn credentials_must_fit_the_protocol() {
    let s = setup();
    let login = CredentialsInput::Password {
        username: "u123".to_owned(),
        password: Zeroizing::new("placeholder-password".to_owned()),
    };
    assert_eq!(
        field(s.service.save_connection(input(None, Some(login))).await),
        "credentials",
        "an S3 connection takes an access key, not a password"
    );
    let key = CredentialsInput::SshKey {
        username: "u123".to_owned(),
        private_key: Zeroizing::new("placeholder-key".to_owned()),
        passphrase: None,
    };
    assert_eq!(
        field(s.service.save_connection(input(None, Some(key))).await),
        "credentials"
    );
    assert!(s.fake.calls().is_empty(), "nothing reached the provider");
}

/// A WebDAV connection of the user on a server in the local network.
fn webdav(folder: &str, create_folder: bool) -> super::model::ConnectionInput {
    super::model::ConnectionInput {
        id: None,
        protocol: Protocol::WebDav,
        provider_name: "NAS".to_owned(),
        provider_kind: super::ProviderKind::Other,
        endpoint: Some("http://127.0.0.1:8080/dav".to_owned()),
        region: String::new(),
        addressing: super::Addressing::Path,
        credentials: Some(CredentialsInput::Password {
            username: "anna".to_owned(),
            password: Zeroizing::new("placeholder-password".to_owned()),
        }),
        bucket_for_test: folder.to_owned(),
        create_folder,
        host_key: None,
    }
}

fn outcome<T: std::fmt::Debug>(result: Result<T, HolziError>) -> super::TestOutcome {
    match result {
        Err(HolziError::StorageTestFailed { outcome, .. }) => outcome,
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn a_webdav_connection_is_saved_with_its_address_as_a_folder() {
    let s = setup();
    s.fake.add_folder("Fotos");
    let saved = s
        .service
        .save_connection(webdav("Fotos", false))
        .await
        .expect("saved");
    assert_eq!(saved.protocol, Protocol::WebDav);
    assert_eq!(saved.endpoint, "http://127.0.0.1:8080/dav/");
}

#[tokio::test]
async fn a_missing_folder_is_made_only_when_the_user_says_so() {
    let s = setup();
    assert_eq!(
        outcome(s.service.save_connection(webdav("Fotos", false)).await),
        super::TestOutcome::FolderMissing
    );
    assert!(
        !s.fake.has_folder("Fotos"),
        "nothing made without the user's word"
    );
    assert!(!s.fake.calls().iter().any(|(op, _)| *op == Op::Put));

    s.service
        .save_connection(webdav("Fotos", true))
        .await
        .expect("saved after the folder was made");
    assert!(s.fake.has_folder("Fotos"));
}

#[tokio::test]
async fn a_storage_inside_another_one_of_the_connection_gets_a_warning() {
    let s = setup();
    s.fake.add_folder("Fotos");
    s.fake.add_folder("Fotos/2026");
    s.fake.add_folder("Musik");
    let connection = s
        .service
        .save_connection(webdav("Fotos", false))
        .await
        .expect("connection");
    let storage = |name: &str, folder: &str| super::model::StorageInput {
        id: None,
        connection_id: connection.id.clone(),
        name: name.to_owned(),
        bucket: folder.to_owned(),
        create_folder: false,
    };
    let first = s.service.save_storage(storage("Fotos", "Fotos")).await;
    assert_eq!(first.expect("first").warning, None);
    let inner = s.service.save_storage(storage("2026", "Fotos/2026")).await;
    assert_eq!(
        inner.expect("inner").warning,
        Some(super::model::StorageWarning::NestedFolder)
    );
    let apart = s.service.save_storage(storage("Musik", "Musik")).await;
    assert_eq!(apart.expect("apart").warning, None);
}
