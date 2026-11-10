//! A Blossom-Server in the test process (spec 072 T056): blobs in memory, served over `hyper` on a
//! free port of the loopback interface. It checks the kind 24242 events as BUD-01/02 ask (signature,
//! `t`, `x`, `expiration`), deletes only for the uploading key, can refuse uploads over a size, and
//! logs every request with its body for the privacy checks. It ends with the test's runtime.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex, PoisonError};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use nostr::event::{Event, Kind};
use nostr::types::Timestamp;
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;

use super::blossom::AUTH_KIND;
use crate::sync::keys::hex;

/// One request as the server saw it.
#[derive(Debug, Clone)]
pub(crate) struct Logged {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Default)]
struct State {
    /// Hash → blob and the uploading key (hex).
    blobs: HashMap<String, (Vec<u8>, String)>,
    log: Vec<Logged>,
    /// Uploads larger than this are refused with `413` and the reason.
    max_size: Option<(u64, String)>,
}

/// A running test server.
#[derive(Clone)]
pub(crate) struct BlossomTestServer {
    pub url: String,
    state: Arc<Mutex<State>>,
}

impl BlossomTestServer {
    pub(crate) async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let server = Self {
            url: format!("http://127.0.0.1:{port}/"),
            state: Arc::default(),
        };
        tokio::spawn(serve(listener, server.clone()));
        server
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Refuses uploads larger than `bytes` with `413` and `reason` as `X-Reason`.
    pub(crate) fn limit(&self, bytes: u64, reason: &str) {
        self.lock().max_size = Some((bytes, reason.to_owned()));
    }

    pub(crate) fn has(&self, sha256: &str) -> bool {
        self.lock().blobs.contains_key(sha256)
    }

    pub(crate) fn log(&self) -> Vec<Logged> {
        self.lock().log.clone()
    }

    async fn handle(&self, request: Request<Incoming>) -> Response<Full<Bytes>> {
        let method = request.method().clone();
        let path = request.uri().path().to_owned();
        let headers: Vec<(String, String)> = request
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_owned(),
                    value.to_str().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        let body = match request.into_body().collect().await {
            Ok(collected) => collected.to_bytes().to_vec(),
            Err(_) => return reply(StatusCode::BAD_REQUEST, None, Vec::new()),
        };
        self.lock().log.push(Logged {
            method: method.to_string(),
            path: path.clone(),
            headers: headers.clone(),
            body: body.clone(),
        });
        let header = |name: &str| {
            headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.clone())
        };
        let sha = path.trim_start_matches('/').to_owned();
        match (method, path.as_str()) {
            (Method::HEAD, "/upload") => {
                let Some(hash) = header("x-sha-256") else {
                    return reply(StatusCode::BAD_REQUEST, Some("no X-SHA-256"), Vec::new());
                };
                if let Err(reason) = authorized(header("authorization"), "upload", &hash) {
                    return reply(StatusCode::UNAUTHORIZED, Some(reason), Vec::new());
                }
                let size = header("x-content-length")
                    .and_then(|size| size.parse::<u64>().ok())
                    .unwrap_or(0);
                self.over_limit(size)
                    .unwrap_or_else(|| reply(StatusCode::OK, None, Vec::new()))
            }
            (Method::PUT, "/upload") => {
                let hash = hex(&Sha256::digest(&body));
                let uploader = match authorized(header("authorization"), "upload", &hash) {
                    Ok(uploader) => uploader,
                    Err(reason) => {
                        return reply(StatusCode::UNAUTHORIZED, Some(reason), Vec::new())
                    }
                };
                if let Some(refusal) = self.over_limit(body.len() as u64) {
                    return refusal;
                }
                let size = body.len();
                self.lock().blobs.insert(hash.clone(), (body, uploader));
                let descriptor = serde_json::json!({
                    "url": format!("{}{hash}", self.url),
                    "sha256": hash,
                    "size": size,
                    "type": "application/octet-stream",
                    "uploaded": Timestamp::now().as_secs(),
                });
                reply(StatusCode::OK, None, descriptor.to_string().into_bytes())
            }
            (Method::GET, _) => match self.lock().blobs.get(&sha) {
                Some((blob, _)) => reply(StatusCode::OK, None, blob.clone()),
                None => reply(StatusCode::NOT_FOUND, None, Vec::new()),
            },
            (Method::DELETE, _) => {
                let deleter = match authorized(header("authorization"), "delete", &sha) {
                    Ok(deleter) => deleter,
                    Err(reason) => {
                        return reply(StatusCode::UNAUTHORIZED, Some(reason), Vec::new())
                    }
                };
                let mut state = self.lock();
                match state.blobs.get(&sha) {
                    None => reply(StatusCode::NOT_FOUND, None, Vec::new()),
                    Some((_, uploader)) if *uploader != deleter => {
                        reply(StatusCode::FORBIDDEN, Some("not the uploader"), Vec::new())
                    }
                    Some(_) => {
                        state.blobs.remove(&sha);
                        reply(StatusCode::OK, None, Vec::new())
                    }
                }
            }
            _ => reply(StatusCode::NOT_FOUND, None, Vec::new()),
        }
    }

    fn over_limit(&self, size: u64) -> Option<Response<Full<Bytes>>> {
        let state = self.lock();
        let (max, reason) = state.max_size.as_ref()?;
        (size > *max).then(|| {
            reply(
                StatusCode::PAYLOAD_TOO_LARGE,
                Some(reason.as_str()),
                Vec::new(),
            )
        })
    }
}

/// Checks `Nostr <base64url(event)>`: signed, kind 24242, `t = action`, `x = sha256`, not expired.
/// Answers the signing key (hex).
fn authorized(header: Option<String>, action: &str, sha256: &str) -> Result<String, &'static str> {
    let encoded = header
        .as_deref()
        .and_then(|value| value.strip_prefix("Nostr "))
        .ok_or("no authorization")?;
    let json = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "not base64url")?;
    let event = Event::from_json(json).map_err(|_| "not an event")?;
    event.verify().map_err(|_| "bad signature")?;
    if event.kind != Kind::Custom(AUTH_KIND) {
        return Err("wrong kind");
    }
    let tag = |name: &str| {
        event
            .tags
            .iter()
            .map(|tag| tag.as_slice())
            .find(|values| values.first().map(String::as_str) == Some(name))
            .and_then(|values| values.get(1).cloned())
    };
    if tag("t").as_deref() != Some(action) {
        return Err("wrong action");
    }
    if !tag("x").is_some_and(|x| x.eq_ignore_ascii_case(sha256)) {
        return Err("wrong blob");
    }
    let expires = tag("expiration")
        .and_then(|at| at.parse::<u64>().ok())
        .ok_or("no expiration")?;
    if expires <= Timestamp::now().as_secs() {
        return Err("expired");
    }
    Ok(event.pubkey.to_hex())
}

fn reply(status: StatusCode, reason: Option<&str>, body: Vec<u8>) -> Response<Full<Bytes>> {
    let mut response = Response::builder().status(status);
    if let Some(reason) = reason {
        response = response.header("X-Reason", reason);
    }
    response
        .body(Full::new(Bytes::from(body)))
        .expect("response")
}

async fn serve(listener: TcpListener, server: BlossomTestServer) {
    while let Ok((stream, _)) = listener.accept().await {
        let server = server.clone();
        tokio::spawn(async move {
            let service = service_fn(move |request| {
                let server = server.clone();
                async move { Ok::<_, Infallible>(server.handle(request).await) }
            });
            let _ = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await;
        });
    }
}
