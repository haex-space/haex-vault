//! The S3 implementation against `wiremock` (research R10): signed requests, answers read, errors
//! mapped, limits and the deadline enforced while streaming, no redirect followed, and the request
//! sent to the checked address of a host name.

use std::io;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use reqwest::StatusCode;
use tokio::time::Instant;
use wiremock::matchers::{body_bytes, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use zeroize::Zeroizing;

use super::address::Resolver;
use super::s3::{classify, S3Store};
use super::{
    Access, Addressing, Credentials, EndpointScope, Location, ObjectStore, Protocol, ProviderKind,
    S3Credentials, StorageError,
};

const BUCKET: &str = "holzi-test";

fn access(endpoint: &str, addressing: Addressing, scope: EndpointScope) -> Access {
    Access {
        location: Location {
            protocol: Protocol::S3,
            provider_kind: ProviderKind::Rustfs,
            endpoint: endpoint.to_owned(),
            endpoint_scope: scope,
            region: "us-east-1".to_owned(),
            addressing,
            bucket: BUCKET.to_owned(),
            host_key: None,
        },
        credentials: Credentials::S3(S3Credentials {
            access_key_id: "AKIDEXAMPLE".to_owned(),
            secret_access_key: Zeroizing::new("placeholder-secret".to_owned()),
            session_token: None,
        }),
    }
}

fn user(server: &MockServer) -> Access {
    access(&server.uri(), Addressing::Path, EndpointScope::Local)
}

/// Resolves every name to 127.0.0.1 and remembers the names.
#[derive(Default)]
struct Loopback {
    asked: Mutex<Vec<String>>,
}

#[async_trait]
impl Resolver for Loopback {
    async fn lookup(&self, host: &str, _port: u16) -> io::Result<Vec<IpAddr>> {
        self.asked.lock().expect("lock").push(host.to_owned());
        Ok(vec![IpAddr::from([127, 0, 0, 1])])
    }
}

fn store() -> S3Store {
    S3Store::new(Arc::new(Loopback::default()))
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

fn generous() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

fn signed(request: &Request) -> bool {
    request
        .url
        .query_pairs()
        .any(|(name, _)| name == "X-Amz-Signature")
}

#[tokio::test]
async fn put_get_and_delete_send_signed_requests_to_the_object() {
    let server = MockServer::start().await;
    let key = format!("/{BUCKET}/a/b.txt");
    Mock::given(method("PUT"))
        .and(path(key.as_str()))
        .and(body_bytes(b"hello".to_vec()))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(key.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello".to_vec()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(key.as_str()))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let store = store();
    let access = user(&server);
    store
        .put(&access, "a/b.txt", b"hello".to_vec(), generous())
        .await
        .expect("put");
    let body = store
        .get(&access, "a/b.txt", 100, generous())
        .await
        .expect("get");
    assert_eq!(body, b"hello");
    store
        .delete(&access, "a/b.txt", generous())
        .await
        .expect("delete");

    let requests = server.received_requests().await.expect("recorded");
    assert!(requests.iter().all(signed), "every request is signed");
}

#[tokio::test]
async fn a_download_over_the_limit_is_too_large() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7u8; 1024]))
        .mount(&server)
        .await;
    assert_eq!(
        store().get(&user(&server), "big", 1023, soon()).await,
        Err(StorageError::TooLarge)
    );
}

fn page(keys: &[&str], next: Option<&str>) -> String {
    let contents: String = keys
        .iter()
        .map(|k| {
            format!(
                "<Contents><Key>{k}</Key><LastModified>2026-10-06T10:00:00.000Z</LastModified>\
                 <ETag>\"x\"</ETag><Size>5</Size><StorageClass>STANDARD</StorageClass></Contents>"
            )
        })
        .collect();
    let token = next
        .map(|t| format!("<NextContinuationToken>{t}</NextContinuationToken>"))
        .unwrap_or_default();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Name>{BUCKET}</Name><Prefix>p/</Prefix><KeyCount>{}</KeyCount>\
         <MaxKeys>1000</MaxKeys><IsTruncated>{}</IsTruncated>{contents}{token}</ListBucketResult>",
        keys.len(),
        next.is_some()
    )
}

