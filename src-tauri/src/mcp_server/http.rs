//! The HTTP way (spec 021 US1, US8, T093, contracts/http-auth.md, research R1/R14): a loopback
//! listener for clients that speak only HTTP and for Android/iOS. Every request passes the same
//! checks first: a loopback `Host` with holzi's port (else 421, against DNS rebinding), at most
//! 1 MiB, and no `Origin` on `/mcp` and `/oauth/token` (403, scripts of web pages). `/mcp` needs an
//! access token; rmcp serves the session with the token's agent on the request.

use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::http::{header, HeaderMap, HeaderValue, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::net::TcpListener;
use tower_service::Service as _;

use super::http_session::{HttpAgent, HttpSession, SessionCount};
use super::oauth::OAuthState;
use super::oauth_flow;
use super::oauth_http;
use super::server::Core;

/// The largest request body (contracts/http-auth.md).
pub const MAX_BODY_BYTES: usize = 1024 * 1024;
/// Open MCP sessions at a time (contracts/http-auth.md).
pub const MAX_SESSIONS: usize = 16;
/// The paths a web page's script must not reach (contracts/http-auth.md).
const NO_ORIGIN_PATHS: &[&str] = &["/mcp", "/oauth/token"];

pub type Body = BoxBody<Bytes, Infallible>;

/// What every request of the HTTP way shares.
pub(super) struct HttpContext {
    pub(super) core: Arc<Core>,
    pub(super) oauth: Arc<OAuthState>,
    pub(super) port: u16,
    mcp: StreamableHttpService<HttpSession, LocalSessionManager>,
}

/// Binds the loopback listener on `port`; when another program holds it, on a free one (the
/// caller keeps the new port). Port 0 picks a free one.
pub async fn bind(port: u16) -> std::io::Result<TcpListener> {
    match TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await {
        Err(error) if port != 0 && error.kind() == std::io::ErrorKind::AddrInUse => {
            log::warn!("mcp server: port {port} is taken, the HTTP way moves");
            TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await
        }
        bound => bound,
    }
}

/// Whether `host` names holzi's listener on the loopback (`Host`, RFC 9110 §7.2).
pub fn host_allowed(host: Option<&str>, port: u16) -> bool {
    let Some(host) = host else {
        return false;
    };
    [
        format!("127.0.0.1:{port}"),
        format!("[::1]:{port}"),
        format!("localhost:{port}"),
    ]
    .iter()
    .any(|allowed| allowed.eq_ignore_ascii_case(host))
}

/// Whether a request with an `Origin` header is refused on `path`.
pub fn origin_refused(path: &str, has_origin: bool) -> bool {
    has_origin && NO_ORIGIN_PATHS.contains(&path)
}

/// Whether the announced body is over [`MAX_BODY_BYTES`].
pub fn too_large(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_BODY_BYTES as u64)
}

