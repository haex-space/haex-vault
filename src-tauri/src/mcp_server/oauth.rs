//! holzi's own OAuth 2.1 server for the HTTP way (spec 021 US1, US8, T094, contracts/http-auth.md,
//! research R1/R2): the part without database or network. Dynamic client registration only
//! remembers a client, it creates no agent; an authorization waits for the registration dialog;
//! its code binds the client, the accepted redirect URI, the PKCE challenge and the agent; access
//! tokens live one hour in memory. The refresh token is the agent's credential (`token_hash`) and
//! lives in the database (`oauth_flow.rs`).
//!
//! ponytail: registered clients live in memory only, so after a restart a client that still has to
//! authorize registers again (an MCP client does that when its client id is refused). Ceiling: a
//! client that keeps its id across a holzi restart before authorizing gets `invalid_client` once.
//! Upgrade path: persist clients per device next to `mcp.http_port`.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use super::registry::{self, Credential};

/// How long an access token works (contracts/http-auth.md).
pub const ACCESS_TTL: Duration = Duration::from_secs(60 * 60);
/// How long an authorization code may wait for its exchange.
pub const CODE_TTL: Duration = Duration::from_secs(120);
/// Open authorizations at a time (contracts/http-auth.md).
pub const MAX_PENDING: usize = 10;
/// Registered clients kept; the oldest goes first. Registration needs no credential, so it is bounded.
pub const MAX_CLIENTS: usize = 64;
/// How long a decided authorization waits for its page to collect it.
pub const PENDING_TTL: Duration = Duration::from_secs(10 * 60);

/// Schemes a native client may not use as its own (RFC 8252 §7.1 allows private-use schemes).
const REFUSED_SCHEMES: &[&str] = &[
    "https",
    "javascript",
    "data",
    "file",
    "vbscript",
    "about",
    "blob",
    "ftp",
    "ws",
    "wss",
];

/// An OAuth error as the endpoints answer it (RFC 6749 §5.2, RFC 7591 §3.2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthError {
    InvalidRequest,
    InvalidClient,
    InvalidGrant,
    UnsupportedGrantType,
    UnsupportedResponseType,
    InvalidRedirectUri,
    InvalidClientMetadata,
    /// More than [`MAX_PENDING`] open authorizations.
    TooMany,
}

impl OAuthError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::InvalidClient => "invalid_client",
            Self::InvalidGrant => "invalid_grant",
            Self::UnsupportedGrantType => "unsupported_grant_type",
            Self::UnsupportedResponseType => "unsupported_response_type",
            Self::InvalidRedirectUri => "invalid_redirect_uri",
            Self::InvalidClientMetadata => "invalid_client_metadata",
            Self::TooMany => "temporarily_unavailable",
        }
    }

    pub fn status(self) -> u16 {
        match self {
            Self::InvalidClient => 401,
            Self::TooMany => 503,
            _ => 400,
        }
    }
}

/// A client from dynamic registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub id: String,
    pub name: Option<String>,
    pub redirect_uris: Vec<String>,
}

/// A loopback `http` URI (any port) or a native client's own scheme; everything else is refused.
pub fn redirect_allowed(uri: &str) -> bool {
    let Ok(url) = Url::parse(uri) else {
        return false;
    };
    if url.fragment().is_some() {
        return false;
    }
    match url.scheme() {
        "http" => is_loopback(&url),
        scheme => !REFUSED_SCHEMES.contains(&scheme),
    }
}

fn is_loopback(url: &Url) -> bool {
    matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"))
}

/// Whether `asked` is one of the client's `registered` URIs; a loopback URI may differ only by port
/// (RFC 8252 §7.3).
pub fn redirect_matches(registered: &[String], asked: &str) -> bool {
    if registered.iter().any(|uri| uri == asked) {
        return true;
    }
    let Ok(asked) = Url::parse(asked) else {
        return false;
    };
    if asked.scheme() != "http" || !is_loopback(&asked) {
        return false;
    }
    registered
        .iter()
        .filter_map(|uri| Url::parse(uri).ok())
        .any(|uri| {
            uri.scheme() == "http"
                && uri.host_str() == asked.host_str()
                && uri.path() == asked.path()
                && uri.query() == asked.query()
        })
}

