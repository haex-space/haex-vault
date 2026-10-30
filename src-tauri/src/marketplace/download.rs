//! The checked download of a bundle (spec 069 US2, FR-010 to FR-018, research R5, R6; contracts/
//! marketplace-api.md "Download").
//!
//! The host asks the marketplace for the address of one version, loads that address at once and
//! without the login, counts what arrives and refuses a body that is larger or smaller than the
//! marketplace said, or whose SHA-256 differs. Nothing is written to disk: the bytes go to the
//! pending slot of [`super::pending`] and from there into the install preview of spec 017, which
//! checks the signature on its own.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use futures::{Stream, StreamExt};
use semver::Version;
use sha2::{Digest, Sha256};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;

use super::catalog::{installability, valid_slug, Installability, Source};
use super::client::{policy, send_failure, status_failure, Http, SourceFailure};
use super::commands::{failure_error, refused};
use super::model::DownloadProgress;
use super::wire::{WireDetail, WireDownload};
use super::{app_version, kinds, CONNECT_TIMEOUT, DOWNLOAD_IDLE_TIMEOUT, PROGRESS_INTERVAL};
use crate::error::{HolziError, Result};
use crate::extensions::bundle::limits::ARCHIVE_BYTES;

/// The downloads that are running, each with the token that ends it.
#[derive(Default)]
pub struct Downloads {
    running: Mutex<HashMap<Uuid, CancellationToken>>,
}

/// Removes its download from [`Downloads`] when dropped, however the download ends.
pub struct DownloadGuard<'a> {
    downloads: &'a Downloads,
    id: Uuid,
}

impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        self.downloads.lock().remove(&self.id);
    }
}

impl Downloads {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, CancellationToken>> {
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Registers the download `id` and returns the token that ends it: it fires on
    /// [`Downloads::cancel`] and when `closing` (the vault) fires. An `id` that is already
    /// running is refused.
    pub fn begin(
        &self,
        id: Uuid,
        closing: &CancellationToken,
    ) -> Result<(CancellationToken, DownloadGuard<'_>)> {
        let mut running = self.lock();
        if running.contains_key(&id) {
            return Err(HolziError::InvalidInput {
                reason: "download id already in use".into(),
            });
        }
        let token = closing.child_token();
        running.insert(id, token.clone());
        Ok((
            token,
            DownloadGuard {
                downloads: self,
                id,
            },
        ))
    }

    /// Ends the running download `id`; one that is not running (or already ended) is ignored.
    pub fn cancel(&self, id: Uuid) {
        if let Some(token) = self.lock().get(&id) {
            token.cancel();
        }
    }
}

/// What the marketplace promised about the bytes behind a download address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub url: Url,
    pub size: u64,
    /// Lowercase hex, 64 characters.
    pub hash: String,
}

/// Whether `text` may be loaded as the download address of a marketplace at `base`: `https`, or
/// `http` only when the marketplace itself is `http` (research R6). Any other scheme is refused.
pub fn check_destination(base: &str, text: &str) -> Result<Url> {
    let refused_destination = || refused(kinds::DESTINATION_REFUSED);
    let url = Url::parse(text).map_err(|_| refused_destination())?;
    let allowed = match url.scheme() {
        "https" => true,
        "http" => Url::parse(base).is_ok_and(|base| base.scheme() == "http"),
        _ => false,
    };
    if !allowed || url.host_str().is_none() {
        return Err(refused_destination());
    }
    Ok(url)
}

fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// What the download call of the marketplace says, as far as it can be trusted: both checksum and
/// size must be there (FR-013), the size within the host's limit, the version the one asked for
/// and the address one the host may load.
pub fn target_of(base: &str, asked: &str, answer: WireDownload) -> Result<Target> {
    let (Some(size), Some(hash)) = (answer.bundle_size, answer.bundle_hash) else {
        return Err(refused(kinds::NO_HASH));
    };
    if !is_sha256_hex(hash.trim()) {
        return Err(refused(kinds::NO_HASH));
    }
    if answer.version != asked {
        return Err(refused(kinds::SOURCE_BAD_RESPONSE));
    }
    if size > ARCHIVE_BYTES {
        return Err(refused(kinds::TOO_LARGE));
    }
    Ok(Target {
        url: check_destination(base, &answer.download_url)?,
        size,
        hash: hash.trim().to_ascii_lowercase(),
    })
}

/// Asks the marketplace whether `version` of `slug` may be installed here (FR-018) and where its
/// bytes are. Two calls with the login of the source: the details for the bounds, the download
/// call for the address, size and checksum.
pub async fn resolve(
    http: Http,
    source: &Source,
    slug: &str,
    version: &str,
    app: &Version,
) -> Result<Target> {
    if !valid_slug(slug) {
        return Err(refused(kinds::NOT_FOUND));
    }
    if let Some(kind) = source.unavailable {
        return Err(failure_error(&SourceFailure { kind, detail: None }));
    }
    let auth = source.auth.as_ref();
    let detail: WireDetail = http
        .get_json(&source.base_url, &format!("/extensions/{slug}"), &[], auth)
        .await
        .map_err(|failure| failure_error(&failure))?;
    let listed = detail
        .item
        .versions
        .iter()
        .find(|listed| listed.version == version)
        .ok_or_else(|| refused(kinds::NOT_FOUND))?;
    match installability(listed, app) {
        Installability::Installable => {}
        Installability::NeedsNewer(_) | Installability::NeedsOlder(_) => {
            return Err(refused(kinds::INCOMPATIBLE_VERSION));
        }
        Installability::NoChecksum => return Err(refused(kinds::NO_HASH)),
    }
    let answer: WireDownload = http
        .get_json(
            &source.base_url,
            &format!("/extensions/{slug}/download"),
            &[("version", version.to_owned())],
            auth,
        )
        .await
        .map_err(|failure| failure_error(&failure))?;
    target_of(&source.base_url, version, answer)
}

