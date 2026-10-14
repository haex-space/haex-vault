//! The file browser's S3 calls (spec 044, T061) against `wiremock`: `HEAD`, a range read that
//! streams, a listing of one level with `delimiter=/` across pages, and a server-side copy. Each
//! request is signed, with the headers it carries among the signed ones.

use std::io;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncReadExt;
use tokio::time::Instant;
use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use zeroize::Zeroizing;

use super::address::Resolver;
use super::s3::{copy_source, S3Store};
use super::{
    Access, Addressing, Credentials, EndpointScope, Location, ObjectStore, Protocol, ProviderKind,
    S3Credentials, StorageError,
};

const BUCKET: &str = "holzi-test";

struct Loopback;

#[async_trait]
impl Resolver for Loopback {
    async fn lookup(&self, _host: &str, _port: u16) -> io::Result<Vec<IpAddr>> {
        Ok(vec![IpAddr::from([127, 0, 0, 1])])
    }
}

fn store() -> S3Store {
    S3Store::new(Arc::new(Loopback))
}

fn access(server: &MockServer) -> Access {
    Access {
        location: Location {
            protocol: Protocol::S3,
            provider_kind: ProviderKind::Rustfs,
            endpoint: server.uri(),
            endpoint_scope: EndpointScope::Local,
            region: "us-east-1".to_owned(),
            addressing: Addressing::Path,
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

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

/// The names the signature covers (`X-Amz-SignedHeaders`).
fn signed_headers(request: &Request) -> String {
    request
        .url
        .query_pairs()
        .find(|(name, _)| name == "X-Amz-SignedHeaders")
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default()
}

#[tokio::test]
async fn head_tells_size_and_time() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .and(path(format!("/{BUCKET}/Fotos/a.jpg")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-length", "1234")
                .insert_header("last-modified", "Tue, 06 Oct 2026 10:00:00 GMT"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let head = store()
        .head(&access(&server), "Fotos/a.jpg", soon())
        .await
        .expect("head");
    assert_eq!(head.size, 1234);
    assert_eq!(
        head.last_modified.as_deref(),
        Some("Tue, 06 Oct 2026 10:00:00 GMT")
    );
}

#[tokio::test]
async fn a_range_read_sends_a_signed_range_and_streams_the_part() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/{BUCKET}/film.mp4")))
        .and(header("range", "bytes=10-19"))
        .respond_with(ResponseTemplate::new(206).set_body_bytes(b"0123456789".to_vec()))
        .expect(1)
        .mount(&server)
        .await;
    let mut reader = store()
        .get_range(&access(&server), "film.mp4", 10, 10, soon())
        .await
        .expect("range");
    let mut body = Vec::new();
    reader.read_to_end(&mut body).await.expect("read");
    assert_eq!(body, b"0123456789");
    let requests = server.received_requests().await.expect("recorded");
    assert_eq!(signed_headers(&requests[0]), "host;range");
}

#[tokio::test]
async fn a_whole_object_for_a_range_from_the_middle_is_refused() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 100]))
        .mount(&server)
        .await;
    let result = store()
        .get_range(&access(&server), "film.mp4", 10, 10, soon())
        .await;
    assert!(matches!(result, Err(StorageError::Network)));
}

#[tokio::test]
async fn an_empty_range_asks_nothing() {
    let server = MockServer::start().await;
    let mut reader = store()
        .get_range(&access(&server), "leer.txt", 0, 0, soon())
        .await
        .expect("range");
    let mut body = Vec::new();
    reader.read_to_end(&mut body).await.expect("read");
    assert!(body.is_empty());
    assert!(server
        .received_requests()
        .await
        .expect("recorded")
        .is_empty());
}

fn page(contents: &[(&str, u64)], prefixes: &[&str], next: Option<&str>) -> String {
    let objects: String = contents
        .iter()
        .map(|(key, size)| {
            format!(
                "<Contents><Key>{key}</Key><LastModified>2026-10-06T10:00:00.000Z</LastModified>\
                 <ETag>\"x\"</ETag><Size>{size}</Size><StorageClass>STANDARD</StorageClass></Contents>"
            )
        })
        .collect();
    let common: String = prefixes
        .iter()
        .map(|prefix| format!("<CommonPrefixes><Prefix>{prefix}</Prefix></CommonPrefixes>"))
        .collect();
    let token = next.map_or_else(String::new, |next| {
        format!("<NextContinuationToken>{next}</NextContinuationToken>")
    });
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
         <Name>{BUCKET}</Name><Prefix>Fotos/</Prefix><KeyCount>3</KeyCount><MaxKeys>1000</MaxKeys>\
         <Delimiter>/</Delimiter><IsTruncated>{}</IsTruncated>{objects}{common}{token}</ListBucketResult>",
        next.is_some()
    )
}