/// PKCE S256 (RFC 7636 §4.6); a verifier outside 43–128 unreserved characters never matches.
pub fn verify_pkce(challenge: &str, verifier: &str) -> bool {
    let valid = (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'));
    valid && URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) == challenge
}

/// The parameters of `/oauth/authorize` holzi uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizeRequest {
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub state: Option<String>,
}

/// Reads `/oauth/authorize`'s query: `response_type=code`, PKCE with S256 required.
pub fn parse_authorize(query: &str) -> Result<AuthorizeRequest, OAuthError> {
    let fields: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    let field = |name: &str| fields.get(name).filter(|value| !value.is_empty()).cloned();
    if field("response_type").as_deref() != Some("code") {
        return Err(OAuthError::UnsupportedResponseType);
    }
    if field("code_challenge_method").as_deref() != Some("S256") {
        return Err(OAuthError::InvalidRequest);
    }
    Ok(AuthorizeRequest {
        client_id: field("client_id").ok_or(OAuthError::InvalidRequest)?,
        redirect_uri: field("redirect_uri").ok_or(OAuthError::InvalidRequest)?,
        code_challenge: field("code_challenge").ok_or(OAuthError::InvalidRequest)?,
        state: field("state"),
    })
}

/// How the user decided an authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The agent registered; the code carries its credential.
    Granted {
        agent: Uuid,
    },
    Denied,
}

/// What the page of a pending authorization gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Collected {
    Waiting,
    /// Where the browser goes: the client's redirect URI with `code` or `error`, and `state`.
    Redirect(String),
    Unknown,
}

struct Pending {
    request: AuthorizeRequest,
    decision: Option<(Decision, Option<Credential>)>,
    since: Instant,
}

struct CodeGrant {
    client_id: String,
    redirect_uri: String,
    challenge: String,
    agent: Uuid,
    refresh: Credential,
    until: Instant,
}

struct AccessGrant {
    agent: Uuid,
    until: Instant,
}

#[derive(Default)]
struct Inner {
    clients: HashMap<String, Client>,
    order: VecDeque<String>,
    pending: HashMap<String, Pending>,
    codes: HashMap<String, CodeGrant>,
    access: HashMap<String, AccessGrant>,
}

/// What a code exchange hands out.
#[derive(Debug)]
pub struct Exchanged {
    pub agent: Uuid,
    pub refresh: Credential,
}

/// The in-memory state of the OAuth server.
#[derive(Default)]
pub struct OAuthState {
    inner: Mutex<Inner>,
}

impl OAuthState {
    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Dynamic client registration: remembers the client, creates no agent.
    pub fn register_client(
        &self,
        name: Option<String>,
        redirect_uris: Vec<String>,
    ) -> Result<Client, OAuthError> {
        if redirect_uris.is_empty() {
            return Err(OAuthError::InvalidRedirectUri);
        }
        if !redirect_uris.iter().all(|uri| redirect_allowed(uri)) {
            return Err(OAuthError::InvalidRedirectUri);
        }
        let id = random()?;
        let client = Client {
            id: id.clone(),
            name: name.map(|name| registry::kept_name(&name)),
            redirect_uris,
        };
        let mut inner = self.inner();
        while inner.order.len() >= MAX_CLIENTS {
            if let Some(oldest) = inner.order.pop_front() {
                inner.clients.remove(&oldest);
            }
        }
        inner.order.push_back(id.clone());
        inner.clients.insert(id, client.clone());
        Ok(client)
    }

    pub fn client(&self, id: &str) -> Option<Client> {
        self.inner().clients.get(id).cloned()
    }

    /// Checks an authorization against its client and opens it; returns its id and the client.
    pub fn begin(
        &self,
        request: AuthorizeRequest,
        now: Instant,
    ) -> Result<(String, Client), OAuthError> {
        let client = self
            .client(&request.client_id)
            .ok_or(OAuthError::InvalidClient)?;
        if !redirect_matches(&client.redirect_uris, &request.redirect_uri) {
            return Err(OAuthError::InvalidRedirectUri);
        }
        let id = random()?;
        let mut inner = self.inner();
        inner
            .pending
            .retain(|_, pending| now.duration_since(pending.since) < PENDING_TTL);
        if inner.pending.len() >= MAX_PENDING {
            return Err(OAuthError::TooMany);
        }
        inner.pending.insert(
            id.clone(),
            Pending {
                request,
                decision: None,
                since: now,
            },
        );
        Ok((id, client))
    }

