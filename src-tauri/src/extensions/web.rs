//! Network of extensions (spec 017, US8, T098, FR-050, FR-051, contracts/bridge.md, research R18).
//!
//! An extension frame has no network of its own (CSP, sandbox); every request goes through holzi
//! and needs a `web` permission for its method and address. holzi follows redirects itself, at
//! most [`MAX_REDIRECTS`], and checks every target with the same rule, so a granted host cannot
//! hand the request on to one that is not. When the origin changes, `Authorization`, `Cookie` and
//! `Proxy-Authorization` are dropped. Only `http` and `https`; run time and answer size come from
//! the extension's limits.

use std::sync::OnceLock;
use std::time::Duration;

use base64::Engine;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Method, StatusCode, Url};
use serde_json::{json, Map, Value};

use crate::extensions::bridge::blocking::block_on;
use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::permissions::store::candidates;
use crate::extensions::permissions::{
    evaluate, Action, Decision, Permission, PermissionKind, PermissionRequest, RequestTarget,
    Target, WebRequest,
};
use crate::extensions::sql::exec::{limits_of, Limits};

pub const MODULE: &str = module_path!();

/// Redirects holzi follows for one request (research R18).
pub const MAX_REDIRECTS: usize = 10;

/// Request headers the extension cannot set: the connection is holzi's.
const FIXED_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "upgrade",
    "keep-alive",
    "te",
    "trailer",
];

/// Credentials that never travel to another origin.
const CREDENTIAL_HEADERS: &[&str] = &["authorization", "cookie", "proxy-authorization"];

fn invalid(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Validation, message)
}

fn web_error(message: impl Into<String>) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Web, message)
}

fn limit(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::LimitExceeded, message)
}

/// One client for every extension: no redirects of its own, no cookie store.
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("haex/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("the HTTP client of extensions builds with static settings")
    })
}

/// What a question about `url` asks for: its whole origin (`scheme://host[:port]/*`), so one
/// answer covers the paths of a service instead of asking for every icon.
pub fn prompt_target(url: &WebRequest) -> String {
    match url.port {
        Some(port) => format!("{}://{}:{port}/*", url.scheme, url.host),
        None => format!("{}://{}/*", url.scheme, url.host),
    }
}

/// The `web` permissions of the calling extension on this device: remembered and held in memory.
fn grants(ctx: &CallContext) -> Result<Vec<Permission>, BridgeError> {
    let (extension_id, device) = (ctx.session.extension_id, ctx.device);
    let mut grants = ctx
        .db
        .read_blocking(move |q| {
            candidates(q, extension_id, PermissionKind::Web, device).map_err(Into::into)
        })
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))?;
    grants.extend(
        ctx.host
            .permissions
            .temporary(extension_id, PermissionKind::Web),
    );
    Ok(grants)
}

fn decide(
    grants: &[Permission],
    device: uuid::Uuid,
    method: &Method,
    url: &WebRequest,
) -> Decision {
    let request = PermissionRequest {
        kind: PermissionKind::Web,
        action: Action::Method(method.as_str().to_owned()),
        target: RequestTarget::Url(url.clone()),
    };
    evaluate(grants, &request, device)
}

/// Methods the SDK may send again after a question: repeating them changes nothing on the server
/// (RFC 9110 9.2.2; `PROPFIND` and `REPORT` of WebDAV only read).
const REPEATABLE: &[&str] = &[
    "GET", "HEAD", "OPTIONS", "TRACE", "PUT", "DELETE", "PROPFIND", "REPORT",
];

/// Whether a request with `method` may be sent again from the start.
fn repeatable(method: &Method) -> bool {
    REPEATABLE.contains(&method.as_str())
}

/// Allowed, or the 1002/1004 answer with `{resourceType, action, target}`. A target no answer
/// could ever cover (holzi cannot store it) is refused instead of asked.
fn check(
    grants: &[Permission],
    device: uuid::Uuid,
    method: &Method,
    url: &WebRequest,
) -> Result<(), BridgeError> {
    let target = prompt_target(url);
    let code = match decide(grants, device, method, url) {
        Decision::Allow => return Ok(()),
        Decision::Prompt if Target::parse(PermissionKind::Web, &target).is_some() => {
            ExtensionErrorCode::PermissionPromptRequired
        }
        Decision::Prompt | Decision::Deny => ExtensionErrorCode::PermissionDenied,
    };
    Err(
        BridgeError::new(code, "permission required").with_details(json!({
            "resourceType": "web",
            "action": method.as_str(),
            "target": target,
        })),
    )
}

/// An `http`/`https` address.
fn parse_url(value: &str) -> Result<(Url, WebRequest), BridgeError> {
    let url = Url::parse(value).map_err(|_| invalid("url is not an address"))?;
    let web = WebRequest::parse(url.as_str()).ok_or_else(|| invalid("only http and https"))?;
    Ok((url, web))
}

fn text<'a>(params: &'a Value, name: &str) -> Result<&'a str, BridgeError> {
    params
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(&format!("{name} must be a string")))
}

