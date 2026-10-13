//! The WebDAV store against a mock server (spec 050 US1, research R2, R7, contracts/remote-store.md):
//! files go through a temporary file and a `MOVE`, missing folders below the storage's folder are
//! made once, a missing folder of the storage is not, ranges and listings read as on S3, and an
//! upload in parts streams one `PUT`.

use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::time::Instant;
use wiremock::matchers::{header, method, path, path_regex};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use zeroize::Zeroizing;

use super::WebDavStore;
use crate::remote_storage::test_support::resolver;
use crate::remote_storage::{
    Access, Addressing, Credentials, EndpointScope, Location, PasswordCredentials, Protocol,
    ProviderKind, RemoteStore, StorageError,
};

const PART: &str = r"^/dav/Fotos/(sub/)?\.holzi-part-[0-9a-f-]+$";

fn access(server: &MockServer) -> Access {
    Access {
        location: Location {
            protocol: Protocol::WebDav,
            provider_kind: ProviderKind::Other,
            endpoint: format!("{}/dav/", server.uri()),
            endpoint_scope: EndpointScope::Local,
            region: String::new(),
            addressing: Addressing::Path,
            bucket: "Fotos".to_owned(),
            host_key: None,
        },
        credentials: Credentials::Password(PasswordCredentials {
            username: "anna".to_owned(),
            password: Zeroizing::new("placeholder-password".to_owned()),
        }),
    }
}

fn store() -> WebDavStore {
    WebDavStore::new(resolver(&[]))
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

async fn requests(server: &MockServer) -> Vec<Request> {
    server.received_requests().await.unwrap_or_default()
}

fn verbs(requests: &[Request]) -> Vec<String> {
    requests.iter().map(|r| r.method.to_string()).collect()
}

fn header_of(request: &Request, name: &str) -> String {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

#[tokio::test]
async fn a_file_goes_to_a_temporary_file_and_is_moved_onto_its_name() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path_regex(PART))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    Mock::given(method("MOVE"))
        .and(path_regex(PART))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    store()
        .put(&access(&server), "a b.txt", b"hallo".to_vec(), soon())
        .await
        .expect("put");
    let seen = requests(&server).await;
    assert_eq!(verbs(&seen), ["PUT", "MOVE"]);
    assert_eq!(seen[0].body, b"hallo");
    assert_eq!(seen[0].url.path(), seen[1].url.path(), "the same file");
    assert!(header_of(&seen[1], "destination").ends_with("/dav/Fotos/a%20b.txt"));
    assert_eq!(header_of(&seen[1], "overwrite"), "T");
}

#[tokio::test]
async fn a_missing_folder_below_the_storage_is_made_once() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path_regex(PART))
        .respond_with(ResponseTemplate::new(409))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path_regex(PART))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    Mock::given(method("MKCOL"))
        .and(path("/dav/Fotos/sub/"))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("MOVE"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    store()
        .put(&access(&server), "sub/c.txt", b"c".to_vec(), soon())
        .await
        .expect("put");
    assert_eq!(
        verbs(&requests(&server).await),
        ["PUT", "MKCOL", "PUT", "MOVE"]
    );
}

#[tokio::test]
async fn a_missing_folder_of_the_storage_itself_is_not_made() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(409))
        .mount(&server)
        .await;
    Mock::given(method("MKCOL"))
        .respond_with(ResponseTemplate::new(409))
        .mount(&server)
        .await;
    let result = store()
        .put(&access(&server), "sub/c.txt", b"c".to_vec(), soon())
        .await;
    assert_eq!(result.err(), Some(StorageError::NotFound));
    let seen = requests(&server).await;
    assert!(
        seen.iter()
            .all(|r| r.method.as_str() != "MKCOL" || r.url.path() == "/dav/Fotos/sub/"),
        "never a MKCOL of the storage's folder (FR-006)"
    );
}

#[tokio::test]
async fn a_failed_move_deletes_the_temporary_file() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    Mock::given(method("MOVE"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path_regex(PART))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let result = store()
        .put(&access(&server), "a.txt", b"a".to_vec(), soon())
        .await;
    assert_eq!(result.err(), Some(StorageError::Network));
}

const FOLDER: &str = r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:">
<d:response><d:href>/dav/Fotos/</d:href><d:propstat><d:prop>
<d:resourcetype><d:collection/></d:resourcetype></d:prop>
<d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
<d:response><d:href>/dav/Fotos/a.jpg</d:href><d:propstat><d:prop><d:resourcetype/>
<d:getcontentlength>5</d:getcontentlength><d:getetag>"e1"</d:getetag>
<d:getlastmodified>Mon, 12 Oct 2026 10:00:00 GMT</d:getlastmodified></d:prop>
<d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
<d:response><d:href>/dav/Fotos/Urlaub/</d:href><d:propstat><d:prop>
<d:resourcetype><d:collection/></d:resourcetype></d:prop>
<d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
<d:response><d:href>/dav/Fotos/.holzi-part-1234</d:href><d:propstat><d:prop><d:resourcetype/>
<d:getcontentlength>9</d:getcontentlength></d:prop>
<d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
</d:multistatus>"#;

#[tokio::test]
async fn one_level_lists_files_and_folders_without_temporary_files() {
    let server = MockServer::start().await;
    Mock::given(method("PROPFIND"))
        .and(path("/dav/Fotos/"))
        .and(header("depth", "1"))
        .respond_with(ResponseTemplate::new(207).set_body_string(FOLDER))
        .mount(&server)
        .await;
    let listing = store()
        .list_dir(&access(&server), "", 100, soon())
        .await
        .expect("list");
    assert_eq!(listing.prefixes, ["Urlaub/"]);
    let objects: Vec<(&str, u64, &str)> = listing
        .objects
        .iter()
        .map(|o| (o.key.as_str(), o.size, o.etag.as_str()))
        .collect();
    assert_eq!(objects, [("a.jpg", 5, "e1")]);
}

