//! The Blossom client of the chat (spec 072 research R12, contracts/blossom.md; BUD-01, BUD-02,
//! BUD-06): ask before uploading (`HEAD /upload`), upload, download and delete blobs. Every call that
//! needs a key carries a kind 24242 event signed by the key of that blob.
//!
//! Requests go through the storages' pinned client: the host is reached only at the addresses that
//! were checked, without redirects or proxies. A server address must be `https`, or `http` to
//! `127.0.0.1` (a server on this machine, the tests). A download address comes from another vault's
//! message, so it must lie on a public host unless it is on the user's own server; a message cannot
//! make this device reach into its local network.

use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use nostr::event::{EventBuilder, FinalizeEvent, Kind, Tag};
use nostr::key::Keys;
use nostr::types::Timestamp;
use reqwest::{Method, StatusCode, Url};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::time::Instant;

use crate::remote_storage::address::{self, Resolver};
use crate::remote_storage::http::pinned_client;
use crate::remote_storage::EndpointScope;
use crate::sync::keys::hex;

use super::crypto::{ciphertext_len, MAX_PLAINTEXT};

/// The kind of Blossom's authorization events.
pub const AUTH_KIND: u16 = 24242;
/// How long an authorization event is valid.
const AUTH_VALID: Duration = Duration::from_secs(10 * 60);
/// The most a short request (ask, delete) may take, connecting included.
const SHORT: Duration = Duration::from_secs(30);
/// The most one upload or download may take.
const TRANSFER: Duration = Duration::from_secs(5 * 60);

/// Why a Blossom request did not go through.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BlossomError {
    /// Not an `https` address (or `http` to `127.0.0.1`), or one on a host it may not reach.
    #[error("the Blossom-Server address is not allowed")]
    NotAllowed,
    /// The server refused with this status and, if it gave one, its reason (`X-Reason`).
    #[error("the Blossom-Server refused ({status})")]
    Refused { status: u16, reason: Option<String> },
    /// The blob is not on the server (any more).
    #[error("the file is no longer available")]
    Gone,
    /// The blob is larger than the caller allows.
    #[error("the file is too large")]
    TooLarge,
    /// The server answered something else than the blob or descriptor that was asked for.
    #[error("the Blossom-Server answered with another file")]
    Mismatch,
    #[error("the Blossom-Server could not be reached")]
    Network,
    #[error("the Blossom-Server did not answer in time")]
    TimedOut,
}

impl BlossomError {
    /// The code the window shows a text for: `blossom:<status>` for a refusal.
    pub fn code(&self) -> String {
        match self {
            Self::NotAllowed => "blossom:notAllowed".into(),
            Self::Refused { status, .. } => format!("blossom:{status}"),
            Self::Gone => "blossom:gone".into(),
            Self::TooLarge => "blossom:tooLarge".into(),
            Self::Mismatch => "blossom:mismatch".into(),
            Self::Network => "blossom:network".into(),
            Self::TimedOut => "blossom:timedOut".into(),
        }
    }
}

type Result<T> = std::result::Result<T, BlossomError>;

/// A Blossom-Server the user chose, checked once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    base: Url,
    scope: EndpointScope,
}

/// What the server answers to an upload (BUD-02 blob descriptor).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Descriptor {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// `https`, or `http` to `127.0.0.1` only.
fn allowed_scheme(url: &Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => url.host_str() == Some("127.0.0.1"),
        _ => false,
    }
}

