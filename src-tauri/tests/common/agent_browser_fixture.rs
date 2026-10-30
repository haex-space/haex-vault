//! The fixture site for the agent browser tests (spec 065, T027): a small HTTP server on loopback
//! that serves `tests/fixtures/agent_browser/site/` and records every request with its host.
//! The same server is two websites: `http://127.0.0.1:<port>` and `http://localhost:<port>`
//! (the partner), so the website check sees a second origin without a second server.
//!
//! `/hang` answers only after a minute.
//!
//! The fixtures are plain HTTP: a website over `http` may be read (research R9), which is all the
//! reading tools need; HTTPS fixtures come with the tools that operate.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// One request the site received: its `Host` header and path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub host: String,
    pub path: String,
}

/// The running site.
pub struct Site {
    pub addr: SocketAddr,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Site {
    /// `http://127.0.0.1:<port><path>`.
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    /// `http://localhost:<port><path>`: the same server as another website.
    pub fn partner(&self, path: &str) -> String {
        format!("http://localhost:{}{path}", self.addr.port())
    }

    /// The website of [`Site::url`].
    pub fn origin(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agent_browser/site")
}

/// Starts the site on a free port.
pub async fn start() -> Site {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recording = Arc::clone(&seen);
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let recording = Arc::clone(&recording);
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                let Ok(read) = stream.read(&mut buffer).await else {
                    return;
                };
                let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_owned();
                let host = request
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Host: ")
                            .or_else(|| line.strip_prefix("host: "))
                    })
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
                recording.lock().unwrap().push(Seen {
                    host,
                    path: path.clone(),
                });
                let file = path.split(['?', '#']).next().unwrap_or("/");
                // A page that never answers in time, for stopping a call that waits for it.
                if file == "/hang" {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }
                let file = if file == "/" { "/index.html" } else { file };
                let name = file.trim_start_matches('/');
                let reply = match std::fs::read_to_string(dir().join(name)) {
                    Ok(body) if !name.contains("..") => {
                        let body =
                            body.replace("PARTNER", &format!("http://localhost:{}", addr.port()));
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    }
                    _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_owned(),
                };
                let _ = stream.write_all(reply.as_bytes()).await;
            });
        }
    });
    Site { addr, seen }
}