async fn two_pages(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path(format!("/{BUCKET}/")))
        .and(query_param("list-type", "2"))
        .and(query_param("prefix", "p/"))
        .and(query_param_is_missing("continuation-token"))
        .respond_with(ResponseTemplate::new(200).set_body_string(page(&["p/1", "p/2"], Some("t1"))))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/{BUCKET}/")))
        .and(query_param("continuation-token", "t1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(page(&["p/3"], None)))
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_listing_follows_the_pages_of_the_provider() {
    let server = MockServer::start().await;
    two_pages(&server).await;
    let objects = store()
        .list(&user(&server), "p/", 10, soon())
        .await
        .expect("list");
    let keys: Vec<_> = objects.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(keys, ["p/1", "p/2", "p/3"]);
    assert_eq!(objects[0].size, 5);
}

#[tokio::test]
async fn a_listing_with_more_objects_than_allowed_is_too_large() {
    let server = MockServer::start().await;
    two_pages(&server).await;
    assert_eq!(
        store().list(&user(&server), "p/", 2, soon()).await,
        Err(StorageError::TooLarge)
    );
}

fn error(status: u16, code: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_string(format!(
        "<?xml version=\"1.0\"?><Error><Code>{code}</Code>\
         <Message>secret details of the provider</Message></Error>"
    ))
}

#[tokio::test]
async fn errors_of_the_provider_map_to_kinds_without_its_text() {
    let cases = [
        (error(404, "NoSuchKey"), StorageError::NotFound),
        (error(404, "NoSuchBucket"), StorageError::NotFound),
        (
            error(403, "SignatureDoesNotMatch"),
            StorageError::AccessDenied,
        ),
        (error(403, "InvalidAccessKeyId"), StorageError::AccessDenied),
        (error(400, "ExpiredToken"), StorageError::AccessDenied),
        (error(403, "AccessDenied"), StorageError::MissingRight),
        (ResponseTemplate::new(401), StorageError::AccessDenied),
        (error(500, "InternalError"), StorageError::Network),
        (ResponseTemplate::new(503), StorageError::Network),
    ];
    for (answer, expected) in cases {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(answer)
            .mount(&server)
            .await;
        let got = store().get(&user(&server), "k", 100, soon()).await;
        assert_eq!(got, Err(expected));
        assert!(!format!("{:?}", got).contains("secret details"));
    }
}

#[test]
fn a_status_without_a_code_maps_by_itself() {
    assert_eq!(
        classify(StatusCode::NOT_FOUND, None),
        StorageError::NotFound
    );
    assert_eq!(
        classify(StatusCode::FORBIDDEN, None),
        StorageError::AccessDenied
    );
    assert_eq!(
        classify(StatusCode::BAD_GATEWAY, None),
        StorageError::Network
    );
}

#[tokio::test]
async fn a_redirect_is_not_followed() {
    let server = MockServer::start().await;
    let elsewhere = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(301).insert_header("Location", format!("{}/x", elsewhere.uri())),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&elsewhere)
        .await;
    assert_eq!(
        store().get(&user(&server), "k", 100, soon()).await,
        Err(StorageError::Network)
    );
}

#[tokio::test]
async fn a_slow_provider_runs_into_the_deadline() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;
    let deadline = Instant::now() + Duration::from_millis(200);
    assert_eq!(
        store().get(&user(&server), "k", 100, deadline).await,
        Err(StorageError::TimedOut)
    );
}

#[tokio::test]
async fn a_host_name_is_reached_at_its_checked_address() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"ok".to_vec()))
        .mount(&server)
        .await;
    let port = server.address().port();
    let resolver = Arc::new(Loopback::default());
    let store = S3Store::new(resolver.clone());

    let virtual_hosted = access(
        &format!("http://s3.storage.test:{port}"),
        Addressing::Virtual,
        EndpointScope::Local,
    );
    let body = store
        .get(&virtual_hosted, "k", 100, soon())
        .await
        .expect("reached through the pinned address");
    assert_eq!(body, b"ok");
    assert_eq!(
        resolver.asked.lock().expect("lock").as_slice(),
        [format!("{BUCKET}.s3.storage.test")],
        "the host of the request itself is checked"
    );
    let request = &server.received_requests().await.expect("recorded")[0];
    assert_eq!(
        request.headers.get("host").and_then(|h| h.to_str().ok()),
        Some(format!("{BUCKET}.s3.storage.test:{port}").as_str()),
        "the signed host name stays in the request"
    );

    let public = access(
        &format!("https://s3.storage.test:{port}"),
        Addressing::Path,
        EndpointScope::Public,
    );
    assert_eq!(
        store.get(&public, "k", 100, soon()).await,
        Err(StorageError::Network),
        "a public endpoint whose name now resolves to a local address is refused (DNS rebinding)"
    );
}

/// Never answers.
struct Silent;

#[async_trait]
impl Resolver for Silent {
    async fn lookup(&self, _host: &str, _port: u16) -> io::Result<Vec<IpAddr>> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn a_lookup_that_does_not_answer_ends_at_the_deadline() {
    let store = S3Store::new(Arc::new(Silent));
    let named = access(
        "http://rustfs.storage.test:9000",
        Addressing::Path,
        EndpointScope::Local,
    );
    let deadline = Instant::now() + Duration::from_millis(200);
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        store.get(&named, "k", 100, deadline),
    )
    .await
    .expect("the call ends at its deadline, not later");
    assert_eq!(result, Err(StorageError::TimedOut));
}
