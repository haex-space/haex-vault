//! The fixture site for the agent browser tests (spec 065, T027, T046): a small HTTP server on
//! loopback that serves `tests/fixtures/agent_browser/site/` and records every request with its
//! host and method. The same server is two websites: `http://127.0.0.1:<port>` and
//! `http://localhost:<port>` (the partner), so the website check sees a second origin without a
//! second server. `/hang` answers only after a minute; any method but `GET` answers with a short
//! page. `/injection/<n>` serves `tests/fixtures/agent_browser/injection/<n>.html`, the pages with
//! injected instructions (T047).
//!
//! [`start`] serves plain HTTP: a website over `http` may be read (research R9). [`start_tls`]
//! serves HTTPS with the test certificate of `tests/fixtures/agent_browser/tls/`, for the tools
//! that operate; the test's Chromium has to accept that certificate.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// One request the site received: its `Host` header, method and path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub host: String,
    pub method: String,
    pub path: String,
}

/// The running site.
pub struct Site {
    pub addr: SocketAddr,
    scheme: &'static str,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Site {
    /// `<scheme>://127.0.0.1:<port><path>`.
    pub fn url(&self, path: &str) -> String {
        format!("{}://{}{path}", self.scheme, self.addr)
    }

    /// `<scheme>://localhost:<port><path>`: the same server as another website.
    pub fn partner(&self, path: &str) -> String {
        format!("{}://localhost:{}{path}", self.scheme, self.addr.port())
    }

    /// The website of [`Site::url`].
    pub fn origin(&self) -> String {
        format!("{}://{}", self.scheme, self.addr)
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    /// The requests with `method` to `path`.
    pub fn count(&self, method: &str, path: &str) -> usize {
        self.seen()
            .iter()
            .filter(|seen| seen.method == method && seen.path == path)
            .count()
    }
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agent_browser")
}

/// Starts the site over plain HTTP on a free port.
pub async fn start() -> Site {
    serve(None).await
}

/// Starts the site over HTTPS with the test certificate on a free port.
pub async fn start_tls() -> Site {
    use tokio_rustls::rustls::pki_types::pem::PemObject;
    use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
    let certs = CertificateDer::pem_file_iter(dir().join("tls/cert.pem"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let key = PrivateKeyDer::from_pem_file(dir().join("tls/key.pem")).unwrap();
    let config = tokio_rustls::rustls::ServerConfig::builder_with_provider(Arc::new(
        tokio_rustls::rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(certs, key)
    .unwrap();
    serve(Some(tokio_rustls::TlsAcceptor::from(Arc::new(config)))).await
}

async fn serve(tls: Option<tokio_rustls::TlsAcceptor>) -> Site {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recording = Arc::clone(&seen);
    let scheme = if tls.is_some() { "https" } else { "http" };
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let recording = Arc::clone(&recording);
            let tls = tls.clone();
            tokio::spawn(async move {
                match tls {
                    Some(acceptor) => {
                        if let Ok(stream) = acceptor.accept(stream).await {
                            answer(stream, addr, scheme, &recording).await;
                        }
                    }
                    None => answer(stream, addr, scheme, &recording).await,
                }
            });
        }
    });
    Site { addr, scheme, seen }
}

async fn answer(
    mut stream: impl AsyncRead + AsyncWrite + Unpin,
    addr: SocketAddr,
    scheme: &'static str,
    recording: &Mutex<Vec<Seen>>,
) {
    let mut buffer = vec![0u8; 16384];
    let Ok(read) = stream.read(&mut buffer).await else {
        return;
    };
    let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
    let mut first = request
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = first.next().unwrap_or("GET").to_owned();
    let path = first.next().unwrap_or("/").to_owned();
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
        host: host.clone(),
        method: method.clone(),
        path: path.clone(),
    });
    let file = path.split(['?', '#']).next().unwrap_or("/");
    // A download of 150 MB that never finishes, for the question over 100 MB (spec 065 FR-031).
    if file == "/big.bin" {
        let head = "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\n\
                    Content-Disposition: attachment; filename=\"gross.bin\"\r\n\
                    Content-Length: 150000000\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(head.as_bytes()).await;
        let _ = stream.write_all(&[0u8; 65536]).await;
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        return;
    }
    // A page that never answers in time, for stopping a call that waits for it.
    if file == "/hang" {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
    let file = if file == "/" { "/index.html" } else { file };
    let name = file.trim_start_matches('/');
    let file_path = match name.strip_prefix("injection/") {
        Some(page) => dir().join("injection").join(format!("{page}.html")),
        None => dir().join("site").join(name),
    };
    let found = if method == "GET" {
        std::fs::read_to_string(file_path).ok()
    } else {
        Some(format!(
            "<!doctype html><title>Gesendet</title><main>{method} {file} angekommen</main>"
        ))
    };
    let reply = match found {
        Some(body) if !name.contains("..") => {
            let body = body.replace("PARTNER", &format!("{scheme}://localhost:{}", addr.port()));
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }
        _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
    };
    let _ = stream.write_all(reply.as_bytes()).await;
    let _ = stream.shutdown().await;
}
