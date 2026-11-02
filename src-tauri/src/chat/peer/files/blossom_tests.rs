//! The Blossom client against the test server (spec 072 T058): asking before an upload with the
//! server's limit and reason, the upload's authorization, the descriptor check, a gone blob, and
//! deleting only with the uploading key. Plus the address rules: no plain `http` to other hosts,
//! no local host from a message.

use nostr::key::Keys;

use super::*;
use crate::chat::peer::files::crypto::{ciphertext_len, MAX_PLAINTEXT};
use crate::chat::peer::files::test_server::BlossomTestServer;
use crate::remote_storage::address::SystemResolver;

async fn server() -> (BlossomTestServer, Server) {
    let test = BlossomTestServer::start().await;
    let server = Server::new(&test.url, &SystemResolver)
        .await
        .expect("the test server is allowed");
    (test, server)
}

fn sha(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

#[test]
fn uploads_respect_the_hxcf_ciphertext_cap() {
    let max = ciphertext_len(MAX_PLAINTEXT);
    assert_eq!(check_upload_size(max), Ok(()));
    assert_eq!(check_upload_size(max + 1), Err(BlossomError::TooLarge));
}

#[tokio::test]
async fn an_upload_is_asked_first_stored_and_downloaded_by_its_hash() {
    let (test, server) = server().await;
    let keys = Keys::generate();
    let blob = b"encrypted bytes".to_vec();
    let hash = sha(&blob);

    server
        .check_upload(&keys, &hash, blob.len() as u64, &SystemResolver)
        .await
        .expect("the server takes it");
    let descriptor = server
        .upload(&keys, blob.clone(), &SystemResolver)
        .await
        .expect("upload");
    assert_eq!(descriptor.sha256, hash);
    assert_eq!(descriptor.size, blob.len() as u64);
    assert!(test.has(&hash));

    let mut seen = 0;
    let got = download(
        &descriptor.url,
        &hash,
        1024,
        Some(&server),
        &SystemResolver,
        &mut |bytes| seen = bytes,
    )
    .await
    .expect("download");
    assert_eq!(got, blob);
    assert_eq!(seen, blob.len() as u64, "progress reports the bytes");

    let put = test
        .log()
        .into_iter()
        .find(|logged| logged.method == "PUT")
        .expect("the upload");
    assert_eq!(put.path, "/upload");
    assert_eq!(put.body, blob, "the body is the blob as given");
    let auth = put
        .headers
        .iter()
        .find(|(name, _)| name == "authorization")
        .map(|(_, value)| value.clone())
        .expect("authorization");
    let json = URL_SAFE_NO_PAD
        .decode(auth.strip_prefix("Nostr ").expect("Nostr scheme"))
        .expect("base64url");
    let event: serde_json::Value = serde_json::from_slice(&json).expect("event");
    assert_eq!(event["kind"], AUTH_KIND);
    assert_eq!(event["pubkey"], keys.public_key().to_hex());
    let tags = event["tags"].as_array().expect("tags");
    assert!(tags.contains(&serde_json::json!(["t", "upload"])));
    assert!(tags.contains(&serde_json::json!(["x", hash])));
    assert!(tags.iter().any(|tag| tag[0] == "expiration"));
}

#[tokio::test]
async fn the_servers_limit_and_reason_refuse_before_and_at_the_upload() {
    let (test, server) = server().await;
    test.limit(10, "free accounts take 10 bytes");
    let keys = Keys::generate();
    let blob = vec![1; 11];
    let refusal = server
        .check_upload(&keys, &sha(&blob), 11, &SystemResolver)
        .await
        .expect_err("over the limit");
    assert_eq!(
        refusal,
        BlossomError::Refused {
            status: 413,
            reason: Some("free accounts take 10 bytes".into())
        }
    );
    assert_eq!(refusal.code(), "blossom:413");
    assert!(matches!(
        server.upload(&keys, blob, &SystemResolver).await,
        Err(BlossomError::Refused { status: 413, .. })
    ));
}

#[tokio::test]
async fn only_the_uploading_key_deletes_and_a_deleted_blob_is_gone() {
    let (test, server) = server().await;
    let uploader = Keys::generate();

    for invalid in [
        "https://attacker.example/blob",
        "//attacker.example/blob",
        "../outside",
        "not-a-sha256",
    ] {
        assert_eq!(
            server.delete(&uploader, invalid, &SystemResolver).await,
            Err(BlossomError::NotAllowed),
            "delete refuses {invalid:?} before constructing a URL"
        );
    }

    let blob = b"to be deleted".to_vec();
    let descriptor = server
        .upload(&uploader, blob.clone(), &SystemResolver)
        .await
        .expect("upload");

    let stranger = Keys::generate();
    assert!(matches!(
        server
            .delete(&stranger, &descriptor.sha256, &SystemResolver)
            .await,
        Err(BlossomError::Refused { status: 403, .. })
    ));
    assert!(test.has(&descriptor.sha256));

    server
        .delete(&uploader, &descriptor.sha256, &SystemResolver)
        .await
        .expect("delete");
    assert!(!test.has(&descriptor.sha256));
    server
        .delete(&uploader, &descriptor.sha256, &SystemResolver)
        .await
        .expect("a gone blob is no failure");
    let gone = download(
        &descriptor.url,
        &descriptor.sha256,
        1024,
        Some(&server),
        &SystemResolver,
        &mut |_| {},
    )
    .await;
    assert_eq!(gone, Err(BlossomError::Gone));
}

#[tokio::test]
async fn a_download_checks_the_hash_and_the_size() {
    let (_test, server) = server().await;
    let keys = Keys::generate();
    let blob = vec![7; 100];
    let descriptor = server
        .upload(&keys, blob, &SystemResolver)
        .await
        .expect("upload");
    let too_small = download(
        &descriptor.url,
        &descriptor.sha256,
        99,
        Some(&server),
        &SystemResolver,
        &mut |_| {},
    )
    .await;
    assert_eq!(too_small, Err(BlossomError::TooLarge));

    // An address whose name is not the hash the message named is not fetched at all.
    let other = sha(b"another file");
    let wrong = download(
        &descriptor.url,
        &other,
        1024,
        Some(&server),
        &SystemResolver,
        &mut |_| {},
    )
    .await;
    assert_eq!(wrong, Err(BlossomError::NotAllowed));
}

#[tokio::test]
async fn addresses_must_be_https_and_a_message_cannot_reach_a_local_host() {
    for address in [
        "http://example.org/",
        "ftp://127.0.0.1/",
        "http://localhost:1/",
        "https://user:pass@example.org/",
        "https://example.org/?x=1",
    ] {
        assert_eq!(
            Server::new(address, &SystemResolver).await,
            Err(BlossomError::NotAllowed),
            "{address}"
        );
    }

    // The test server is local: a download from it without it being the own server is refused.
    let (_test, server) = server().await;
    let keys = Keys::generate();
    let descriptor = server
        .upload(&keys, b"x".to_vec(), &SystemResolver)
        .await
        .expect("upload");
    let refused = download(
        &descriptor.url,
        &descriptor.sha256,
        1024,
        None,
        &SystemResolver,
        &mut |_| {},
    )
    .await;
    assert_eq!(refused, Err(BlossomError::NotAllowed));
}
