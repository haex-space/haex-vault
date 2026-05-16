//! Local HTTP range server for cached media files.
//!
//! Why this exists: WebKitGTK's GStreamer media backend on Linux refuses
//! to play `<audio>` / `<video>` sourced from Tauri's custom URI schemes
//! (`haex-stream://`, `asset://`). It only follows http(s) / file URLs.
//! We sidestep the issue by binding a tiny tokio HTTP server to a random
//! loopback port and handing the media element a plain
//! `http://127.0.0.1:<port>/<token>` URL — which GStreamer accepts on
//! every platform.
//!
//! Security: the server only serves files whose absolute path has been
//! explicitly registered via [`MediaServer::register`]. The registration
//! returns an opaque token (UUIDv4) that the caller embeds in the URL —
//! browsing `/` or any unknown token returns 404. This keeps the surface
//! exactly equal to "what the WebView has already been handed", regardless
//! of who else can speak to the loopback port.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

use crate::remote_storage::streaming::source::{ByteRange, StreamingSource};

/// Source of bytes for a registered token. Either a plain on-disk file or
/// a virtual streaming source (S3, peer, …). Kept private — callers go
/// through `register` / `register_source`.
#[derive(Clone)]
enum MediaSource {
    Local(PathBuf),
    Stream {
        source: Arc<dyn StreamingSource>,
        content_type: Option<String>,
    },
}

/// Per-app singleton — pinned port + tokens → media source mapping.
/// Cloning is cheap (Arc) so the AppState can stash one instance and
/// Tauri commands take owned copies.
#[derive(Clone)]
pub struct MediaServer {
    port: u16,
    tokens: Arc<RwLock<HashMap<String, MediaSource>>>,
}

impl MediaServer {
    /// Start the server on a random loopback port. Returns immediately
    /// after `bind`; the accept loop runs in a background tokio task for
    /// the lifetime of the app.
    pub async fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let tokens: Arc<RwLock<HashMap<String, MediaSource>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let tokens_for_accept = tokens.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let tokens = tokens_for_accept.clone();
                        tokio::spawn(async move {
                            // We log but don't propagate — a misbehaving
                            // client should never poison the accept loop.
                            if let Err(e) = handle_connection(stream, tokens).await {
                                eprintln!("[media-server] connection error: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("[media-server] accept error: {e}");
                        // No back-off — a brief loop is fine and we'd
                        // rather come back quickly when the OS is happy
                        // again than hang the server.
                    }
                }
            }
        });

        Ok(Self { port, tokens })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Register `path` and return a URL the WebView can give an
    /// `<audio>`/`<video>` element. If the same path is already registered
    /// the existing token is reused so the registry can't grow unbounded
    /// across repeated plays of the same file.
    pub async fn register(&self, path: PathBuf) -> String {
        let mut map = self.tokens.write().await;
        if let Some((existing_token, _)) = map
            .iter()
            .find(|(_, src)| matches!(src, MediaSource::Local(p) if *p == path))
        {
            return format!("http://127.0.0.1:{}/{}", self.port, existing_token);
        }
        let token = uuid::Uuid::new_v4().to_string();
        map.insert(token.clone(), MediaSource::Local(path));
        format!("http://127.0.0.1:{}/{}", self.port, token)
    }

    /// Register a virtual streaming source. Each call produces a fresh
    /// token — sources can hold mutable handles (open QUIC connection,
    /// SDK client) that don't dedup cleanly, so cache identity is the
    /// caller's problem.
    pub async fn register_source(
        &self,
        source: Arc<dyn StreamingSource>,
        content_type: Option<String>,
    ) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        let mut map = self.tokens.write().await;
        map.insert(
            token.clone(),
            MediaSource::Stream {
                source,
                content_type,
            },
        );
        format!("http://127.0.0.1:{}/{}", self.port, token)
    }
}

