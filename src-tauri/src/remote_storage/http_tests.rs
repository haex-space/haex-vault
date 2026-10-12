//! The pinned client of the storages (spec 050 research R2): it follows no redirect, and an address
//! holzi never reaches gets no client at all.

use reqwest::{StatusCode, Url};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::http::pinned_client;
use super::test_support::resolver;
use super::{EndpointScope, StorageError};

#[tokio::test]
async fn a_redirect_is_not_followed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", "http://192.0.2.1/elsewhere"),
        )
        .mount(&server)
        .await;
    let url = Url::parse(&server.uri()).expect("url");
    let client = pinned_client(&url, EndpointScope::Local, resolver(&[]).as_ref())
        .await
        .expect("a local endpoint of the user");
    let answer = client.get(url).send().await.expect("an answer");
    assert_eq!(answer.status(), StatusCode::FOUND);
}

#[tokio::test]
async fn an_address_holzi_never_reaches_gets_no_client() {
    let url = Url::parse("http://169.254.169.254/").expect("url");
    let refused = pinned_client(&url, EndpointScope::Local, resolver(&[]).as_ref()).await;
    assert_eq!(refused.err(), Some(StorageError::Network));
    let public = Url::parse("http://storage.example/").expect("url");
    let names = resolver(&[("storage.example", &["203.0.113.7"])]);
    let refused = pinned_client(&public, EndpointScope::Public, names.as_ref()).await;
    assert_eq!(
        refused.err(),
        Some(StorageError::Network),
        "http to a public host is refused"
    );
}
