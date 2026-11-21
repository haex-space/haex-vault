//! The test of a Blossom-Server address (spec 072 FR-080): a server that takes everything, one
//! with a size limit, one that refuses outright, one that is not there, and addresses a chat file
//! may not go to.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;
use crate::chat::peer::files::test_server::BlossomTestServer;
use crate::remote_storage::address::{Resolver, SystemResolver};

struct Unresolved;

#[async_trait::async_trait]
impl Resolver for Unresolved {
    async fn lookup(&self, _host: &str, _port: u16) -> std::io::Result<Vec<std::net::IpAddr>> {
        Err(std::io::Error::other("lookup failed"))
    }
}

#[tokio::test]
async fn a_server_that_takes_the_largest_file_accepts() {
    let server = BlossomTestServer::start().await;
    let result = check(&server.url, &SystemResolver).await;
    assert_eq!(result, Check::of(Verdict::Accepts));
    let asked: Vec<_> = server
        .log()
        .into_iter()
        .map(|logged| logged.method)
        .collect();
    assert_eq!(
        asked,
        ["HEAD"],
        "one question is enough, and nothing is uploaded"
    );
}

#[tokio::test]
async fn a_server_with_a_size_limit_takes_only_small_files_and_gives_its_reason() {
    let server = BlossomTestServer::start().await;
    server.limit(1_000_000, "at most 1 MB");
    let result = check(&server.url, &SystemResolver).await;
    assert_eq!(result.verdict, Verdict::OnlySmall);
    assert_eq!(result.status, Some(413));
    assert_eq!(result.reason.as_deref(), Some("at most 1 MB"));
}

#[tokio::test]
async fn a_server_that_refuses_even_a_small_file_is_refused() {
    let server = BlossomTestServer::start().await;
    server.limit(0, "no uploads here");
    let result = check(&server.url, &SystemResolver).await;
    assert_eq!(result.verdict, Verdict::Refused);
    assert_eq!(result.status, Some(413));
    assert_eq!(result.reason.as_deref(), Some("no uploads here"));
}

#[tokio::test]
async fn a_refusal_that_is_not_about_size_is_asked_about_once() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = format!("http://{}/", listener.local_addr().expect("address"));
    let asked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&asked);
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            counted.fetch_add(1, Ordering::SeqCst);
            let mut head = [0_u8; 4096];
            let _ = stream.read(&mut head).await;
            let _ = stream
                .write_all(
                    b"HTTP/1.1 403 Forbidden\r\nX-Reason: members only\r\n\
                      Content-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
        }
    });
    let result = check(&address, &SystemResolver).await;
    assert_eq!(result.verdict, Verdict::Refused);
    assert_eq!(result.status, Some(403));
    assert_eq!(result.reason.as_deref(), Some("members only"));
    assert_eq!(asked.load(Ordering::SeqCst), 1, "no second question");
}

#[tokio::test]
async fn a_server_that_is_not_there_is_unreachable() {
    // The port was free a moment ago; nothing listens on it now.
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = format!("http://{}/", listener.local_addr().expect("address"));
    drop(listener);
    let result = check(&address, &SystemResolver).await;
    assert_eq!(result, Check::of(Verdict::Unreachable));
}

#[tokio::test]
async fn a_server_whose_name_does_not_resolve_is_unreachable() {
    let result = check("https://blossom.example.com/", &Unresolved).await;
    assert_eq!(result, Check::of(Verdict::Unreachable));
}

#[tokio::test]
async fn an_address_a_chat_file_may_not_go_to_is_not_allowed() {
    for address in [
        "http://example.com/",
        "ftp://example.com/",
        "https://user:secret@example.com/",
        "https://example.com/?key=1",
        "not an address",
        "",
    ] {
        let result = check(address, &SystemResolver).await;
        assert_eq!(result, Check::of(Verdict::NotAllowed), "{address:?}");
    }
}

#[test]
fn the_shape_check_takes_what_the_server_takes_and_nothing_else() {
    for good in [
        "https://blossom.example.com",
        "https://blossom.example.com/files/",
        " https://blossom.example.com/ ",
        "http://127.0.0.1:3000/",
    ] {
        assert!(super::super::blossom::is_valid_address(good), "{good:?}");
    }
    for bad in [
        "",
        "http://blossom.example.com/",
        "https://a:b@blossom.example.com/",
        "https://blossom.example.com/#top",
        "blossom.example.com",
    ] {
        assert!(!super::super::blossom::is_valid_address(bad), "{bad:?}");
    }
}
