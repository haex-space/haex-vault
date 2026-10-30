//! The HTTP side of the marketplace protocol (spec 069, research R5, R6; contracts/marketplace-
//! api.md): one `GET` with the few headers the contract allows, a size-bounded JSON answer, and
//! the failures of a source reduced to a kind and a local detail.
//!
//! The answer of a marketplace is never put into an error: it may echo the `Authorization` header
//! (SC-004). A login travels only over `https`, only to the host of the marketplace address, and
//! is dropped when a redirect leaves that origin (FR-027).

use std::fmt;
use std::sync::OnceLock;
use std::time::Duration;

use base64::Engine as _;
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
use reqwest::redirect::{Attempt, Policy};
use serde::de::DeserializeOwned;
use url::Url;
use zeroize::Zeroizing;

use super::model::SourceErrorKind;
use super::store::login_transport_ok;
use super::{app_version, platform, CONNECT_TIMEOUT, SOURCE_TIMEOUT};

/// The most redirects a request follows.
const MAX_REDIRECTS: usize = 5;

/// The largest JSON answer the host reads.
const MAX_JSON_BYTES: usize = 4 * 1024 * 1024;

/// A value of the `Authorization` header (`Bearer <token>` or `Basic <base64>`). Holds a secret:
/// it is zeroed when dropped and neither `Debug` nor `Display` shows it.
#[derive(Clone)]
pub struct AuthHeader(Zeroizing<String>);

impl AuthHeader {
    pub fn bearer(token: &str) -> Self {
        Self(Zeroizing::new(format!("Bearer {token}")))
    }

    pub fn basic(user: &str, password: &str) -> Self {
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));
        Self(Zeroizing::new(format!("Basic {encoded}")))
    }

    /// The header value, for the tests of other modules that check what an entry makes of itself.
    #[cfg(test)]
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }

    fn header_value(&self) -> Option<HeaderValue> {
        let mut value = HeaderValue::from_str(&self.0).ok()?;
        value.set_sensitive(true);
        Some(value)
    }
}

impl fmt::Debug for AuthHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthHeader(..)")
    }
}

/// Why a call to one source failed: a kind and a short, locally generated detail (`HTTP 500`,
/// `connection failed`), never text of the marketplace, an address or a header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFailure {
    pub kind: SourceErrorKind,
    pub detail: Option<String>,
}

impl SourceFailure {
    fn new(kind: SourceErrorKind, detail: &str) -> Self {
        Self {
            kind,
            detail: Some(detail.to_owned()),
        }
    }
}

/// `base` with the protocol path `route` behind its path prefix, and the query pairs encoded.
pub fn join_url(base: &str, route: &str, query: &[(&str, String)]) -> Result<Url, SourceFailure> {
    let joined = format!(
        "{}/{}",
        base.trim_end_matches('/'),
        route.trim_start_matches('/')
    );
    let mut url = Url::parse(&joined)
        .map_err(|_| SourceFailure::new(SourceErrorKind::BadResponse, "invalid address"))?;
    if !query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in query {
            pairs.append_pair(key, value);
        }
    }
    Ok(url)
}

/// Whether a redirect from `previous` (the chain so far, the first entry being the request) to
/// `next` may be followed. Never more than [`MAX_REDIRECTS`], never from `https` to `http`, and
/// with a login in the request never to another origin (scheme, host or port).
pub fn redirect_allowed(previous: &[Url], next: &Url, carries_login: bool) -> bool {
    let Some(last) = previous.last() else {
        return false;
    };
    if previous.len() > MAX_REDIRECTS {
        return false;
    }
    if last.scheme() == "https" && next.scheme() == "http" {
        return false;
    }
    if carries_login {
        let Some(first) = previous.first() else {
            return false;
        };
        return first.origin() == next.origin();
    }
    true
}