/// Minimal HTTP/1.1 GET handler with Range support.
///
/// The server only cares about the request line and the `Range:` header —
/// we ignore everything else. Reply bodies are streamed in fixed-size
/// chunks so memory stays flat regardless of file size.
async fn handle_connection(
    mut stream: TcpStream,
    tokens: Arc<RwLock<HashMap<String, MediaSource>>>,
) -> std::io::Result<()> {
    let mut buf = [0u8; 8192];
    let mut request_bytes: Vec<u8> = Vec::with_capacity(1024);
    // Read until the end of HTTP headers (CRLF CRLF).
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        request_bytes.extend_from_slice(&buf[..n]);
        if request_bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        // Cap the header size so a misbehaving client can't run us out of
        // memory by trickling bytes forever.
        if request_bytes.len() > 16 * 1024 {
            return write_status(&mut stream, 431, "Request Header Fields Too Large").await;
        }
    }

    let header_text = match std::str::from_utf8(&request_bytes) {
        Ok(s) => s,
        Err(_) => return write_status(&mut stream, 400, "Bad Request").await,
    };

    let mut lines = header_text.lines();
    let request_line = match lines.next() {
        Some(l) => l,
        None => return write_status(&mut stream, 400, "Bad Request").await,
    };

    // Method must be GET or HEAD; path is whatever follows the method.
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    if method != "GET" && method != "HEAD" {
        return write_status(&mut stream, 405, "Method Not Allowed").await;
    }

    let token = path.trim_start_matches('/');
    if token.is_empty() {
        return write_status(&mut stream, 404, "Not Found").await;
    }

    let media_source = {
        let map = tokens.read().await;
        map.get(token).cloned()
    };
    let media_source = match media_source {
        Some(s) => s,
        None => return write_status(&mut stream, 404, "Not Found").await,
    };

    // Resolve total size and content-type before deciding response code.
    // For the local branch we also need the open file handle for the
    // body-streaming step below, so we keep it here.
    let (total, content_type, mut local_file) = match &media_source {
        MediaSource::Local(path) => {
            let file = match tokio::fs::File::open(path).await {
                Ok(f) => f,
                Err(_) => return write_status(&mut stream, 404, "Not Found").await,
            };
            let size = match file.metadata().await {
                Ok(m) => m.len(),
                Err(_) => {
                    return write_status(&mut stream, 500, "Internal Server Error").await
                }
            };
            (size, mime_for(path).to_string(), Some(file))
        }
        MediaSource::Stream {
            source,
            content_type,
        } => {
            let size = match source.size().await {
                Ok(n) => n,
                Err(_) => {
                    return write_status(&mut stream, 500, "Internal Server Error").await
                }
            };
            let ct = content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_string());
            (size, ct, None)
        }
    };

    // Parse `Range: bytes=N-M` / `bytes=N-` / `bytes=-N` if present.
    let range_header = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("range") {
                Some(value.trim().to_string())
            } else {
                None
            }
        })
        .next();

    // 0-byte body: any Range header is unsatisfiable; otherwise return an
    // empty 200. Mirrors the haex-stream protocol handler's behavior.
    if total == 0 {
        if range_header.is_some() {
            return write_range_unsatisfiable(&mut stream, 0).await;
        }
        let mut header = String::from("HTTP/1.1 200 OK\r\n");
        header.push_str(&format!("Content-Type: {}\r\n", content_type));
        header.push_str("Content-Length: 0\r\n");
        header.push_str("Accept-Ranges: bytes\r\n");
        header.push_str("Cache-Control: no-store\r\n");
        header.push_str("Access-Control-Allow-Origin: *\r\n");
        header.push_str("Connection: close\r\n");
        header.push_str("\r\n");
        stream.write_all(header.as_bytes()).await?;
        return Ok(());
    }

    let (start, end, status, status_text) = if let Some(spec) = range_header {
        match parse_range(&spec, total) {
            Some((s, e)) => (s, e, 206u16, "Partial Content"),
            None => {
                // Unsatisfiable — RFC 7233 wants `bytes */<total>` so the
                // client knows the real size.
                return write_range_unsatisfiable(&mut stream, total).await;
            }
        }
    } else {
        (0, total - 1, 200u16, "OK")
    };

    let content_length = end - start + 1;

    // Response header. CORS open — these URLs only resolve inside the
    // WebView (loopback) and any locally-running tool that could already
    // read the file directly off disk anyway.
    let mut header = format!("HTTP/1.1 {} {}\r\n", status, status_text);
    header.push_str(&format!("Content-Type: {}\r\n", content_type));
    header.push_str(&format!("Content-Length: {}\r\n", content_length));
    header.push_str("Accept-Ranges: bytes\r\n");
    if status == 206 {
        header.push_str(&format!(
            "Content-Range: bytes {}-{}/{}\r\n",
            start, end, total
        ));
    }
    header.push_str("Cache-Control: no-store\r\n");
    header.push_str("Access-Control-Allow-Origin: *\r\n");
    header.push_str("Connection: close\r\n");
    header.push_str("\r\n");
    stream.write_all(header.as_bytes()).await?;

    if method == "HEAD" {
        return Ok(());
    }

    match &media_source {
        MediaSource::Local(_) => {
            // Already opened above — `local_file` is Some on this branch.
            let file = local_file
                .as_mut()
                .expect("local_file populated for Local branch");
            stream_local_chunks(&mut stream, file, start, content_length).await
        }
        MediaSource::Stream { source, .. } => {
            let range = match ByteRange::new(start, end) {
                Ok(r) => r,
                Err(_) => return Ok(()), // headers already sent; bail
            };
            let bytes = match source.read_range(range).await {
                Ok(b) => b,
                // Headers are already on the wire — we can't switch status
                // codes now. Drop the connection; the client will see a
                // truncated body, which is the best signal available.
                Err(_) => return Ok(()),
            };
            write_body_chunks(&mut stream, &bytes).await
        }
    }
}