impl Server {
    /// Checks the address the user gave and where its host lies.
    pub async fn new(address: &str, resolver: &dyn Resolver) -> Result<Self> {
        let mut base = Url::parse(address.trim()).map_err(|_| BlossomError::NotAllowed)?;
        if !allowed_scheme(&base)
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err(BlossomError::NotAllowed);
        }
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        let scope = address::scope_of(&base, resolver)
            .await
            .map_err(|_| BlossomError::NotAllowed)?;
        Ok(Self { base, scope })
    }

    /// The address as checked, ending with `/`.
    pub fn address(&self) -> &str {
        self.base.as_str()
    }

    fn join(&self, path: &str) -> Result<Url> {
        self.base.join(path).map_err(|_| BlossomError::NotAllowed)
    }

    fn same_origin(&self, url: &Url) -> bool {
        url.origin() == self.base.origin()
    }

    /// Asks whether the server takes a blob of `size` bytes with hash `sha256` (BUD-06); a refusal
    /// carries the server's reason.
    pub async fn check_upload(
        &self,
        signer: &Keys,
        sha256: &str,
        size: u64,
        resolver: &dyn Resolver,
    ) -> Result<()> {
        check_upload_size(size)?;
        let url = self.join("upload")?;
        let auth = authorization(signer, "upload", sha256, "Upload encrypted chat file")?;
        let response = send(
            self.request(Method::HEAD, &url, resolver)
                .await?
                .header("Authorization", auth)
                .header("X-SHA-256", sha256)
                .header("X-Content-Length", size.to_string())
                .header("X-Content-Type", "application/octet-stream"),
            SHORT,
        )
        .await?;
        refused(&response).map_or(Ok(()), Err)
    }

    /// Uploads `blob` (`ponytail:` whole in memory, ≤ 100 MB per file, research R14) and checks the
    /// descriptor the server answers.
    pub async fn upload(
        &self,
        signer: &Keys,
        blob: Vec<u8>,
        resolver: &dyn Resolver,
    ) -> Result<Descriptor> {
        check_upload_size(blob.len() as u64)?;
        let sha256 = hex(&Sha256::digest(&blob));
        let size = blob.len() as u64;
        let url = self.join("upload")?;
        let auth = authorization(signer, "upload", &sha256, "Upload encrypted chat file")?;
        let response = send(
            self.request(Method::PUT, &url, resolver)
                .await?
                .header("Authorization", auth)
                .header("Content-Type", "application/octet-stream")
                .body(blob),
            TRANSFER,
        )
        .await?;
        if let Some(refusal) = refused(&response) {
            return Err(refusal);
        }
        let deadline = Instant::now() + SHORT;
        let body = read(response, 64 * 1024, deadline, &mut |_| {}).await?;
        let descriptor: Descriptor =
            serde_json::from_slice(&body).map_err(|_| BlossomError::Mismatch)?;
        let named = Url::parse(&descriptor.url).map_err(|_| BlossomError::Mismatch)?;
        if !descriptor.sha256.eq_ignore_ascii_case(&sha256)
            || descriptor.size != size
            || !names_blob(&named, &sha256)
            || !allowed_scheme(&named)
            || (!self.same_origin(&named)
                && !matches!(
                    tokio::time::timeout(SHORT, address::scope_of(&named, resolver)).await,
                    Ok(Ok(EndpointScope::Public))
                ))
        {
            return Err(BlossomError::Mismatch);
        }
        Ok(descriptor)
    }

    /// Deletes the blob `sha256`, signed by the key that uploaded it; one that is gone already is
    /// no failure.
    pub async fn delete(&self, signer: &Keys, sha256: &str, resolver: &dyn Resolver) -> Result<()> {
        if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(BlossomError::NotAllowed);
        }
        let url = self.join(sha256)?;
        let auth = authorization(signer, "delete", sha256, "Delete expired chat file")?;
        let response = send(
            self.request(Method::DELETE, &url, resolver)
                .await?
                .header("Authorization", auth),
            SHORT,
        )
        .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        refused(&response).map_or(Ok(()), Err)
    }

    async fn request(
        &self,
        method: Method,
        url: &Url,
        resolver: &dyn Resolver,
    ) -> Result<reqwest::RequestBuilder> {
        let client = checked_client(url, self.scope, resolver).await?;
        Ok(client.request(method, url.clone()))
    }
}

fn check_upload_size(size: u64) -> Result<()> {
    if size > ciphertext_len(MAX_PLAINTEXT) {
        return Err(BlossomError::TooLarge);
    }
    Ok(())
}

