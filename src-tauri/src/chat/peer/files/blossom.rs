//! The Blossom client of the chat (spec 072 research R12, contracts/blossom.md; BUD-01, BUD-02,
//! BUD-06): ask before uploading (`HEAD /upload`), upload, download and delete blobs. Every call that
//! needs a key carries a kind 24242 event signed by the key of that blob.
//!
//! Requests go through the storages' pinned client: the host is reached only at the addresses that
//! were checked, without redirects or proxies. A server address must be `https`, or `http` to
//! `127.0.0.1` (a server on this machine, the tests). A download address comes from another vault's
//! message, so it must lie on a public host unless it is on the user's own server; a message cannot
//! make this device reach into its local network.

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use futures::stream::{Stream, StreamExt};
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
/// An upload or download that moves no bytes for this long fails.
const STALL: Duration = Duration::from_secs(30);
/// After the last byte of an upload the server may take this long to answer.
const ANSWER: Duration = Duration::from_secs(60);
/// What a transfer may take whatever its size; [`total_limit`] adds the time its size needs.
const TRANSFER: Duration = Duration::from_secs(5 * 60);
/// The slowest average, in bytes per second, a transfer is given time for.
const MIN_RATE: u64 = 64 * 1024;

/// How long a transfer may stand still (the tests shorten it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Patience {
    /// With no byte moved while sending or receiving.
    pub stall: Duration,
    /// For the response headers of a download. An upload stands still at most the shorter of
    /// `stall` and `answer`, whether it waits for its next byte or for the server's answer.
    pub answer: Duration,
}

impl Default for Patience {
    fn default() -> Self {
        Self {
            stall: STALL,
            answer: ANSWER,
        }
    }
}

/// The most a transfer of `size` bytes may take in all: a stalled one fails sooner, this stops
/// one that crawls (a server that sends a byte every few seconds) from holding on for ever.
fn total_limit(size: u64) -> Duration {
    TRANSFER + Duration::from_secs(size / MIN_RATE)
}

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
    patience: Patience,
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

/// The base URL of the address the user gave, ending with `/`; no host lookup yet.
fn parse_address(address: &str) -> Result<Url> {
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
    Ok(base)
}

/// Whether `address` has the shape of a Blossom-Server address: `https` (or `http` to `127.0.0.1`)
/// and no credentials, query or fragment. Where its host lies is checked when it is used.
pub fn is_valid_address(address: &str) -> bool {
    parse_address(address).is_ok()
}

impl Server {
    /// Checks the address the user gave and where its host lies.
    pub async fn new(address: &str, resolver: &dyn Resolver) -> Result<Self> {
        let base = parse_address(address)?;
        let scope = address::scope_of(&base, resolver)
            .await
            .map_err(|_| BlossomError::NotAllowed)?;
        Ok(Self {
            base,
            scope,
            patience: Patience::default(),
        })
    }