/// Stream a byte range from an open file in 64 KiB chunks. Seeks to
/// `start` first; reads exactly `content_length` bytes (or until EOF).
async fn stream_local_chunks(
    stream: &mut TcpStream,
    file: &mut tokio::fs::File,
    start: u64,
    content_length: u64,
) -> std::io::Result<()> {
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let mut reader = BufReader::new(file);
    let mut remaining = content_length;
    let mut chunk = vec![0u8; 64 * 1024];
    while remaining > 0 {
        let to_read = remaining.min(chunk.len() as u64) as usize;
        let n = reader.read(&mut chunk[..to_read]).await?;
        if n == 0 {
            break;
        }
        if let Err(e) = stream.write_all(&chunk[..n]).await {
            if matches!(
                e.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted,
            ) {
                return Ok(());
            }
            return Err(e);
        }
        remaining -= n as u64;
    }
    Ok(())
}

/// Write an in-memory buffer to the TCP stream in 64 KiB chunks,
/// swallowing the broken-pipe family (client disconnected).
async fn write_body_chunks(stream: &mut TcpStream, bytes: &[u8]) -> std::io::Result<()> {
    for chunk in bytes.chunks(64 * 1024) {
        if let Err(e) = stream.write_all(chunk).await {
            if matches!(
                e.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted,
            ) {
                return Ok(());
            }
            return Err(e);
        }
    }
    Ok(())
}

fn parse_range(spec: &str, total: u64) -> Option<(u64, u64)> {
    let rest = spec.trim().strip_prefix("bytes=")?;
    // Browsers don't actually use multi-range for media; reject so we
    // don't have to assemble a multipart response.
    if rest.contains(',') {
        return None;
    }
    let (start_s, end_s) = rest.split_once('-')?;
    if start_s.is_empty() {
        // Suffix range "bytes=-N" → last N bytes.
        let n: u64 = end_s.trim().parse().ok()?;
        if n == 0 || total == 0 {
            return None;
        }
        let start = total.saturating_sub(n);
        return Some((start, total - 1));
    }
    let start: u64 = start_s.trim().parse().ok()?;
    let end: u64 = if end_s.trim().is_empty() {
        total.saturating_sub(1)
    } else {
        end_s.trim().parse().ok()?
    };
    if start > end || end >= total {
        return None;
    }
    Some((start, end))
}

fn mime_for(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("flac") => "audio/flac",
        Some("ogg") => "audio/ogg",
        Some("aac") => "audio/aac",
        Some("m4a") => "audio/mp4",
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("ogv") => "video/ogg",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}

async fn write_status(
    stream: &mut TcpStream,
    code: u16,
    text: &str,
) -> std::io::Result<()> {
    let body = text.to_string();
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
        code,
        text,
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

async fn write_range_unsatisfiable(stream: &mut TcpStream, total: u64) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{}\r\nContent-Length: 0\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        total
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

#[tauri::command]
pub async fn media_server_register(
    state: tauri::State<'_, crate::AppState>,
    path: String,
) -> Result<String, String> {
    let pb = PathBuf::from(&path);
    if !pb.is_absolute() {
        return Err(format!("media_server_register requires absolute path: {path}"));
    }
    let meta = tokio::fs::metadata(&pb)
        .await
        .map_err(|e| format!("media_server_register: cannot stat {path}: {e}"))?;
    if !meta.is_file() {
        return Err(format!(
            "media_server_register: not a regular file: {path}"
        ));
    }
    Ok(state.media_server.register(pb).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Arc;

    struct DummySource {
        data: Vec<u8>,
    }

    #[async_trait]
    impl crate::remote_storage::streaming::source::StreamingSource for DummySource {
        async fn size(
            &self,
        ) -> Result<u64, crate::remote_storage::streaming::source::StreamingError> {
            Ok(self.data.len() as u64)
        }
        async fn read_range(
            &self,
            range: crate::remote_storage::streaming::source::ByteRange,
        ) -> Result<Vec<u8>, crate::remote_storage::streaming::source::StreamingError> {
            Ok(self.data[range.start() as usize..=range.end() as usize].to_vec())
        }
    }

    #[tokio::test]
    async fn serves_range_from_streaming_source() {
        let server = MediaServer::start().await.unwrap();
        let source = Arc::new(DummySource {
            data: (0u8..=200).collect(),
        });
        let url = server
            .register_source(source, Some("application/octet-stream".into()))
            .await;

        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let resp = client
            .get(&url)
            .header("Range", "bytes=10-19")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 206);
        assert_eq!(
            resp.headers().get("Content-Range").unwrap(),
            "bytes 10-19/201",
        );
        let body = resp.bytes().await.unwrap();
        assert_eq!(body.as_ref(), &(10u8..=19).collect::<Vec<u8>>()[..]);
    }

    #[tokio::test]
    async fn serves_full_body_from_streaming_source_without_range_header() {
        let server = MediaServer::start().await.unwrap();
        let source = Arc::new(DummySource {
            data: vec![0xAB; 256],
        });
        let url = server.register_source(source, None).await;

        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let resp = client.get(&url).send().await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
        assert_eq!(
            resp.headers().get("Content-Type").unwrap(),
            "application/octet-stream",
        );
        let body = resp.bytes().await.unwrap();
        assert_eq!(body.len(), 256);
        assert!(body.iter().all(|b| *b == 0xAB));
    }
}
