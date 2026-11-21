//! The Blossom client against the test server (spec 072 T058): asking before an upload with the
//! server's limit and reason, the upload's authorization, the descriptor check, a gone blob, and
//! deleting only with the uploading key. Plus the address rules: no plain `http` to other hosts,
//! no local host from a message.

use std::time::Instant as StdInstant;

use futures::stream;
use nostr::key::Keys;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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

async fn upload_blob(server: &Server, keys: &Keys, blob: Vec<u8>) -> Result<Descriptor> {
    server
        .upload(keys, UploadBody::whole(blob), &SystemResolver, &mut |_| {})
        .await
}

/// The whole blob at `url`, as the receiving side reads it: piece by piece to the verdict.
async fn fetch(url: &str, hash: &str, max: u64, own: Option<&Server>) -> Result<Vec<u8>> {
    let mut stream = download(url, hash, max, own, &SystemResolver).await?;
    let mut all = Vec::new();
    while let Some(piece) = stream.chunk().await? {
        all.extend(piece);
    }
    Ok(all)
}

/// A server on this machine that answers every `GET` with a head that declares `declared` bytes,
/// sends `body` and then stays silent (or hangs up when `hang_up`). Its own server and the address
/// of a blob named `hash`.
async fn answers_then_stays_silent(
    declared: usize,
    body: Vec<u8>,
    hang_up: bool,
    patience: Patience,
    hash: &str,
) -> (Server, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let body = body.clone();
            tokio::spawn(async move {
                let mut seen = Vec::new();
                let mut buf = [0; 1024];
                while !seen.ends_with(b"\r\n\r\n") {
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => seen.extend_from_slice(&buf[..n]),
                    }
                }
                let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {declared}\r\n\r\n");
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(&body).await;
                if !hang_up {
                    std::future::pending::<()>().await;
                }
            });
        }
    });
    let base = format!("http://127.0.0.1:{port}/");
    let server = Server::new(&base, &SystemResolver)
        .await
        .expect("a local server is allowed")
        .with_patience(patience);
    (server, format!("{base}{hash}"))
}

const QUICK: Patience = Patience {
    stall: Duration::from_millis(300),
    answer: Duration::from_secs(2),
};

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
    let descriptor = upload_blob(&server, &keys, blob.clone())
        .await
        .expect("upload");
    assert_eq!(descriptor.sha256, hash);
    assert_eq!(descriptor.size, blob.len() as u64);
    assert!(test.has(&hash));

    let mut stream = download(&descriptor.url, &hash, 1024, Some(&server), &SystemResolver)
        .await
        .expect("download");
    let mut got = Vec::new();
    while let Some(piece) = stream.chunk().await.expect("a piece") {
        got.extend(piece);
    }
    assert_eq!(got, blob);
    assert_eq!(
        stream.received(),
        blob.len() as u64,
        "the bytes are counted"
    );

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
        upload_blob(&server, &keys, blob).await,
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
    let descriptor = upload_blob(&server, &uploader, blob.clone())
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
    let gone = fetch(&descriptor.url, &descriptor.sha256, 1024, Some(&server)).await;
    assert_eq!(gone, Err(BlossomError::Gone));
}

#[tokio::test]
async fn a_download_checks_the_hash_and_the_size() {
    let (_test, server) = server().await;
    let keys = Keys::generate();
    let blob = vec![7; 100];
    let descriptor = upload_blob(&server, &keys, blob).await.expect("upload");
    let too_small = fetch(&descriptor.url, &descriptor.sha256, 99, Some(&server)).await;
    assert_eq!(too_small, Err(BlossomError::TooLarge));

    // An address whose name is not the hash the message named is not fetched at all.
    let other = sha(b"another file");
    let wrong = fetch(&descriptor.url, &other, 1024, Some(&server)).await;
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
    let descriptor = upload_blob(&server, &keys, b"x".to_vec())
        .await
        .expect("upload");
    let refused = fetch(&descriptor.url, &descriptor.sha256, 1024, None).await;
    assert_eq!(refused, Err(BlossomError::NotAllowed));
}

#[test]
fn the_total_allowance_grows_with_the_size() {
    assert_eq!(total_limit(0), TRANSFER);
    assert_eq!(
        total_limit(100 * MIN_RATE),
        TRANSFER + Duration::from_secs(100)
    );
    assert!(total_limit(ciphertext_len(MAX_PLAINTEXT)) > Duration::from_secs(50 * 60));
}

#[tokio::test]
async fn an_upload_that_stands_still_fails_and_one_that_moves_slowly_does_not() {
    let (_test, server) = server().await;
    let server = server.with_patience(QUICK);
    let keys = Keys::generate();

    // One piece, then nothing: the body never ends.
    let stalled = UploadBody {
        sha256: sha(&[1; 20]),
        size: 20,
        chunks: Box::pin(stream::once(async { Ok(vec![1; 10]) }).chain(stream::pending())),
    };
    let start = StdInstant::now();
    let result = server
        .upload(&keys, stalled, &SystemResolver, &mut |_| {})
        .await;
    assert_eq!(result, Err(BlossomError::TimedOut));
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );

    // Longer than the stall time in all, but never still for that long.
    let blob = vec![2; 20];
    let slow = UploadBody {
        sha256: sha(&blob),
        size: 20,
        chunks: Box::pin(stream::iter(vec![vec![2; 5]; 4]).then(|piece| async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            Ok(piece)
        })),
    };
    let mut heard = Vec::new();
    let descriptor = server
        .upload(&keys, slow, &SystemResolver, &mut |sent| heard.push(sent))
        .await
        .expect("a slow upload that keeps moving goes through");
    assert_eq!(descriptor.sha256, sha(&blob));
    assert!(heard.windows(2).all(|pair| pair[0] < pair[1]), "{heard:?}");
}