#[tokio::test]
async fn a_head_asks_with_depth_0_and_a_redirect_is_a_protocol_error() {
    let server = MockServer::start().await;
    Mock::given(method("PROPFIND"))
        .and(path("/dav/Fotos/"))
        .and(header("depth", "0"))
        .respond_with(ResponseTemplate::new(207).set_body_string(FOLDER))
        .mount(&server)
        .await;
    Mock::given(method("PROPFIND"))
        .and(path("/dav/Fotos/x.txt"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/login"))
        .mount(&server)
        .await;
    let store = store();
    let access = access(&server);
    let head = store.head(&access, "", soon()).await.expect("head");
    assert_eq!(head.size, 0);
    assert_eq!(
        store.head(&access, "x.txt", soon()).await.err(),
        Some(StorageError::Protocol)
    );
}

#[tokio::test]
async fn a_range_is_read_from_206_and_refused_from_a_whole_answer() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/dav/Fotos/film.mp4"))
        .and(header("range", "bytes=2-4"))
        .respond_with(ResponseTemplate::new(206).set_body_bytes(b"234".to_vec()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/dav/Fotos/whole.mp4"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"0123456".to_vec()))
        .mount(&server)
        .await;
    let store = store();
    let access = access(&server);
    let mut reader = store
        .get_range(&access, "film.mp4", 2, 3, soon())
        .await
        .expect("range");
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.expect("read");
    assert_eq!(bytes, b"234");
    assert_eq!(
        store
            .get_range(&access, "whole.mp4", 2, 3, soon())
            .await
            .err(),
        Some(StorageError::Network)
    );
}

#[tokio::test]
async fn an_upload_in_parts_streams_one_put_and_moves_it() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path_regex(PART))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    Mock::given(method("MOVE"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    let store = store();
    let access = access(&server);
    let id = store
        .begin_upload(&access, "gross.bin", soon())
        .await
        .expect("begin");
    let mut marks = Vec::new();
    for (number, part) in [(1u16, b"aaa".to_vec()), (2, b"bbb".to_vec())] {
        marks.push(
            store
                .upload_part(&access, "gross.bin", &id, number, part, soon())
                .await
                .expect("part"),
        );
    }
    store
        .finish_upload(&access, "gross.bin", &id, &marks, soon())
        .await
        .expect("finish");
    let seen = requests(&server).await;
    assert_eq!(verbs(&seen), ["PUT", "MOVE"]);
    assert_eq!(seen[0].body, b"aaabbb");
    assert!(header_of(&seen[1], "destination").ends_with("/dav/Fotos/gross.bin"));
}

#[tokio::test]
async fn a_cancelled_upload_deletes_its_temporary_file() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path_regex(PART))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let store = store();
    let access = access(&server);
    let id = store
        .begin_upload(&access, "gross.bin", soon())
        .await
        .expect("begin");
    store
        .abort_upload(&access, "gross.bin", &id, soon())
        .await
        .expect("abort");
    assert!(
        !verbs(&requests(&server).await).contains(&"MOVE".to_owned()),
        "nothing lands under the name"
    );
}

#[tokio::test]
async fn a_listing_whose_answer_does_not_come_in_time_is_timed_out() {
    let server = MockServer::start().await;
    Mock::given(method("PROPFIND"))
        .respond_with(
            ResponseTemplate::new(207)
                .set_body_string(FOLDER)
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&server)
        .await;
    let started = std::time::Instant::now();
    let deadline = Instant::now() + Duration::from_millis(300);
    let result = store().list_dir(&access(&server), "", 100, deadline).await;
    assert_eq!(result.err(), Some(StorageError::TimedOut), "FR-013");
    assert!(started.elapsed() < Duration::from_secs(5));
}

const OLD_PART: &str = r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:">
<d:response><d:href>/dav/Fotos/</d:href><d:propstat><d:prop>
<d:resourcetype><d:collection/></d:resourcetype></d:prop>
<d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
<d:response><d:href>/dav/Fotos/.holzi-part-alt</d:href><d:propstat><d:prop><d:resourcetype/>
<d:getcontentlength>9</d:getcontentlength>
<d:getlastmodified>Mon, 12 Oct 2020 10:00:00 GMT</d:getlastmodified></d:prop>
<d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
<d:response><d:href>/dav/Fotos/a.jpg</d:href><d:propstat><d:prop><d:resourcetype/>
<d:getcontentlength>5</d:getcontentlength>
<d:getlastmodified>Mon, 12 Oct 2026 10:00:00 GMT</d:getlastmodified></d:prop>
<d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
</d:multistatus>"#;

#[tokio::test]
async fn a_listing_deletes_an_old_temporary_file_and_ignores_a_failed_delete() {
    let server = MockServer::start().await;
    Mock::given(method("PROPFIND"))
        .respond_with(ResponseTemplate::new(207).set_body_string(OLD_PART))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/dav/Fotos/.holzi-part-alt"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&server)
        .await;
    let listing = store()
        .list_dir(&access(&server), "", 100, soon())
        .await
        .expect("a failed delete leaves the listing alone");
    let objects: Vec<(&str, &str)> = listing
        .objects
        .iter()
        .map(|o| (o.key.as_str(), o.last_modified.as_str()))
        .collect();
    assert_eq!(objects, [("a.jpg", "2026-10-12T10:00:00.000Z")]);
}