/// Downloads the blob at `url` (from a message) and checks it is the one with hash `sha256` and at
/// most `max` bytes. `own` is the user's own server: only there may a local address be reached.
/// `progress` hears the bytes received so far.
pub async fn download(
    url: &str,
    sha256: &str,
    max: u64,
    own: Option<&Server>,
    resolver: &dyn Resolver,
    progress: &mut (dyn FnMut(u64) + Send),
) -> Result<Vec<u8>> {
    let url = Url::parse(url).map_err(|_| BlossomError::NotAllowed)?;
    if !allowed_scheme(&url) || !names_blob(&url, sha256) {
        return Err(BlossomError::NotAllowed);
    }
    let scope = match own.filter(|server| server.same_origin(&url)) {
        Some(server) => server.scope,
        None => EndpointScope::Public,
    };
    let client = checked_client(&url, scope, resolver).await?;
    let deadline = Instant::now() + TRANSFER;
    let response = send(client.get(url), TRANSFER).await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err(BlossomError::Gone);
    }
    if let Some(refusal) = refused(&response) {
        return Err(refusal);
    }
    if response.content_length().is_some_and(|length| length > max) {
        return Err(BlossomError::TooLarge);
    }
    let max = usize::try_from(max).unwrap_or(usize::MAX);
    let blob = read(response, max, deadline, progress).await?;
    if !hex(&Sha256::digest(&blob)).eq_ignore_ascii_case(sha256) {
        return Err(BlossomError::Mismatch);
    }
    Ok(blob)
}

/// Resolves and pins an endpoint within the same 30-second connection allowance as the request.
async fn checked_client(
    url: &Url,
    scope: EndpointScope,
    resolver: &dyn Resolver,
) -> Result<reqwest::Client> {
    tokio::time::timeout(SHORT, pinned_client(url, scope, resolver))
        .await
        .map_err(|_| BlossomError::TimedOut)?
        .map_err(|_| BlossomError::NotAllowed)
}

/// The last path segment of a blob address is its hash, with or without an extension (BUD-01).
fn names_blob(url: &Url, sha256: &str) -> bool {
    url.path_segments()
        .and_then(|mut segments| segments.next_back())
        .map(|last| last.split('.').next().unwrap_or_default())
        .is_some_and(|name| name.eq_ignore_ascii_case(sha256))
}

/// `Nostr <base64url(event)>` for `action` on the blob `sha256` (contracts/blossom.md).
fn authorization(signer: &Keys, action: &str, sha256: &str, content: &str) -> Result<String> {
    let expiration = Timestamp::from_secs(Timestamp::now().as_secs() + AUTH_VALID.as_secs());
    let event = EventBuilder::new(Kind::Custom(AUTH_KIND), content)
        .tag(Tag::custom("t", [action]))
        .tag(Tag::custom("x", [sha256]))
        .tag(Tag::expiration(expiration))
        .finalize(signer)
        .map_err(|_| BlossomError::Network)?;
    let json = serde_json::to_vec(&event).map_err(|_| BlossomError::Network)?;
    Ok(format!("Nostr {}", URL_SAFE_NO_PAD.encode(json)))
}

async fn send(request: reqwest::RequestBuilder, limit: Duration) -> Result<reqwest::Response> {
    match tokio::time::timeout(limit, request.timeout(limit).send()).await {
        Err(_) => Err(BlossomError::TimedOut),
        Ok(Err(error)) if error.is_timeout() => Err(BlossomError::TimedOut),
        Ok(Err(_)) => Err(BlossomError::Network),
        Ok(Ok(response)) if response.status().is_redirection() => Err(BlossomError::Network),
        Ok(Ok(response)) => Ok(response),
    }
}

/// A `4xx`/`5xx` answer as a refusal with the server's `X-Reason`.
fn refused(response: &reqwest::Response) -> Option<BlossomError> {
    let status = response.status();
    if status.is_success() {
        return None;
    }
    let reason = response
        .headers()
        .get("X-Reason")
        .and_then(|value| value.to_str().ok())
        .map(|reason| reason.chars().take(200).collect());
    Some(BlossomError::Refused {
        status: status.as_u16(),
        reason,
    })
}

async fn read(
    mut response: reqwest::Response,
    max: usize,
    deadline: Instant,
    progress: &mut (dyn FnMut(u64) + Send),
) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(chunk) = tokio::time::timeout_at(deadline, response.chunk())
        .await
        .map_err(|_| BlossomError::TimedOut)?
        .map_err(|_| BlossomError::Network)?
    {
        if body.len() + chunk.len() > max {
            return Err(BlossomError::TooLarge);
        }
        body.extend_from_slice(&chunk);
        progress(body.len() as u64);
    }
    Ok(body)
}

#[cfg(test)]
#[path = "blossom_tests.rs"]
mod tests;