#[tokio::test]
async fn a_download_that_stands_still_fails_after_the_stall_time() {
    let hash = sha(b"whatever");
    let (server, url) = answers_then_stays_silent(20, vec![9; 5], false, QUICK, &hash).await;
    let mut stream = download(&url, &hash, 1024, Some(&server), &SystemResolver)
        .await
        .expect("the head comes");
    assert_eq!(
        stream.chunk().await.expect("the first piece"),
        Some(vec![9; 5])
    );
    let start = StdInstant::now();
    assert_eq!(stream.chunk().await, Err(BlossomError::TimedOut));
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn a_blob_that_is_not_the_one_named_ends_in_a_mismatch_after_its_last_byte() {
    let hash = sha(b"the file the message named");
    let (server, url) = answers_then_stays_silent(20, vec![9; 20], true, QUICK, &hash).await;
    let mut stream = download(&url, &hash, 1024, Some(&server), &SystemResolver)
        .await
        .expect("the head comes");
    let mut got = 0;
    let verdict = loop {
        match stream.chunk().await {
            Ok(Some(piece)) => got += piece.len(),
            other => break other,
        }
    };
    assert_eq!(got, 20, "the bytes arrive before the check can tell");
    assert_eq!(verdict, Err(BlossomError::Mismatch));
}

/// Runs `handler` for every connection to a server on this machine; answers its port.
async fn serve_raw<F, Fut>(handler: F) -> u16
where
    F: Fn(TcpStream) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(handler(socket));
        }
    });
    port
}

/// Reads what the client sends up to the end of its request head.
async fn read_head(socket: &mut TcpStream) -> bool {
    let mut seen = Vec::new();
    let mut buf = [0; 1024];
    while !seen.windows(4).any(|window| window == b"\r\n\r\n") {
        match socket.read(&mut buf).await {
            Ok(0) | Err(_) => return false,
            Ok(n) => seen.extend_from_slice(&buf[..n]),
        }
    }
    true
}

/// A server that reads every request head and never answers.
async fn never_answers(patience: Patience) -> Server {
    let port = serve_raw(|mut socket| async move {
        if read_head(&mut socket).await {
            std::future::pending::<()>().await;
        }
    })
    .await;
    Server::new(&format!("http://127.0.0.1:{port}/"), &SystemResolver)
        .await
        .expect("a local server is allowed")
        .with_patience(patience)
}

/// A server that answers at once with a head declaring all `pieces`, sends them `gap` apart and
/// hangs up. Its own server and the address of a blob named `hash`.
async fn trickles(
    pieces: Vec<Vec<u8>>,
    gap: Duration,
    patience: Patience,
    hash: &str,
) -> (Server, String) {
    let declared: usize = pieces.iter().map(Vec::len).sum();
    let port = serve_raw(move |mut socket| {
        let pieces = pieces.clone();
        async move {
            if !read_head(&mut socket).await {
                return;
            }
            let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {declared}\r\n\r\n");
            let _ = socket.write_all(head.as_bytes()).await;
            for piece in pieces {
                tokio::time::sleep(gap).await;
                if socket.write_all(&piece).await.is_err() {
                    return;
                }
            }
        }
    })
    .await;
    let base = format!("http://127.0.0.1:{port}/");
    let server = Server::new(&base, &SystemResolver)
        .await
        .expect("a local server is allowed")
        .with_patience(patience);
    (server, format!("{base}{hash}"))
}

#[tokio::test]
async fn an_upload_whose_answer_never_comes_fails_after_the_stall_time() {
    let server = never_answers(QUICK).await;
    let keys = Keys::generate();
    let start = StdInstant::now();
    let result = upload_blob(&server, &keys, vec![4; 20]).await;
    assert_eq!(result, Err(BlossomError::TimedOut));
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn a_download_whose_head_never_comes_fails_after_the_answer_time() {
    let patience = Patience {
        stall: Duration::from_secs(30),
        answer: Duration::from_millis(400),
    };
    let server = never_answers(patience).await;
    let hash = sha(b"whatever");
    let url = format!("{}{hash}", server.address());
    let start = StdInstant::now();
    let result = download(&url, &hash, 1024, Some(&server), &SystemResolver)
        .await
        .map(|_| ());
    assert_eq!(result, Err(BlossomError::TimedOut));
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn a_body_that_keeps_coming_is_not_cut_by_the_time_for_the_head() {
    let patience = Patience {
        stall: Duration::from_millis(400),
        answer: Duration::from_millis(300),
    };
    let pieces = vec![vec![9; 5]; 5];
    let blob: Vec<u8> = pieces.concat();
    let hash = sha(&blob);
    // Five pieces 120 ms apart take longer than the answer time in all.
    let (server, url) = trickles(pieces, Duration::from_millis(120), patience, &hash).await;
    let got = fetch(&url, &hash, 1024, Some(&server)).await;
    assert_eq!(got, Ok(blob));
}

#[tokio::test]
async fn a_download_past_its_total_allowance_fails_before_the_next_piece() {
    let hash = sha(b"whatever");
    let (server, url) =
        trickles(vec![vec![9; 5]; 2], Duration::from_millis(50), QUICK, &hash).await;
    let mut stream = download(&url, &hash, 1024, Some(&server), &SystemResolver)
        .await
        .expect("the head comes");
    stream.until = Instant::now();
    assert_eq!(stream.chunk().await, Err(BlossomError::TimedOut));
}
