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

/// Per-app singleton — pinned port + tokens → path mapping. Cloning is
/// cheap (Arc) so the AppState can stash one instance and Tauri commands
/// take owned copies.
#[derive(Clone)]
pub struct MediaServer {
    port: u16,
    tokens: Arc<RwLock<HashMap<String, PathBuf>>>,
}

impl MediaServer {
    /// Start the server on a random loopback port. Returns immediately
    /// after `bind`; the accept loop runs in a background tokio task for
    /// the lifetime of the app.
    pub async fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let tokens: Arc<RwLock<HashMap<String, PathBuf>>> =
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
        if let Some((existing_token, _)) = map.iter().find(|(_, p)| **p == path) {
            return format!("http://127.0.0.1:{}/{}", self.port, existing_token);
        }
        let token = uuid::Uuid::new_v4().to_string();
        map.insert(token.clone(), path);
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
    tokens: Arc<RwLock<HashMap<String, PathBuf>>>,
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

    let target_path = {
        let map = tokens.read().await;
        map.get(token).cloned()
    };
    let target_path = match target_path {
        Some(p) => p,
        None => return write_status(&mut stream, 404, "Not Found").await,
    };

    // Open the file + figure out its size before deciding response code.
    let mut file = match tokio::fs::File::open(&target_path).await {
        Ok(f) => f,
        Err(_) => return write_status(&mut stream, 404, "Not Found").await,
    };
    let total = match file.metadata().await {
        Ok(m) => m.len(),
        Err(_) => return write_status(&mut stream, 500, "Internal Server Error").await,
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
        (0, total.saturating_sub(1), 200u16, "OK")
    };

    let content_length = end - start + 1;
    let mime = mime_for(&target_path);

    // Response header. CORS open — these URLs only resolve inside the
    // WebView (loopback) and any locally-running tool that could already
    // read the file directly off disk anyway.
    let mut header = format!("HTTP/1.1 {} {}\r\n", status, status_text);
    header.push_str(&format!("Content-Type: {}\r\n", mime));
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

    // Stream the body in 64 KiB chunks so we never hold more than that in
    // memory regardless of file size.
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
            // Client closed connection mid-stream (user closed the
            // player). Not an error worth surfacing.
            let kind = e.kind();
            if matches!(
                kind,
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