    /// The user decided; a grant brings the new credential the code hands out.
    pub fn settle(&self, id: &str, decision: Decision, refresh: Option<Credential>) {
        if let Some(pending) = self.inner().pending.get_mut(id) {
            pending.decision = Some((decision, refresh));
        }
    }

    /// What the page of authorization `id` shows; a decided one is collected once.
    pub fn collect(&self, id: &str, now: Instant) -> Result<Collected, OAuthError> {
        let mut inner = self.inner();
        match inner.pending.get(id) {
            None => return Ok(Collected::Unknown),
            Some(pending) if pending.decision.is_none() => return Ok(Collected::Waiting),
            Some(_) => {}
        }
        let Some(Pending {
            request,
            decision: Some((decision, refresh)),
            ..
        }) = inner.pending.remove(id)
        else {
            return Ok(Collected::Unknown);
        };
        let mut url =
            Url::parse(&request.redirect_uri).map_err(|_| OAuthError::InvalidRedirectUri)?;
        {
            let mut query = url.query_pairs_mut();
            match (decision, refresh) {
                (Decision::Granted { agent }, Some(refresh)) => {
                    let code = random()?;
                    query.append_pair("code", &code);
                    inner.codes.retain(|_, grant| grant.until > now);
                    inner.codes.insert(
                        registry::hash(&code),
                        CodeGrant {
                            client_id: request.client_id.clone(),
                            redirect_uri: request.redirect_uri.clone(),
                            challenge: request.code_challenge.clone(),
                            agent,
                            refresh,
                            until: now + CODE_TTL,
                        },
                    );
                }
                _ => {
                    query.append_pair("error", "access_denied");
                }
            }
            if let Some(state) = &request.state {
                query.append_pair("state", state);
            }
        }
        Ok(Collected::Redirect(url.into()))
    }

    /// `authorization_code`: the code is used up by any attempt; client, redirect URI and PKCE
    /// verifier must match what the authorization bound.
    pub fn exchange(
        &self,
        code: &str,
        client_id: &str,
        redirect_uri: &str,
        verifier: &str,
        now: Instant,
    ) -> Result<Exchanged, OAuthError> {
        let grant = self
            .inner()
            .codes
            .remove(&registry::hash(code))
            .ok_or(OAuthError::InvalidGrant)?;
        let valid = grant.until > now
            && grant.client_id == client_id
            && grant.redirect_uri == redirect_uri
            && verify_pkce(&grant.challenge, verifier);
        if !valid {
            return Err(OAuthError::InvalidGrant);
        }
        Ok(Exchanged {
            agent: grant.agent,
            refresh: grant.refresh,
        })
    }

    /// A new access token for `agent`.
    pub fn issue_access(&self, agent: Uuid, now: Instant) -> Result<String, OAuthError> {
        let token = random()?;
        let mut inner = self.inner();
        inner.access.retain(|_, grant| grant.until > now);
        inner.access.insert(
            registry::hash(&token),
            AccessGrant {
                agent,
                until: now + ACCESS_TTL,
            },
        );
        Ok(token)
    }

    /// The agent of a live access token.
    pub fn agent_of(&self, token: &str, now: Instant) -> Option<Uuid> {
        self.inner()
            .access
            .get(&registry::hash(token))
            .filter(|grant| grant.until > now)
            .map(|grant| grant.agent)
    }

    /// Forgets the access tokens of `agent` (revocation, a rotated refresh token).
    pub fn forget_agent(&self, agent: Uuid) {
        self.inner().access.retain(|_, grant| grant.agent != agent);
    }

    /// Open authorizations now.
    pub fn pending_count(&self) -> usize {
        self.inner().pending.len()
    }
}

fn random() -> Result<String, OAuthError> {
    Credential::generate()
        .map(Credential::into_string)
        .map_err(|_| OAuthError::TooMany)
}