    /// The same server with another patience, for the tests.
    #[cfg(test)]
    pub fn with_patience(mut self, patience: Patience) -> Self {
        self.patience = patience;
        self
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

    /// Uploads the blob as it is produced and checks the descriptor the server answers. `progress`
    /// hears the bytes sent so far. The upload fails when no byte moves for [`Patience::stall`].
    pub async fn upload(
        &self,
        signer: &Keys,
        body: UploadBody,
        resolver: &dyn Resolver,
        progress: &mut (dyn FnMut(u64) + Send),
    ) -> Result<Descriptor> {
        check_upload_size(body.size)?;
        let (sha256, size) = (body.sha256, body.size);
        let url = self.join("upload")?;
        let auth = authorization(signer, "upload", &sha256, "Upload encrypted chat file")?;
        let moved = Arc::new(AtomicU64::new(0));
        let counted = {
            let moved = Arc::clone(&moved);
            body.chunks.inspect(move |chunk| {
                if let Ok(chunk) = chunk {
                    moved.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                }
            })
        };
        let request = self
            .request(Method::PUT, &url, resolver)
            .await?
            .header("Authorization", auth)
            .header("Content-Type", "application/octet-stream")
            .header(reqwest::header::CONTENT_LENGTH, size)
            .body(reqwest::Body::wrap_stream(counted));
        let (response, deadline) =
            standing_still_fails(request.send(), &moved, size, self.patience, progress).await?;
        let response = answer(response)?;
        if let Some(refusal) = refused(&response) {
            return Err(refusal);
        }
        let deadline = deadline.min(Instant::now() + SHORT);
        if Instant::now() >= deadline {
            return Err(BlossomError::TimedOut);
        }
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

/// A blob on its way down, checked as it comes: [`Self::chunk`] answers its bytes one piece at a
/// time and, once the last one is in, whether they hashed to the name the message gave. The caller
/// must treat what it wrote as unconfirmed until `chunk` has answered `None`.
pub struct Download {
    response: reqwest::Response,
    hasher: Sha256,
    expected: String,
    max: u64,
    received: u64,
    patience: Patience,
    until: Instant,
}

impl Download {
    /// The bytes received so far.
    pub fn received(&self) -> u64 {
        self.received
    }

    /// The same download with another patience, for the tests.
    #[cfg(test)]
    pub fn with_patience(mut self, patience: Patience) -> Self {
        self.patience = patience;
        self
    }

    /// The next piece, or `None` when the blob is complete and is the one with the hash that was
    /// named. Fails after [`Patience::stall`] without a byte, past the total allowance, above the
    /// size the caller allows and when the hash is another.
    pub async fn chunk(&mut self) -> Result<Option<Vec<u8>>> {
        if Instant::now() >= self.until {
            return Err(BlossomError::TimedOut);
        }
        let limit = (Instant::now() + self.patience.stall).min(self.until);
        let piece = tokio::time::timeout_at(limit, self.response.chunk())
            .await
            .map_err(|_| BlossomError::TimedOut)?
            .map_err(|_| BlossomError::Network)?;
        let Some(piece) = piece else {
            return if hex(&self.hasher.clone().finalize()).eq_ignore_ascii_case(&self.expected) {
                Ok(None)
            } else {
                Err(BlossomError::Mismatch)
            };
        };
        self.received += piece.len() as u64;
        if self.received > self.max {
            return Err(BlossomError::TooLarge);
        }
        self.hasher.update(&piece);
        Ok(Some(piece.to_vec()))
    }
}

/// Starts the download of the blob at `url` (from a message) with hash `sha256`, at most `max`
/// bytes. `own` is the user's own server: only there may a local address be reached.
pub async fn download(
    url: &str,
    sha256: &str,
    max: u64,
    own: Option<&Server>,
    resolver: &dyn Resolver,
) -> Result<Download> {
    let url = Url::parse(url).map_err(|_| BlossomError::NotAllowed)?;
    if !allowed_scheme(&url) || !names_blob(&url, sha256) {
        return Err(BlossomError::NotAllowed);
    }
    let scope = match own.filter(|server| server.same_origin(&url)) {
        Some(server) => server.scope,
        None => EndpointScope::Public,
    };
    let patience = own.map_or_else(Patience::default, |server| server.patience);
    let client = checked_client(&url, scope, resolver).await?;
    // `send()` also configures reqwest's whole-request timeout, which would cap a progressing
    // response body. Here only waiting for the headers gets the answer deadline; `Download::chunk`
    // applies the stall and size-based deadlines to the body.
    let response = match tokio::time::timeout(patience.answer, client.get(url).send()).await {
        Err(_) => return Err(BlossomError::TimedOut),
        Ok(result) => answer(result)?,
    };
    if response.status() == StatusCode::NOT_FOUND {
        return Err(BlossomError::Gone);
    }
    if let Some(refusal) = refused(&response) {
        return Err(refusal);
    }
    if response.content_length().is_some_and(|length| length > max) {
        return Err(BlossomError::TooLarge);
    }
    Ok(Download {
        response,
        hasher: Sha256::new(),
        expected: sha256.to_owned(),
        max,
        received: 0,
        patience,
        until: Instant::now() + total_limit(max),
    })
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
        Ok(result) => answer(result),
    }
}

/// A transport answer as this client's result: a timeout, a failure, a redirect (never followed).
fn answer(
    result: std::result::Result<reqwest::Response, reqwest::Error>,
) -> Result<reqwest::Response> {
    match result {
        Err(error) if error.is_timeout() => Err(BlossomError::TimedOut),
        Err(_) => Err(BlossomError::Network),
        Ok(response) if response.status().is_redirection() => Err(BlossomError::Network),
        Ok(response) => Ok(response),
    }
}

/// Runs `operation` (an upload whose body is read by the connection, counted in `moved`) and gives
/// up when `moved` stands still for the stall time, when the answer to a finished body takes
/// longer than the answer time, or after the total allowance of `size` bytes. `progress` hears
/// `moved` as it grows.
async fn standing_still_fails<T>(
    operation: impl std::future::Future<Output = T>,
    moved: &AtomicU64,
    size: u64,
    patience: Patience,
    progress: &mut (dyn FnMut(u64) + Send),
) -> Result<(T, Instant)> {
    tokio::pin!(operation);
    let until = Instant::now() + total_limit(size);
    let (mut last, mut since) = (0, Instant::now());
    let mut tick = tokio::time::interval(
        (patience.stall / 4).clamp(Duration::from_millis(20), Duration::from_millis(500)),
    );
    loop {
        tokio::select! {
            output = &mut operation => return Ok((output, until)),
            _ = tick.tick() => {
                let now = moved.load(Ordering::Relaxed);
                if now != last {
                    (last, since) = (now, Instant::now());
                    progress(now);
                }
                let waiting_for = patience.stall.min(patience.answer);
                if since.elapsed() >= waiting_for || Instant::now() >= until {
                    return Err(BlossomError::TimedOut);
                }
            }
        }
    }
}

/// What an upload sends: the blob as a stream of pieces, with the hash and length of all of it
/// (the server is asked and the upload is signed before the first byte).
pub struct UploadBody {
    pub sha256: String,
    pub size: u64,
    pub chunks: Pin<Box<dyn Stream<Item = std::io::Result<Vec<u8>>> + Send + Sync>>,
}

impl UploadBody {
    /// A blob that is in memory.
    #[cfg(test)]
    pub fn whole(blob: Vec<u8>) -> Self {
        Self {
            sha256: hex(&Sha256::digest(&blob)),
            size: blob.len() as u64,
            chunks: Box::pin(futures::stream::once(async move { Ok(blob) })),
        }
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
