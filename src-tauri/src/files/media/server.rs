//! The local media server (spec 044 FR-012, FR-013, FR-016, research R4, contracts/media-server.md;
//! after haex-vault 2.x (Legacy) `src-tauri/src/media_server/mod.rs`, revision
//! fc4e84b61a050576ba42e0dc832d04064a8605a3).
//!
//! WebKitGTK plays `<video>`/`<audio>` only from http(s), and `asset://` has no ranges, so holzi
//! serves opened files on `127.0.0.1` at a random port. Every file gets an unguessable token for one
//! tab; any other path, also `/`, is `404`. A token ends with [`MediaServer::release`], with
//! [`MediaServer::release_tab`] when its tab closes, and with the process when the vault locks
//! (ADR 0003): the accept loop stops with the gate's token. Bodies go out through a fixed buffer of
//! [`CHUNK`] bytes, whatever the size of the file.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::http::{self, Head, MAX_HEAD};
use super::range::{span, Span};
use crate::files::streaming::StreamingSource;

/// The most of a body held in memory at once.
pub const CHUNK: usize = 256 * 1024;

struct Grant {
    tab: String,
    source: Arc<dyn StreamingSource>,
}

type Grants = Arc<Mutex<HashMap<String, Grant>>>;

/// The running server; cheap to clone.
#[derive(Clone)]
pub struct MediaServer {
    port: u16,
    grants: Grants,
}

impl MediaServer {
    /// Binds `127.0.0.1` on a random port and serves until `stop` is cancelled.
    pub async fn start(stop: CancellationToken) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        let grants: Grants = Arc::default();
        let served = Arc::clone(&grants);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = stop.cancelled() => break,
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { continue };
                        let grants = Arc::clone(&served);
                        tokio::spawn(async move {
                            if let Err(error) = serve(stream, grants).await {
                                log::debug!("files: a media request ended early: {error}");
                            }
                        });
                    }
                }
            }
        });
        Ok(Self { port, grants })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    fn grants(&self) -> MutexGuard<'_, HashMap<String, Grant>> {
        self.grants.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The URL of `token`.
    pub fn url_of(&self, token: &str) -> String {
        format!("http://127.0.0.1:{}/{token}", self.port)
    }

    /// Serves `source` for `tab` under a new token; returns its URL.
    pub fn register(&self, tab: &str, source: Arc<dyn StreamingSource>) -> String {
        let token = Uuid::new_v4().to_string();
        self.grants().insert(
            token.clone(),
            Grant {
                tab: tab.to_owned(),
                source,
            },
        );
        self.url_of(&token)
    }

    /// Ends the token of `url` (or a bare token); true when there was one.
    pub fn release(&self, url: &str) -> bool {
        let token = url.rsplit('/').next().unwrap_or(url);
        self.grants().remove(token).is_some()
    }

    /// Ends every token of `tab`.
    pub fn release_tab(&self, tab: &str) {
        self.grants().retain(|_, grant| grant.tab != tab);
    }
}

async fn serve(mut stream: TcpStream, grants: Grants) -> std::io::Result<()> {
    let Some(head) = read_head(&mut stream).await? else {
        return write(&mut stream, &http::empty(400, "Bad Request")).await;
    };
    log::debug!(
        "files: media {} {} range {:?}",
        head.method,
        head.path.get(..9).unwrap_or(&head.path),
        head.range
    );
    if head.method == "OPTIONS" {
        return write(&mut stream, &http::empty(204, "No Content")).await;
    }
    if head.method != "GET" && head.method != "HEAD" {
        return write(&mut stream, &http::empty(405, "Method Not Allowed")).await;
    }
    let source = http::token_of(&head.path).and_then(|token| {
        grants
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(token)
            .map(|grant| Arc::clone(&grant.source))
    });
    let Some(source) = source else {
        return write(&mut stream, &http::empty(404, "Not Found")).await;
    };
    let Ok(total) = source.size().await else {
        return write(&mut stream, &http::empty(404, "Not Found")).await;
    };
    let (start, length, part) = match span(head.range.as_deref(), total) {
        Span::Whole => (0, total, None),
        Span::Part { start, end } => (start, end - start + 1, Some((start, end, total))),
        Span::Unsatisfiable => return write(&mut stream, &http::unsatisfiable(total)).await,
    };
    stream
        .write_all(http::body(source.content_type(), length, part).as_bytes())
        .await?;
    if head.method == "HEAD" || length == 0 {
        return stream.shutdown().await;
    }
    let mut reader = source.open_range(start, length).await?;
    copy_body(&mut reader, &mut stream).await?;
    stream.shutdown().await
}

/// Reads the request head; `None` when it is broken or larger than [`MAX_HEAD`].
async fn read_head(stream: &mut TcpStream) -> std::io::Result<Option<Head>> {
    let mut bytes = Vec::with_capacity(1024);
    let mut buf = [0u8; 4096];
    loop {
        if let Some(head) = http::parse_head(&bytes) {
            return Ok(Some(head));
        }
        if bytes.len() > MAX_HEAD {
            return Ok(None);
        }
        let read = stream.read(&mut buf).await?;
        if read == 0 {
            return Ok(None);
        }
        bytes.extend_from_slice(&buf[..read]);
    }
}

/// Answers with a head alone and closes.
async fn write(stream: &mut TcpStream, head: &str) -> std::io::Result<()> {
    stream.write_all(head.as_bytes()).await?;
    stream.shutdown().await
}

/// Copies `reader` to `writer` through one buffer of [`CHUNK`] bytes (FR-013).
pub async fn copy_body(
    reader: &mut (dyn AsyncRead + Send + Unpin),
    writer: &mut (impl AsyncWrite + Unpin),
) -> std::io::Result<()> {
    let mut buf = vec![0u8; CHUNK];
    loop {
        let read = reader.read(&mut buf).await?;
        if read == 0 {
            return writer.flush().await;
        }
        writer.write_all(&buf[..read]).await?;
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