/// The client of the byte download: no login, no marketplace headers, redirects as for any
/// request without a login. There is no total timeout (research R5).
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(format!("HaexVault/{}", app_version()))
            .redirect(policy(false))
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

/// Loads `target` and checks it. The signed address is loaded without any `Authorization`.
async fn fetch(
    target: &Target,
    cancel: &CancellationToken,
    progress: &mut impl FnMut(DownloadProgress),
) -> Result<Vec<u8>> {
    let sent = tokio::time::timeout(
        DOWNLOAD_IDLE_TIMEOUT,
        client().get(target.url.clone()).send(),
    );
    let response = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err(refused(kinds::DOWNLOAD_CANCELLED)),
        sent = sent => match sent {
            Err(_) => return Err(refused(kinds::SOURCE_TIMEOUT)),
            Ok(Err(error)) => return Err(failure_error(&send_failure(&error))),
            Ok(Ok(response)) => response,
        },
    };
    if !response.status().is_success() {
        // The body is dropped unread, as everywhere else.
        return Err(failure_error(&status_failure(response.status())));
    }
    collect(
        response.bytes_stream(),
        Limits::of(target),
        DOWNLOAD_IDLE_TIMEOUT,
        cancel,
        progress,
    )
    .await
}

/// What the bytes of a download must match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// The most bytes the host takes at all.
    pub max: u64,
    /// Exactly this many bytes must arrive.
    pub size: u64,
    /// Lowercase hex SHA-256.
    pub hash: String,
}

impl Limits {
    fn of(target: &Target) -> Self {
        Self {
            max: ARCHIVE_BYTES,
            size: target.size,
            hash: target.hash.clone(),
        }
    }
}

/// Reads `stream` to its end and returns the bytes if they are exactly `limits.size` many and
/// have the checksum. Stops at the first byte beyond `limits.max` or `limits.size`, without
/// reading the rest. `idle` is the longest silence between two chunks.
pub async fn collect<B, E>(
    stream: impl Stream<Item = std::result::Result<B, E>>,
    limits: Limits,
    idle: Duration,
    cancel: &CancellationToken,
    progress: &mut impl FnMut(DownloadProgress),
) -> Result<Vec<u8>>
where
    B: AsRef<[u8]>,
{
    futures::pin_mut!(stream);
    let mut bytes = Vec::with_capacity(usize::try_from(limits.size.min(1 << 20)).unwrap_or(0));
    let mut hasher = Sha256::new();
    let mut reported: Option<Instant> = None;
    loop {
        let next = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(refused(kinds::DOWNLOAD_CANCELLED)),
            next = tokio::time::timeout(idle, stream.next()) => next,
        };
        let chunk = match next {
            Err(_) => return Err(refused(kinds::SOURCE_TIMEOUT)),
            Ok(None) => break,
            Ok(Some(Err(_))) => return Err(refused(kinds::SOURCE_UNREACHABLE)),
            Ok(Some(Ok(chunk))) => chunk,
        };
        let chunk = chunk.as_ref();
        let loaded = bytes.len() as u64 + chunk.len() as u64;
        if loaded > limits.max {
            return Err(refused(kinds::TOO_LARGE));
        }
        if loaded > limits.size {
            return Err(refused(kinds::SIZE_MISMATCH));
        }
        hasher.update(chunk);
        bytes.extend_from_slice(chunk);
        if reported.is_none_or(|at| at.elapsed() >= PROGRESS_INTERVAL) {
            reported = Some(Instant::now());
            progress(DownloadProgress {
                loaded,
                total: limits.size,
            });
        }
    }
    if bytes.len() as u64 != limits.size {
        return Err(refused(kinds::SIZE_MISMATCH));
    }
    if hex(&hasher.finalize()) != limits.hash {
        return Err(refused(kinds::HASH_MISMATCH));
    }
    progress(DownloadProgress {
        loaded: limits.size,
        total: limits.size,
    });
    Ok(bytes)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(64), |mut text, b| {
        let _ = write!(text, "{b:02x}");
        text
    })
}

/// Downloads `version` of `slug` from `source` and returns the checked bytes (FR-010 to FR-013,
/// FR-018). `progress` hears of the advance at most every [`PROGRESS_INTERVAL`]; `cancel` ends it
/// with `marketplace_download_cancelled`.
pub async fn download_bundle(
    http: Http,
    source: &Source,
    slug: &str,
    version: &str,
    app: &Version,
    cancel: &CancellationToken,
    progress: &mut impl FnMut(DownloadProgress),
) -> Result<Vec<u8>> {
    let target = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err(refused(kinds::DOWNLOAD_CANCELLED)),
        target = resolve(http, source, slug, version, app) => target?,
    };
    fetch(&target, cancel, progress).await
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
