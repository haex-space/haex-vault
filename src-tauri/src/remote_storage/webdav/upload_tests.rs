//! Upload sessions of WebDAV (spec 050 research R7): an unknown session, a `PUT` the server
//! refuses, which comes back with the temporary file to delete, and the deadlines of a part and of
//! the answer (FR-013).

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};
use zeroize::Zeroizing;

use super::upload::Sessions;
use crate::remote_storage::test_support::resolver;
use crate::remote_storage::{
    Access, Addressing, Credentials, EndpointScope, Location, PasswordCredentials, Protocol,
    ProviderKind, StorageError,
};

fn access(server: &MockServer) -> Access {
    access_at(&server.uri())
}

fn access_at(uri: &str) -> Access {
    Access {
        location: Location {
            protocol: Protocol::WebDav,
            provider_kind: ProviderKind::Other,
            endpoint: format!("{uri}/"),
            endpoint_scope: EndpointScope::Local,
            region: String::new(),
            addressing: Addressing::Path,
            bucket: String::new(),
            host_key: None,
        },
        credentials: Credentials::Password(PasswordCredentials {
            username: "anna".to_owned(),
            password: Zeroizing::new("placeholder-password".to_owned()),
        }),
    }
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

#[tokio::test]
async fn an_unknown_session_has_nothing() {
    let sessions = Sessions::default();
    assert_eq!(
        sessions.part("nope", b"x".to_vec(), soon()).await.err(),
        Some(StorageError::NotFound)
    );
    assert!(sessions.finish("nope", soon()).await.is_none());
    assert!(sessions.abort("nope").is_none());
}

#[tokio::test]
async fn a_refused_put_comes_back_with_its_temporary_file() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(507))
        .mount(&server)
        .await;
    let sessions = Sessions::default();
    let resolver: Arc<dyn crate::remote_storage::address::Resolver> = resolver(&[]);
    let id = sessions
        .begin(resolver, &access(&server), ".holzi-part-1".to_owned())
        .expect("begin");
    // The server may refuse before it read the body, then the part finds the stream closed.
    let _ = sessions.part(&id, b"x".to_vec(), soon()).await;
    let (part, outcome) = sessions.finish(&id, soon()).await.expect("known");
    assert_eq!(part, ".holzi-part-1");
    assert!(
        matches!(
            outcome,
            Err(StorageError::QuotaExceeded) | Err(StorageError::Network)
        ),
        "{outcome:?}"
    );
}

/// An address whose server takes the connection and then reads nothing.
async fn deaf_server() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("address").port();
    (listener, format!("http://127.0.0.1:{port}"))
}

#[tokio::test]
async fn a_part_the_server_does_not_take_in_time_breaks_the_session() {
    let (listener, uri) = deaf_server().await;
    let held = tokio::spawn(async move {
        let mut open = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            open.push(stream);
        }
    });
    let mut access = access_at(&uri);
    access.location.bucket = String::new();
    let sessions = Sessions::default();
    let id = sessions
        .begin(resolver(&[]), &access, ".holzi-part-2".to_owned())
        .expect("begin");
    let mut outcome = Ok(());
    for _ in 0..64 {
        let deadline = Instant::now() + Duration::from_millis(300);
        outcome = sessions.part(&id, vec![0; 4 << 20], deadline).await;
        if outcome.is_err() {
            break;
        }
    }
    assert_eq!(outcome.err(), Some(StorageError::TimedOut), "FR-013");
    assert_eq!(
        sessions.part(&id, b"x".to_vec(), soon()).await.err(),
        Some(StorageError::Network),
        "a broken session takes no more parts"
    );
    let (_, finished) = sessions.finish(&id, soon()).await.expect("known");
    assert_eq!(finished.err(), Some(StorageError::Network));
    held.abort();
}

#[tokio::test]
async fn an_answer_that_does_not_come_in_time_is_timed_out() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(201).set_delay(Duration::from_secs(30)))
        .mount(&server)
        .await;
    let sessions = Sessions::default();
    let id = sessions
        .begin(resolver(&[]), &access(&server), ".holzi-part-3".to_owned())
        .expect("begin");
    sessions
        .part(&id, b"x".to_vec(), soon())
        .await
        .expect("part");
    let started = std::time::Instant::now();
    let deadline = Instant::now() + Duration::from_millis(300);
    let (_, outcome) = sessions.finish(&id, deadline).await.expect("known");
    assert_eq!(outcome.err(), Some(StorageError::TimedOut), "FR-013");
    assert!(started.elapsed() < Duration::from_secs(5));
}
