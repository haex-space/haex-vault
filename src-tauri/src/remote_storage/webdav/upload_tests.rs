//! Upload sessions of WebDAV (spec 050 research R7): an unknown session, and a `PUT` the server
//! refuses, which comes back with the temporary file to delete.

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
    Access {
        location: Location {
            protocol: Protocol::WebDav,
            provider_kind: ProviderKind::Other,
            endpoint: format!("{}/", server.uri()),
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
