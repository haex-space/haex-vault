use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::files::streaming::LocalFileSource;

/// One raw request and the whole answer as text (bodies are small here) plus its raw bytes.
async fn request(port: u16, raw: &str) -> (String, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let split = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map_or(bytes.len(), |i| i + 4);
    (
        String::from_utf8_lossy(&bytes[..split]).into_owned(),
        bytes[split..].to_vec(),
    )
}

fn get(token: &str, range: Option<&str>) -> String {
    let range = range.map(|r| format!("Range: {r}\r\n")).unwrap_or_default();
    format!("GET /{token} HTTP/1.1\r\nHost: 127.0.0.1\r\n{range}\r\n")
}

async fn server_with_file(content: &[u8]) -> (MediaServer, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("film.mp4");
    std::fs::write(&path, content).unwrap();
    let server = MediaServer::start(CancellationToken::new()).await.unwrap();
    let url = server.register_file(
        "tab-1",
        path.clone(),
        Arc::new(LocalFileSource::new(path, "video/mp4")),
    );
    let token = url.rsplit('/').next().unwrap().to_owned();
    (server, token, dir)
}

#[tokio::test]
async fn the_url_names_the_loopback_and_a_uuid_token() {
    let (server, token, _dir) = server_with_file(b"x").await;
    assert!(uuid::Uuid::parse_str(&token).is_ok());
    assert_eq!(
        server.url_of(&token),
        format!("http://127.0.0.1:{}/{token}", server.port())
    );
}

#[tokio::test]
async fn the_whole_file_comes_with_200() {
    let (server, token, _dir) = server_with_file(b"0123456789").await;
    let (head, body) = request(server.port(), &get(&token, None)).await;
    assert!(head.starts_with("HTTP/1.1 200 OK"), "{head}");
    assert!(head.contains("Content-Type: video/mp4"));
    assert_eq!(body, b"0123456789");
}

#[tokio::test]
async fn a_range_comes_with_206() {
    let (server, token, _dir) = server_with_file(b"0123456789").await;
    let (head, body) = request(server.port(), &get(&token, Some("bytes=2-5"))).await;
    assert!(head.starts_with("HTTP/1.1 206"), "{head}");
    assert!(head.contains("Content-Range: bytes 2-5/10"));
    assert_eq!(body, b"2345");
}

#[tokio::test]
async fn an_unsatisfiable_range_comes_with_416() {
    let (server, token, _dir) = server_with_file(b"0123456789").await;
    let (head, _) = request(server.port(), &get(&token, Some("bytes=20-"))).await;
    assert!(head.starts_with("HTTP/1.1 416"), "{head}");
    assert!(head.contains("Content-Range: bytes */10"));
}

#[tokio::test]
async fn head_has_no_body() {
    let (server, token, _dir) = server_with_file(b"0123456789").await;
    let (head, body) = request(server.port(), &format!("HEAD /{token} HTTP/1.1\r\n\r\n")).await;
    assert!(head.contains("Content-Length: 10"), "{head}");
    assert!(body.is_empty());
}

#[tokio::test]
async fn options_answers_the_preflight() {
    let (server, token, _dir) = server_with_file(b"x").await;
    let (head, _) = request(server.port(), &format!("OPTIONS /{token} HTTP/1.1\r\n\r\n")).await;
    assert!(head.starts_with("HTTP/1.1 204"), "{head}");
    assert!(head.contains("Access-Control-Allow-Headers: Range"));
}

#[tokio::test]
async fn unknown_tokens_and_the_root_are_404() {
    let (server, _token, _dir) = server_with_file(b"x").await;
    for path in ["/", "/nope", "/a/b"] {
        let (head, _) = request(server.port(), &format!("GET {path} HTTP/1.1\r\n\r\n")).await;
        assert!(head.starts_with("HTTP/1.1 404"), "{path}: {head}");
    }
}

