//! Connections across protocols in the settings (spec 050 FR-001, contracts/tauri-commands.md): the
//! protocol of a saved connection stays, and credentials must fit it before anything reaches a
//! provider.

use zeroize::Zeroizing;

use super::model::CredentialsInput;
use super::service_tests::{input, setup, typed};
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