/// A request as the SDK sends it: `{url, method?, headers?, body? (base64), timeout? (ms)}`.
struct Request {
    method: Method,
    url: Url,
    web: WebRequest,
    headers: HeaderMap,
    body: Option<Vec<u8>>,
    timeout: Duration,
}

fn parse_headers(params: &Value) -> Result<HeaderMap, BridgeError> {
    let mut headers = HeaderMap::new();
    let Some(given) = params.get("headers").filter(|h| !h.is_null()) else {
        return Ok(headers);
    };
    let given = given
        .as_object()
        .ok_or_else(|| invalid("headers must be an object"))?;
    for (name, value) in given {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| invalid("header name not allowed"))?;
        if FIXED_HEADERS.contains(&name.as_str()) {
            return Err(invalid("header set by holzi"));
        }
        let value = value
            .as_str()
            .and_then(|v| HeaderValue::from_str(v).ok())
            .ok_or_else(|| invalid("header value must be a string"))?;
        headers.append(name, value);
    }
    Ok(headers)
}

/// `method` of `params` (default `GET`): one a permission can name, not `*`.
fn parse_method(params: &Value) -> Option<Method> {
    let Some(method) = params.get("method").and_then(Value::as_str) else {
        return Some(Method::GET);
    };
    let method = method.to_ascii_uppercase();
    Action::parse(PermissionKind::Web, &method)
        .filter(|a| *a != Action::AnyMethod)
        .and_then(|_| Method::from_bytes(method.as_bytes()).ok())
}

fn parse_request(params: &Value, limits: &Limits) -> Result<Request, BridgeError> {
    let (url, web) = parse_url(text(params, "url")?)?;
    let method = parse_method(params).ok_or_else(|| invalid("method not allowed"))?;
    let body = match params.get("body").filter(|b| !b.is_null()) {
        None => None,
        Some(body) => {
            let body = body
                .as_str()
                .ok_or_else(|| invalid("body must be base64"))?;
            if body.len() as u64 > limits.max_response_bytes {
                return Err(limit("request too large"));
            }
            Some(
                base64::engine::general_purpose::STANDARD
                    .decode(body)
                    .map_err(|_| invalid("body must be base64"))?,
            )
        }
    };
    let limit_ms = limits.timeout_ms.max(1);
    let timeout_ms = match params.get("timeout").filter(|t| !t.is_null()) {
        None => limit_ms,
        Some(t) => t
            .as_u64()
            .filter(|t| *t > 0)
            .ok_or_else(|| invalid("timeout must be a positive number"))?
            .min(limit_ms),
    };
    Ok(Request {
        method,
        url,
        web,
        headers: parse_headers(params)?,
        body,
        timeout: Duration::from_millis(timeout_ms),
    })
}

fn same_origin(a: &Url, b: &Url) -> bool {
    a.origin() == b.origin()
}

/// The answer the SDK reads: `{status, statusText, headers, body (base64), url}`.
fn answer(status: StatusCode, headers: &HeaderMap, body: &[u8], url: &Url) -> Value {
    let mut map = Map::new();
    for name in headers.keys() {
        let joined = headers
            .get_all(name)
            .iter()
            .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned())
            .collect::<Vec<_>>()
            .join(", ");
        map.insert(name.as_str().to_owned(), Value::String(joined));
    }
    json!({
        "status": status.as_u16(),
        "statusText": status.canonical_reason().unwrap_or(""),
        "headers": map,
        "body": base64::engine::general_purpose::STANDARD.encode(body),
        "url": url.as_str(),
    })
}

/// The statuses holzi follows, as `fetch` does; another 3xx with a `Location` (300, 304) is the
/// answer.
fn is_redirect(status: StatusCode) -> bool {
    matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
}

/// The method after a redirect: `303` turns everything but `HEAD` into `GET`, and so do `301`
/// and `302` for a `POST` (as browsers do); `307` and `308` keep method and body.
fn method_after(status: StatusCode, method: &Method) -> Method {
    match status.as_u16() {
        303 if *method != Method::HEAD => Method::GET,
        301 | 302 if *method == Method::POST => Method::GET,
        _ => method.clone(),
    }
}

