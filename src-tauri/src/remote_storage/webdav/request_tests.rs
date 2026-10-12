//! Requests of the WebDAV client (spec 050 research R2): addresses below the folder of a storage,
//! Basic authentication only with a password login, no redirect followed, statuses as errors.

use std::time::Duration;

use reqwest::{Method, StatusCode};
use tokio::time::Instant;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use zeroize::Zeroizing;

use super::request::{send_raw, status_error, url_of};
use crate::remote_storage::test_support::{credentials, resolver};
use crate::remote_storage::{
    Access, Addressing, Credentials, EndpointScope, Location, PasswordCredentials, Protocol,
    ProviderKind, StorageError,
};

fn location(endpoint: &str, folder: &str) -> Location {
    Location {
        protocol: Protocol::WebDav,
        provider_kind: ProviderKind::Other,
        endpoint: endpoint.to_owned(),
        endpoint_scope: EndpointScope::Local,
        region: String::new(),
        addressing: Addressing::Path,
        bucket: folder.to_owned(),
        host_key: None,
    }
}

fn access(server: &MockServer) -> Access {
    Access {
        location: location(&format!("{}/dav/", server.uri()), ""),
        credentials: Credentials::Password(PasswordCredentials {
            username: "anna".to_owned(),
            password: Zeroizing::new("placeholder-password".to_owned()),
        }),
    }
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

#[test]
fn addresses_lie_below_the_folder_with_every_part_encoded() {
    let at = location("https://box.example/dav/", "Fotos/2026");
    let url = |key| url_of(&at, key).expect("url").to_string();
    assert_eq!(
        url("a b/c#d.txt"),
        "https://box.example/dav/Fotos/2026/a%20b/c%23d.txt"
    );
    assert_eq!(url("Urlaub/"), "https://box.example/dav/Fotos/2026/Urlaub/");
    assert_eq!(url(""), "https://box.example/dav/Fotos/2026/");
    let root = location("https://box.example/", "");
    assert_eq!(
        url_of(&root, "").expect("url").to_string(),
        "https://box.example/"
    );
    assert_eq!(
        url_of(&root, "../x").err(),
        Some(StorageError::NotFound),
        "a key never leads out of the folder"
    );
}

#[test]
fn statuses_become_the_errors_of_a_storage() {
    for (status, error) in [
        (401, StorageError::AccessDenied),
        (403, StorageError::MissingRight),
        (404, StorageError::NotFound),
        (409, StorageError::NotFound),
        (405, StorageError::Protocol),
        (507, StorageError::QuotaExceeded),
        (500, StorageError::Network),
        (429, StorageError::Network),
    ] {
        let status = StatusCode::from_u16(status).expect("status");
        assert_eq!(status_error(&Method::PUT, status), error, "{status}");
    }
}

#[tokio::test]
async fn a_request_signs_in_with_basic() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/dav/a.txt"))
        // base64("anna:placeholder-password")
        .and(header(
            "authorization",
            "Basic YW5uYTpwbGFjZWhvbGRlci1wYXNzd29yZA==",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("a"))
        .expect(1)
        .mount(&server)
        .await;
    let access = access(&server);
    let url = url_of(&access.location, "a.txt").expect("url");
    let answer = send_raw(
        resolver(&[]).as_ref(),
        &access,
        Method::GET,
        url,
        &[],
        None,
        soon(),
    )
    .await
    .expect("an answer");
    assert_eq!(answer.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_redirect_is_a_protocol_error_and_not_followed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", "/elsewhere/"))
        .expect(1)
        .mount(&server)
        .await;
    let access = access(&server);
    let url = url_of(&access.location, "a").expect("url");
    let answer = send_raw(
        resolver(&[]).as_ref(),
        &access,
        Method::GET,
        url,
        &[],
        None,
        soon(),
    )
    .await;
    assert_eq!(answer.err(), Some(StorageError::Protocol));
}

#[tokio::test]
async fn credentials_of_another_kind_send_nothing() {
    let server = MockServer::start().await;
    let mut access = access(&server);
    access.credentials = credentials();
    let url = url_of(&access.location, "a").expect("url");
    let answer = send_raw(
        resolver(&[]).as_ref(),
        &access,
        Method::GET,
        url,
        &[],
        None,
        soon(),
    )
    .await;
    assert_eq!(answer.err(), Some(StorageError::AccessDenied));
    assert!(server
        .received_requests()
        .await
        .unwrap_or_default()
        .is_empty());
}
