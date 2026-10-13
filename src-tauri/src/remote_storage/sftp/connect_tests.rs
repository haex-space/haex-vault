//! Connecting to an SFTP server (spec 050 research R3, R5, T064): an unknown or changed host key
//! stops the connection before any login (SC-004) and names the key the server showed; with the
//! confirmed key a password and a key sign in; a wrong password is refused; a slow server runs
//! into the deadline; an address holzi never reaches is not dialled.

use std::time::Duration;

use tokio::time::Instant;
use zeroize::Zeroizing;

use super::connect::connect;
use super::keys::{fingerprint, generate};
use super::test_server::{self, attempts, Options};
use crate::remote_storage::test_support::resolver;
use crate::remote_storage::{
    Access, Addressing, Credentials, EndpointScope, Location, PasswordCredentials, Protocol,
    ProviderKind, SshKeyCredentials, StorageError,
};

pub(super) fn access(endpoint: &str, host_key: Option<&str>, credentials: Credentials) -> Access {
    Access {
        location: Location {
            protocol: Protocol::Sftp,
            provider_kind: ProviderKind::Other,
            endpoint: endpoint.to_owned(),
            endpoint_scope: EndpointScope::Local,
            region: String::new(),
            addressing: Addressing::Path,
            bucket: String::new(),
            host_key: host_key.map(str::to_owned),
        },
        credentials,
    }
}

pub(super) fn password(text: &str) -> Credentials {
    Credentials::Password(PasswordCredentials {
        username: "anna".to_owned(),
        password: Zeroizing::new(text.to_owned()),
    })
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[tokio::test]
async fn an_unknown_host_key_stops_before_any_login_and_names_the_key() {
    let server = test_server::start(Options::new(1)).await;
    let refused = connect(
        resolver(&[]).as_ref(),
        &access(&server.endpoint(), None, password("password")),
        soon(),
    )
    .await
    .err()
    .expect("refused");
    assert_eq!(refused.error, StorageError::HostKeyUnknown);
    let shown = refused.shown_key.expect("the shown key");
    assert_eq!(shown, server.host_key);
    assert!(fingerprint(&shown)
        .expect("fingerprint")
        .starts_with("SHA256:"));
    assert_eq!(attempts(&server), 0, "no credentials were sent");
}

#[tokio::test]
async fn a_changed_host_key_stops_before_any_login() {
    let server = test_server::start(Options::new(1)).await;
    let other = test_server::start(Options::new(2)).await;
    let refused = connect(
        resolver(&[]).as_ref(),
        &access(
            &server.endpoint(),
            Some(&other.host_key),
            password("password"),
        ),
        soon(),
    )
    .await
    .err()
    .expect("refused");
    assert_eq!(refused.error, StorageError::HostKeyChanged);
    assert_eq!(refused.shown_key.as_deref(), Some(server.host_key.as_str()));
    assert_eq!(attempts(&server), 0, "SC-004: zero login attempts");
}

#[tokio::test]
async fn the_confirmed_key_lets_a_password_and_a_key_sign_in() {
    let (private, public) = generate().expect("generate");
    let mut options = Options::new(3);
    options.public_key = Some(super::keys::parse_host_key(&public).expect("public"));
    let server = test_server::start(options).await;
    let names = resolver(&[]);
    let session = connect(
        names.as_ref(),
        &access(
            &server.endpoint(),
            Some(&server.host_key),
            password("password"),
        ),
        soon(),
    )
    .await
    .expect("password");
    assert!(session.extensions.posix_rename);
    let key = Credentials::SshKey(SshKeyCredentials {
        username: "anna".to_owned(),
        private_key: private,
        passphrase: None,
    });
    connect(
        names.as_ref(),
        &access(&server.endpoint(), Some(&server.host_key), key),
        soon(),
    )
    .await
    .expect("key");
    let wrong = connect(
        names.as_ref(),
        &access(
            &server.endpoint(),
            Some(&server.host_key),
            password("falsch"),
        ),
        soon(),
    )
    .await
    .err()
    .expect("refused");
    assert_eq!(wrong.error, StorageError::AccessDenied);
}

#[tokio::test]
async fn a_server_that_never_answers_runs_into_the_deadline() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let endpoint = format!("sftp://{}", listener.local_addr().expect("address"));
    let held = tokio::spawn(async move {
        let mut open = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            open.push(stream);
        }
    });
    let started = std::time::Instant::now();
    let refused = connect(
        resolver(&[]).as_ref(),
        &access(&endpoint, None, password("password")),
        Instant::now() + Duration::from_millis(300),
    )
    .await
    .err()
    .expect("refused");
    assert_eq!(refused.error, StorageError::TimedOut, "FR-013");
    assert!(started.elapsed() < Duration::from_secs(5));
    held.abort();
}

#[tokio::test]
async fn an_address_holzi_never_reaches_is_not_dialled() {
    let refused = connect(
        resolver(&[]).as_ref(),
        &access("sftp://169.254.169.254", None, password("password")),
        soon(),
    )
    .await
    .err()
    .expect("refused");
    assert_eq!(refused.error, StorageError::Network);
    let mut public = access("sftp://203.0.113.9", None, password("password"));
    public.location.endpoint_scope = EndpointScope::Local;
    let refused = connect(resolver(&[]).as_ref(), &public, soon())
        .await
        .err()
        .expect("refused");
    assert_eq!(refused.error, StorageError::Network, "out of its scope");
}