#[tokio::test]
async fn other_methods_are_refused() {
    let (server, token, _dir) = server_with_file(b"x").await;
    let (head, _) = request(server.port(), &format!("POST /{token} HTTP/1.1\r\n\r\n")).await;
    assert!(head.starts_with("HTTP/1.1 405"), "{head}");
}

#[tokio::test]
async fn a_released_token_is_404() {
    let (server, token, _dir) = server_with_file(b"x").await;
    assert!(server.release(&server.url_of(&token)));
    let (head, _) = request(server.port(), &get(&token, None)).await;
    assert!(head.starts_with("HTTP/1.1 404"), "{head}");
    assert!(!server.release(&token));
}

#[tokio::test]
async fn releasing_a_tab_drops_only_its_tokens() {
    let (server, token, dir) = server_with_file(b"x").await;
    let other = server.register(
        "tab-2",
        Arc::new(LocalFileSource::new(
            dir.path().join("film.mp4"),
            "video/mp4",
        )),
    );
    server.release_tab("tab-1");
    let (gone, _) = request(server.port(), &get(&token, None)).await;
    assert!(gone.starts_with("HTTP/1.1 404"), "{gone}");
    let other_token = other.rsplit('/').next().unwrap();
    let (kept, _) = request(server.port(), &get(other_token, None)).await;
    assert!(kept.starts_with("HTTP/1.1 200"), "{kept}");
}

#[tokio::test]
async fn releasing_a_drive_drops_only_the_tokens_of_its_files() {
    let (server, token, dir) = server_with_file(b"x").await;
    let beside = dir.path().with_extension("neben");
    let kept = server.register_file(
        "tab-1",
        beside.join("film.mp4"),
        Arc::new(LocalFileSource::new(
            dir.path().join("film.mp4"),
            "video/mp4",
        )),
    );
    let storage = server.register(
        "tab-1",
        Arc::new(LocalFileSource::new(
            dir.path().join("film.mp4"),
            "video/mp4",
        )),
    );

    assert_eq!(server.release_under(dir.path()), 1);

    let (gone, _) = request(server.port(), &get(&token, None)).await;
    assert!(gone.starts_with("HTTP/1.1 404"), "{gone}");
    for url in [kept, storage] {
        let (head, _) = request(server.port(), &get(url.rsplit('/').next().unwrap(), None)).await;
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    }
}

#[tokio::test]
async fn an_empty_file_is_an_empty_200() {
    let (server, token, _dir) = server_with_file(b"").await;
    let (head, body) = request(server.port(), &get(&token, None)).await;
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(head.contains("Content-Length: 0"));
    assert!(body.is_empty());
}

#[tokio::test]
async fn the_server_stops_with_its_token() {
    let cancel = CancellationToken::new();
    let server = MediaServer::start(cancel.clone()).await.unwrap();
    cancel.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(TcpStream::connect(("127.0.0.1", server.port()))
        .await
        .is_err());
}

/// A writer that records the largest single write it was handed.
#[derive(Default)]
struct Counting {
    total: u64,
    largest: usize,
}

impl AsyncWrite for Counting {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.total += buf.len() as u64;
        self.largest = self.largest.max(buf.len());
        Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// FR-013: a large file goes out in chunks of at most [`CHUNK`]; memory does not grow with it.
#[tokio::test]
async fn a_large_file_goes_out_in_bounded_chunks() {
    const SIZE: u64 = 64 * 1024 * 1024;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gross.bin");
    std::fs::File::create(&path).unwrap().set_len(SIZE).unwrap();
    let source = LocalFileSource::new(path, "application/octet-stream");
    let mut reader = source.open_range(0, SIZE).await.unwrap();
    let mut writer = Counting::default();
    copy_body(&mut reader, &mut writer).await.unwrap();
    assert_eq!(writer.total, SIZE);
    assert!(writer.largest <= CHUNK, "{}", writer.largest);
}
