//! The pinned client of the storages (spec 050 research R2): it follows no redirect, and an address
//! holzi never reaches gets no client at all.

use std::sync::mpsc;
use std::time::Duration;

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

#[tokio::test]
async fn a_kept_client_never_spares_the_check_of_a_call() {
    // A public name that later points into the local network (DNS rebinding) is refused although a
    // client for the name is kept from the first call.
    let url = Url::parse("https://rebind.example/").expect("url");
    let first = resolver(&[("rebind.example", &["203.0.113.9"])]);
    pinned_client(&url, EndpointScope::Public, first.as_ref())
        .await
        .expect("a public address");
    let rebound = resolver(&[("rebind.example", &["10.0.0.1"])]);
    let refused = pinned_client(&url, EndpointScope::Public, rebound.as_ref()).await;
    assert_eq!(refused.err(), Some(StorageError::Network));
}

#[test]
fn a_client_kept_on_one_runtime_does_not_serve_another() {
    // A kept client's idle connections run on the runtime that opened them. Each test has a
    // runtime of its own: a call from another runtime that took such a connection broke off
    // ("runtime dropped the dispatch task") when the first one ended under it.
    let servers = tokio::runtime::Runtime::new().expect("runtime");
    let server = servers.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(800)))
            .mount(&server)
            .await;
        server
    });
    let url = Url::parse(&server.uri()).expect("url");

    let (opened, connection_kept) = mpsc::channel();
    let first = std::thread::spawn({
        let url = url.clone();
        move || {
            let runtime = tokio::runtime::Runtime::new().expect("runtime");
            runtime.block_on(async {
                let client = pinned_client(&url, EndpointScope::Local, resolver(&[]).as_ref())
                    .await
                    .expect("a local endpoint of the user");
                let answer = client.get(url).send().await.expect("an answer");
                assert_eq!(answer.status(), StatusCode::OK);
            });
            opened.send(()).expect("signal");
            std::thread::sleep(Duration::from_millis(300));
        }
    });
    connection_kept.recv().expect("the first call");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let answer = runtime.block_on(async {
        let client = pinned_client(&url, EndpointScope::Local, resolver(&[]).as_ref())
            .await
            .expect("a local endpoint of the user");
        client.put(url).body(b"hello".to_vec()).send().await
    });
    first.join().expect("the first runtime");
    assert_eq!(answer.expect("an answer").status(), StatusCode::OK);
}