/// Serves the HTTP way on `listener` until the server stops.
pub(super) async fn serve(core: Arc<Core>, oauth: Arc<OAuthState>, listener: TcpListener) {
    let port = match listener.local_addr() {
        Ok(address) => address.port(),
        Err(error) => {
            log::warn!("mcp server: the HTTP way has no address: {error}");
            return;
        }
    };
    let manager = Arc::new(LocalSessionManager::default());
    let sessions = SessionCount::default();
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts([
            format!("127.0.0.1:{port}"),
            format!("[::1]:{port}"),
            format!("localhost:{port}"),
        ])
        .enforce_origin_validation()
        .with_legacy_session_mode(true)
        .with_max_request_body_bytes(MAX_BODY_BYTES)
        .with_cancellation_token(core.token.child_token());
    let factory = {
        let (core, manager, sessions) = (Arc::clone(&core), Arc::clone(&manager), sessions.clone());
        move || HttpSession::new(&core, &manager, &sessions, MAX_SESSIONS)
    };
    let context = Arc::new(HttpContext {
        core: Arc::clone(&core),
        oauth,
        port,
        mcp: StreamableHttpService::new(factory, manager, config),
    });
    log::info!("mcp server: the HTTP way listens on port {port}");
    loop {
        let accepted = tokio::select! {
            _ = core.token.cancelled() => break,
            accepted = listener.accept() => accepted,
        };
        let stream = match accepted {
            Ok((stream, _)) => stream,
            Err(error) => {
                log::warn!("mcp server: HTTP accept failed: {error}");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let (context, token) = (Arc::clone(&context), core.token.clone());
        tokio::spawn(async move {
            let service = hyper::service::service_fn(move |request| {
                let context = Arc::clone(&context);
                async move { Ok::<_, Infallible>(route(context, request).await) }
            });
            let connection = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service);
            tokio::pin!(connection);
            tokio::select! {
                _ = connection.as_mut() => {}
                _ = token.cancelled() => connection.as_mut().graceful_shutdown(),
            }
        });
    }
}

async fn route(context: Arc<HttpContext>, request: Request<Incoming>) -> Response<Body> {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    if !host_allowed(host.as_deref(), context.port) {
        return status(StatusCode::MISDIRECTED_REQUEST);
    }
    if too_large(request.headers()) {
        return status(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let path = request.uri().path().to_owned();
    if origin_refused(&path, request.headers().contains_key(header::ORIGIN)) {
        return status(StatusCode::FORBIDDEN);
    }
    let base = format!("http://{}", host.unwrap_or_default());
    let query = request.uri().query().unwrap_or_default().to_owned();
    match (request.method().clone(), path.as_str()) {
        (
            Method::GET,
            "/.well-known/oauth-protected-resource" | "/.well-known/oauth-protected-resource/mcp",
        ) => json(StatusCode::OK, &oauth_http::resource_metadata(&base)),
        (Method::GET, "/.well-known/oauth-authorization-server") => {
            json(StatusCode::OK, &oauth_http::server_metadata(&base))
        }
        (Method::POST, "/oauth/register") => match read_body(request).await {
            Some(body) => oauth_http::register(&context, &body),
            None => status(StatusCode::PAYLOAD_TOO_LARGE),
        },
        (Method::GET, "/oauth/authorize") => oauth_http::authorize(&context, &query),
        (Method::GET, "/oauth/authorize/wait") => oauth_http::wait(&context, &query),
        (Method::POST, "/oauth/token") => match read_body(request).await {
            Some(body) => oauth_http::token(&context, &body).await,
            None => status(StatusCode::PAYLOAD_TOO_LARGE),
        },
        (_, "/mcp") => mcp(&context, &base, request).await,
        _ => status(StatusCode::NOT_FOUND),
    }
}

/// `/mcp`: an access token of a registered agent, then rmcp with the agent on the request.
async fn mcp(context: &HttpContext, base: &str, mut request: Request<Incoming>) -> Response<Body> {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim);
    let agent = token.and_then(|token| context.oauth.agent_of(token, Instant::now()));
    let agent = match agent {
        Some(agent) if oauth_flow::still_registered(&context.core, agent).await => agent,
        Some(agent) => {
            context.oauth.forget_agent(agent);
            return unauthorized(base, true);
        }
        None => return unauthorized(base, token.is_some()),
    };
    request.extensions_mut().insert(HttpAgent(agent));
    let mut service = context.mcp.clone();
    match service.call(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    }
}

fn unauthorized(base: &str, invalid: bool) -> Response<Body> {
    let mut challenge =
        format!("Bearer resource_metadata=\"{base}/.well-known/oauth-protected-resource\"");
    if invalid {
        challenge.push_str(", error=\"invalid_token\"");
    }
    let mut response = status(StatusCode::UNAUTHORIZED);
    if let Ok(value) = HeaderValue::from_str(&challenge) {
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, value);
    }
    response
}

/// The body of a request, at most [`MAX_BODY_BYTES`] even when no length was announced.
async fn read_body(request: Request<Incoming>) -> Option<Bytes> {
    Limited::new(request.into_body(), MAX_BODY_BYTES)
        .collect()
        .await
        .ok()
        .map(|collected| collected.to_bytes())
}

pub(super) fn respond(
    code: StatusCode,
    content_type: &'static str,
    body: impl Into<Bytes>,
) -> Response<Body> {
    let mut response = Response::new(Full::new(body.into()).boxed());
    *response.status_mut() = code;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub(super) fn status(code: StatusCode) -> Response<Body> {
    respond(code, "text/plain; charset=utf-8", Bytes::new())
}

pub(super) fn json(code: StatusCode, value: &serde_json::Value) -> Response<Body> {
    respond(code, "application/json", value.to_string())
}