pub(super) fn policy(carries_login: bool) -> Policy {
    Policy::custom(move |attempt: Attempt<'_>| {
        if redirect_allowed(attempt.previous(), attempt.url(), carries_login) {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

fn build(carries_login: bool) -> reqwest::Client {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert("x-platform", HeaderValue::from_static(platform()));
    if let Ok(version) = HeaderValue::from_str(&app_version().to_string()) {
        headers.insert("x-app-version", version);
    }
    reqwest::Client::builder()
        .user_agent(format!("HaexVault/{}", app_version()))
        .default_headers(headers)
        .redirect(policy(carries_login))
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// One client for requests without a login, one with: their redirect rules differ.
fn shared(carries_login: bool) -> &'static reqwest::Client {
    static PLAIN: OnceLock<reqwest::Client> = OnceLock::new();
    static LOGIN: OnceLock<reqwest::Client> = OnceLock::new();
    if carries_login {
        LOGIN.get_or_init(|| build(true))
    } else {
        PLAIN.get_or_init(|| build(false))
    }
}

/// Maps a failed send to a kind and a detail of the fixed vocabulary.
pub(super) fn send_failure(error: &reqwest::Error) -> SourceFailure {
    if error.is_timeout() {
        SourceFailure::new(SourceErrorKind::Timeout, "timeout")
    } else if error.is_connect() {
        SourceFailure::new(SourceErrorKind::Unreachable, "connection failed")
    } else if error.is_redirect() {
        SourceFailure::new(SourceErrorKind::Unreachable, "redirect refused")
    } else {
        SourceFailure::new(SourceErrorKind::Unreachable, "request failed")
    }
}

/// Maps a status that is not a success.
pub(super) fn status_failure(status: reqwest::StatusCode) -> SourceFailure {
    let kind = if matches!(status.as_u16(), 401 | 403) {
        SourceErrorKind::AuthRejected
    } else {
        SourceErrorKind::Unreachable
    };
    SourceFailure::new(kind, &format!("HTTP {}", status.as_u16()))
}

/// The calls of the protocol, with a limit on how long one may take.
#[derive(Debug, Clone, Copy)]
pub struct Http {
    pub timeout: Duration,
}

impl Default for Http {
    fn default() -> Self {
        Self {
            timeout: SOURCE_TIMEOUT,
        }
    }
}

impl Http {
    /// A client with another limit; the tests use a short one against a server that never answers.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// `GET base/route?query` and the answer as `T`. The `Authorization` header is sent only when
    /// `auth` is given **and** `base` is `https` (or loopback in a debug build, see
    /// [`login_transport_ok`]); a login with any other `http` fails before anything is
    /// sent (`insecure_credentials`, FR-027).
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        base: &str,
        route: &str,
        query: &[(&str, String)],
        auth: Option<&AuthHeader>,
    ) -> Result<T, SourceFailure> {
        let url = join_url(base, route, query)?;
        let mut request = shared(auth.is_some())
            .get(url.clone())
            .timeout(self.timeout);
        if let Some(auth) = auth {
            if !login_transport_ok(url.as_str()) {
                return Err(SourceFailure::new(
                    SourceErrorKind::InsecureCredentials,
                    "login over http",
                ));
            }
            let value = auth.header_value().ok_or_else(|| {
                SourceFailure::new(SourceErrorKind::CredentialsMissing, "unusable credentials")
            })?;
            request = request.header(AUTHORIZATION, value);
        }
        let response = request.send().await.map_err(|e| send_failure(&e))?;
        let status = response.status();
        if !status.is_success() {
            // The body is dropped unread: it may echo the login.
            return Err(status_failure(status));
        }
        let mut stream = response.bytes_stream();
        let mut body = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| send_failure(&e))?;
            if body.len() + chunk.len() > MAX_JSON_BYTES {
                return Err(SourceFailure::new(
                    SourceErrorKind::BadResponse,
                    "answer too large",
                ));
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body)
            .map_err(|_| SourceFailure::new(SourceErrorKind::BadResponse, "invalid answer"))
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