#[tokio::test]
async fn one_level_lists_objects_and_prefixes_across_pages() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/{BUCKET}/")))
        .and(query_param("delimiter", "/"))
        .and(query_param("prefix", "Fotos/"))
        .and(query_param_is_missing("continuation-token"))
        .respond_with(ResponseTemplate::new(200).set_body_string(page(
            &[("Fotos/", 0), ("Fotos/a.jpg", 10)],
            &["Fotos/2025/"],
            Some("weiter"),
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/{BUCKET}/")))
        .and(query_param("continuation-token", "weiter"))
        .respond_with(ResponseTemplate::new(200).set_body_string(page(
            &[("Fotos/b.jpg", 20)],
            &["Fotos/2026/"],
            None,
        )))
        .expect(1)
        .mount(&server)
        .await;
    let listing = store()
        .list_dir(&access(&server), "Fotos/", 100, soon())
        .await
        .expect("list");
    let keys: Vec<_> = listing.objects.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(
        keys,
        ["Fotos/a.jpg", "Fotos/b.jpg"],
        "the marker is left out"
    );
    assert_eq!(listing.prefixes, ["Fotos/2025/", "Fotos/2026/"]);
}

#[tokio::test]
async fn a_level_larger_than_the_limit_is_too_large() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(page(
            &[("Fotos/a.jpg", 1), ("Fotos/b.jpg", 1)],
            &["Fotos/x/"],
            None,
        )))
        .mount(&server)
        .await;
    let result = store()
        .list_dir(&access(&server), "Fotos/", 2, soon())
        .await;
    assert_eq!(result, Err(StorageError::TooLarge));
}

#[tokio::test]
async fn a_copy_is_a_put_with_a_signed_copy_source() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(format!("/{BUCKET}/neu/b.txt")))
        .and(header(
            "x-amz-copy-source",
            format!("/{BUCKET}/alt/Urlaub%202026.txt").as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("<CopyObjectResult/>"))
        .expect(1)
        .mount(&server)
        .await;
    store()
        .copy(&access(&server), "alt/Urlaub 2026.txt", "neu/b.txt", soon())
        .await
        .expect("copy");
    let requests = server.received_requests().await.expect("recorded");
    assert_eq!(signed_headers(&requests[0]), "host;x-amz-copy-source");
    assert!(requests[0].body.is_empty());
}

#[test]
fn the_copy_source_keeps_slashes_and_encodes_the_rest() {
    assert_eq!(
        copy_source("b", "a/ä ö+?.txt"),
        "/b/a/%C3%A4%20%C3%B6%2B%3F.txt"
    );
}

#[tokio::test]
async fn a_missing_object_is_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let result = store().head(&access(&server), "weg.txt", soon()).await;
    assert_eq!(result, Err(StorageError::NotFound));
}

#[tokio::test]
async fn an_upload_in_parts_creates_uploads_completes_and_aborts() {
    let server = MockServer::start().await;
    let object = format!("/{BUCKET}/gross.bin");
    Mock::given(method("POST"))
        .and(path(object.as_str()))
        // `?uploads` has no value; complete differs by its `uploadId`.
        .and(query_param_is_missing("uploadId"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <InitiateMultipartUploadResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <Bucket>{BUCKET}</Bucket><Key>gross.bin</Key><UploadId>upload-1</UploadId>\
             </InitiateMultipartUploadResult>"
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(object.as_str()))
        .and(query_param("partNumber", "1"))
        .and(query_param("uploadId", "upload-1"))
        .respond_with(ResponseTemplate::new(200).insert_header("etag", "\"teil-1\""))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(object.as_str()))
        .and(query_param("uploadId", "upload-1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("<CompleteMultipartUploadResult/>"),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(object.as_str()))
        .and(query_param("uploadId", "upload-2"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let store = store();
    let access = access(&server);
    let id = store
        .begin_upload(&access, "gross.bin", soon())
        .await
        .expect("create");
    assert_eq!(id, "upload-1");
    let etag = store
        .upload_part(&access, "gross.bin", &id, 1, b"teil".to_vec(), soon())
        .await
        .expect("part");
    assert_eq!(etag, "\"teil-1\"");
    store
        .finish_upload(&access, "gross.bin", &id, &[etag], soon())
        .await
        .expect("complete");
    store
        .abort_upload(&access, "gross.bin", "upload-2", soon())
        .await
        .expect("abort");

    let requests = server.received_requests().await.expect("recorded");
    let completed = requests
        .iter()
        .find(|r| {
            r.method.as_str() == "POST" && r.url.query().is_some_and(|q| q.contains("uploadId"))
        })
        .expect("the complete request");
    let body = String::from_utf8_lossy(&completed.body);
    assert!(body.contains("<PartNumber>1</PartNumber>"), "{body}");
    assert!(body.contains("teil-1"), "{body}");
}

#[tokio::test]
async fn a_part_without_an_etag_fails() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let result = store()
        .upload_part(&access(&server), "a", "u", 1, Vec::new(), soon())
        .await;
    assert_eq!(result, Err(StorageError::Network));
}