/// Sends `request`, following redirects that `allowed` lets through, and reads the answer up to
/// `max_body` bytes. After a question (1004) the SDK sends the whole request again, so a redirect
/// target that would need one is only asked for when the first method is [`repeatable`];
/// otherwise the redirect itself is the answer (as `fetch` with `redirect: "manual"`), and the
/// extension follows it with a request of its own. A `POST` thus reaches the first server once.
async fn send(
    mut request: Request,
    max_body: usize,
    allowed: impl Fn(&Method, &WebRequest) -> Result<(), BridgeError>,
) -> Result<Value, BridgeError> {
    let deadline = tokio::time::Instant::now() + request.timeout;
    let first_repeatable = repeatable(&request.method);
    allowed(&request.method, &request.web)?;
    for hop in 0..=MAX_REDIRECTS {
        let mut builder = client()
            .request(request.method.clone(), request.url.clone())
            .headers(request.headers.clone());
        if let Some(body) = &request.body {
            builder = builder.body(body.clone());
        }
        let response = tokio::time::timeout_at(deadline, builder.send())
            .await
            .map_err(|_| timed_out())?
            .map_err(|e| web_error(format!("request failed: {}", without_url(&e))))?;
        let status = response.status();
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|l| l.to_str().ok())
            .map(str::to_owned);
        if let (true, Some(location)) = (is_redirect(status), location) {
            if hop == MAX_REDIRECTS {
                return Err(web_error("too many redirects"));
            }
            let next = request
                .url
                .join(&location)
                .map_err(|_| web_error("redirect to an invalid address"))?;
            let web = WebRequest::parse(next.as_str())
                .ok_or_else(|| web_error("redirect to a scheme other than http or https"))?;
            let method = method_after(status, &request.method);
            match allowed(&method, &web) {
                Ok(()) => {}
                Err(error)
                    if !first_repeatable
                        && error.code == ExtensionErrorCode::PermissionPromptRequired =>
                {
                    return read_answer(response, deadline, max_body).await;
                }
                Err(error) => return Err(error),
            }
            if !same_origin(&request.url, &next) {
                for name in CREDENTIAL_HEADERS {
                    request.headers.remove(*name);
                }
            }
            if method != request.method {
                request.body = None;
                request.headers.remove(reqwest::header::CONTENT_TYPE);
            }
            request.method = method;
            request.url = next;
            request.web = web;
            continue;
        }
        return read_answer(response, deadline, max_body).await;
    }
    Err(web_error("too many redirects"))
}

/// The error for a request that ran past its time limit.
fn timed_out() -> BridgeError {
    limit("time limit exceeded")
}

/// Reads `response` up to `max_body` bytes before `deadline` into the answer of `fetch`.
async fn read_answer(
    mut response: reqwest::Response,
    deadline: tokio::time::Instant,
    max_body: usize,
) -> Result<Value, BridgeError> {
    let status = response.status();
    let headers = response.headers().clone();
    let url = response.url().clone();
    let mut body = Vec::new();
    while let Some(chunk) = tokio::time::timeout_at(deadline, response.chunk())
        .await
        .map_err(|_| timed_out())?
        .map_err(|e| web_error(format!("reading the answer failed: {}", without_url(&e))))?
    {
        if body.len() + chunk.len() > max_body {
            return Err(limit("answer too large"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(answer(status, &headers, &body, &url))
}

/// What went wrong, without reqwest's text: it names addresses, also of redirect targets.
fn without_url(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "timed out".to_owned()
    } else if error.is_connect() {
        "could not connect".to_owned()
    } else {
        "network error".to_owned()
    }
}

/// The largest body whose base64 form fits into `max_response_bytes`.
fn max_body(limits: &Limits) -> usize {
    usize::try_from(limits.max_response_bytes / 4 * 3).unwrap_or(usize::MAX)
}

fn limits(ctx: &CallContext) -> Result<Limits, BridgeError> {
    let extension_id = ctx.session.extension_id;
    ctx.db
        .read_blocking(move |q| limits_of(q, extension_id))
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))
}

/// `extension_web_fetch`: `{url, method?, headers?, body?, timeout?}` →
/// `{status, statusText, headers, body, url}` (FR-050).
pub fn fetch(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let limits = limits(ctx)?;
    let request = parse_request(params, &limits)?;
    let grants = grants(ctx)?;
    let device = ctx.device;
    block_on(send(request, max_body(&limits), |method, url| {
        check(&grants, device, method, url)
    }))
}

/// `extension_web_open`: `{url}`, opened in the system's browser after the check for `GET` on
/// that address (FR-051).
pub fn open(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let (url, web) = parse_url(text(params, "url")?)?;
    check(&grants(ctx)?, ctx.device, &Method::GET, &web)?;
    ctx.host
        .desktop()
        .ok_or_else(BridgeError::not_available)?
        .open_url(url.as_str())
        .map_err(|_| web_error("could not open the address"))?;
    Ok(Value::Null)
}

/// `extension_permissions_check_web`: `{url, method?}` → `{status: granted | denied | ask}`;
/// grants nothing.
pub fn check_web(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let Ok((_, web)) = parse_url(text(params, "url")?) else {
        return Ok(json!({ "status": "denied" }));
    };
    // A method `fetch` refuses is never granted.
    let Some(method) = parse_method(params) else {
        return Ok(json!({ "status": "denied" }));
    };
    Ok(json!({
        "status": match decide(&grants(ctx)?, ctx.device, &method, &web) {
            Decision::Allow => "granted",
            Decision::Deny => "denied",
            Decision::Prompt => "ask",
        }
    }))
}

#[cfg(test)]
#[path = "web_tests.rs"]
mod tests;
