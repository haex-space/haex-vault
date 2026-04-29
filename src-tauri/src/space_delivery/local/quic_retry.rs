//! Retry helper for transient QUIC failures in space delivery.
//!
//! PushInvite and ClaimInvite occasionally fail with transient errors like
//! "connection lost" — especially in containerized environments where the
//! iroh relay can momentarily drop packets. This module wraps a QUIC
//! request/response cycle in a small retry loop that distinguishes between
//! transient and final errors **by matching on typed error enum variants**
//! (not by string inspection).
//!
//! - Transient (retried): timeouts, connection reset/closed, stream I/O drops.
//! - Final (not retried): protocol rejections, invalid JSON, encoding errors.
//!
//! The DRY win: both PushInvite and ClaimInvite share the exact same QUIC
//! dance (connect → open_bi → write → read → close), so it's extracted into
//! a single `send_request_once` helper, wrapped with retry.

use std::future::Future;
use std::time::Duration;

use iroh::endpoint::{ConnectError, ConnectionError, Endpoint, WriteError};
use iroh::EndpointAddr;
use iroh::RelayUrl;

use crate::peer_storage::protocol::PeerProtocolError;

use super::protocol::{self, Response, ALPN};

/// Build an `iroh::EndpointAddr` for `remote_endpoint_id` with a
/// best-effort relay URL.
///
/// Three call sites in `space_delivery/local` previously inlined this same
/// dance with subtly different fallback chains — `PeerSession::connect`
/// even shipped without the live-relay fallback for months and silently
/// failed in the docker-split-network rig (see
/// `project_share_visibility_after_accept`). Centralising avoids the next
/// drift.
///
/// Fallback order:
///   1. `explicit_relay` — explicit URL from the request payload.
///   2. `configured_relay` — `peer_storage`'s startup-time relay setting.
///   3. live relay from `endpoint.addr()` — what the local endpoint is
///      currently registered with.
///
/// All three sources may be missing. The resulting `EndpointAddr` is
/// still valid for direct/mDNS-only attempts, which the caller may treat
/// as a degraded-but-acceptable mode (see `local_delivery_push_invite`'s
/// "Connecting without relay" warning).
pub fn build_endpoint_addr(
    endpoint: &Endpoint,
    remote_endpoint_id: &str,
    explicit_relay: Option<&str>,
    configured_relay: Option<&RelayUrl>,
) -> Result<EndpointAddr, String> {
    let remote_id: iroh::EndpointId = remote_endpoint_id
        .parse()
        .map_err(|e| format!("invalid endpoint id: {e}"))?;

    let relay = parse_explicit_relay(explicit_relay)
        .or_else(|| configured_relay.cloned())
        .or_else(|| endpoint.addr().relay_urls().next().cloned());

    Ok(match relay {
        Some(url) => EndpointAddr::new(remote_id).with_relay_url(url),
        None => EndpointAddr::new(remote_id),
    })
}

/// Variant of [`build_endpoint_addr`] that also returns the resolved
/// relay URL (if any), so callers can log it without re-parsing.
pub fn build_endpoint_addr_with_relay(
    endpoint: &Endpoint,
    remote_endpoint_id: &str,
    explicit_relay: Option<&str>,
    configured_relay: Option<&RelayUrl>,
) -> Result<(EndpointAddr, Option<RelayUrl>), String> {
    let remote_id: iroh::EndpointId = remote_endpoint_id
        .parse()
        .map_err(|e| format!("invalid endpoint id: {e}"))?;

    let relay = parse_explicit_relay(explicit_relay)
        .or_else(|| configured_relay.cloned())
        .or_else(|| endpoint.addr().relay_urls().next().cloned());

    let addr = match relay.clone() {
        Some(url) => EndpointAddr::new(remote_id).with_relay_url(url),
        None => EndpointAddr::new(remote_id),
    };
    Ok((addr, relay))
}

/// Parse an explicit relay URL string, logging a warning on parse failure
/// instead of silently treating it the same as `None`. Malformed URLs from
/// peer protocol payloads (or stale config) should be visible in logs so
/// they can be diagnosed, even though we still fall back gracefully.
fn parse_explicit_relay(explicit_relay: Option<&str>) -> Option<RelayUrl> {
    let raw = explicit_relay?;
    match raw.parse::<RelayUrl>() {
        Ok(url) => Some(url),
        Err(e) => {
            eprintln!(
                "[quic_retry] explicit relay url '{raw}' failed to parse: {e} — falling back"
            );
            None
        }
    }
}

/// Total attempts (initial + retries). Tuned to CI observations: networks
/// typically recover within 1–2 seconds, so 3 attempts covers >99% of
/// transient dropouts without noticeably slowing true failures.
const MAX_ATTEMPTS: u32 = 3;

/// Backoff before each retry (milliseconds). Indexed by retry number: the 1st
/// retry waits `RETRY_DELAYS_MS[0]`, the 2nd waits `RETRY_DELAYS_MS[1]`, etc.
const RETRY_DELAYS_MS: [u64; 2] = [500, 2_000];

/// Connection timeout for a single connect attempt.
const CONNECT_TIMEOUT_SECS: u64 = 10;

/// Read timeout for the response after the request has been sent.
/// Without this, a connection where the QUIC path degrades after the
/// handshake (e.g. relay established but direct-path migration to IPv6 fails
/// leaving the connection with no usable path) blocks both sides indefinitely
/// until the QUIC idle timeout fires (~150s). With this timeout, the call
/// fails fast as a transient error and the retry loop re-establishes a clean
/// connection on the next attempt.
const READ_TIMEOUT_SECS: u64 = 30;

/// Errors from [`send_request_once`], preserving the original iroh/quinn error
/// types so the retry policy can match on variants instead of strings.
#[derive(Debug, thiserror::Error)]
pub enum QuicSendError {
    #[error("connect timeout after {0}s")]
    ConnectTimeout(u64),

    #[error("connect failed: {0}")]
    Connect(#[from] ConnectError),

    #[error("open bi-stream: {0}")]
    OpenStream(ConnectionError),

    #[error("write request: {0}")]
    Write(#[from] WriteError),

    #[error("finish send: {0}")]
    Finish(String),

    #[error("read response: {0}")]
    Read(#[from] PeerProtocolError),
}

impl QuicSendError {
    /// Whether this error is likely to succeed on retry.
    ///
    /// Classification is done by pattern-matching on the typed error enums
    /// from iroh/quinn — never on error message strings.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::ConnectTimeout(_) => true,
            Self::Connect(e) => is_connect_transient(e),
            Self::OpenStream(e) => is_connection_transient(e),
            // Write-side failures that wrap a connection loss are transient;
            // peer-initiated stream stops (Stopped) are protocol-level and final.
            Self::Write(WriteError::ConnectionLost(ce)) => is_connection_transient(ce),
            Self::Write(_) => false,
            // finish() only fails if the stream is already closed — retrying
            // would just reopen it with no useful state recovery. Treat as final.
            Self::Finish(_) => false,
            // PeerProtocolError::Read(_) comes from RecvStream I/O dropping.
            // InvalidJson / MessageTooLarge are protocol errors — final.
            Self::Read(PeerProtocolError::Read(_)) => true,
            Self::Read(_) => false,
        }
    }
}

/// True for `ConnectionError` variants that indicate a transient network
/// condition (as opposed to a peer-initiated close or protocol violation).
fn is_connection_transient(ce: &ConnectionError) -> bool {
    matches!(
        ce,
        ConnectionError::TimedOut
            | ConnectionError::Reset
            | ConnectionError::ConnectionClosed(_),
    )
}

/// True for `ConnectError` variants likely to recover on retry.
/// `ConnectError` is `#[non_exhaustive]`, so unknown future variants are
/// treated as non-transient (conservative default).
fn is_connect_transient(ce: &ConnectError) -> bool {
    match ce {
        // The connection was established but then lost/reset during setup.
        ConnectError::Connection { source, .. } => is_connection_transient(source),
        // Mid-handshake failure — most commonly a network blip.
        ConnectError::Connecting { .. } => true,
        // Address resolution / setup failures — won't resolve on retry.
        ConnectError::Connect { .. } => false,
        _ => false,
    }
}

/// Execute a single QUIC request/response cycle: connect → open_bi → write →
/// read → close. Returns the decoded [`Response`] on success.
///
/// Each call establishes a fresh connection. For a retry-capable version, use
/// [`send_request_with_retry`].
pub async fn send_request_once(
    endpoint: &Endpoint,
    addr: EndpointAddr,
    request_bytes: &[u8],
) -> Result<Response, QuicSendError> {
    let conn = tokio::time::timeout(
        Duration::from_secs(CONNECT_TIMEOUT_SECS),
        endpoint.connect(addr, ALPN),
    )
    .await
    .map_err(|_| QuicSendError::ConnectTimeout(CONNECT_TIMEOUT_SECS))??;

    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(QuicSendError::OpenStream)?;

    send.write_all(request_bytes).await?;
    send.finish()
        .map_err(|e| QuicSendError::Finish(e.to_string()))?;

    let response = tokio::time::timeout(
        Duration::from_secs(READ_TIMEOUT_SECS),
        protocol::read_response(&mut recv),
    )
    .await
    .map_err(|_| QuicSendError::Read(crate::peer_storage::protocol::PeerProtocolError::Read(
        format!("read timeout after {READ_TIMEOUT_SECS}s"),
    )))??;

    // Best-effort close; ignore errors since we already have the response.
    conn.close(0u32.into(), b"done");

    Ok(response)
}

/// Execute a QUIC request/response cycle with up to [`MAX_ATTEMPTS`] attempts,
/// retrying on transient failures (see [`QuicSendError::is_transient`]).
///
/// `operation` is used for diagnostic logging only.
pub async fn send_request_with_retry(
    operation: &str,
    endpoint: &Endpoint,
    addr: EndpointAddr,
    request_bytes: &[u8],
) -> Result<Response, QuicSendError> {
    let mut last_error: Option<QuicSendError> = None;
    for attempt in 1..=MAX_ATTEMPTS {
        match send_request_once(endpoint, addr.clone(), request_bytes).await {
            Ok(response) => {
                if attempt > 1 {
                    eprintln!(
                        "[{operation}] succeeded on attempt {attempt}/{MAX_ATTEMPTS}"
                    );
                }
                return Ok(response);
            }
            Err(e) if !e.is_transient() => return Err(e),
            Err(e) => {
                if attempt < MAX_ATTEMPTS {
                    let delay_ms = RETRY_DELAYS_MS[(attempt - 1) as usize];
                    eprintln!(
                        "[{operation}] transient error on attempt \
                         {attempt}/{MAX_ATTEMPTS}: {e}. \
                         Retrying in {delay_ms}ms…"
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                last_error = Some(e);
            }
        }
    }
    Err(last_error.expect("loop runs at least once"))
}

/// Generic retry wrapper for caller-supplied async operations that don't fit
/// the `send_request_with_retry` shape. Callers provide a closure returning
/// `Result<T, E>` and a classifier that says whether an error is transient.
///
/// Prefer [`send_request_with_retry`] whenever possible — it preserves typed
/// errors end-to-end. This helper exists for paths where the closure body
/// already reduces errors to a unified type.
pub async fn retry_transient<T, E, F, Fut, IsTransient>(
    operation: &str,
    mut op: F,
    is_transient: IsTransient,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    IsTransient: Fn(&E) -> bool,
    E: std::fmt::Display,
{
    let mut last_error: Option<E> = None;
    for attempt in 1..=MAX_ATTEMPTS {
        match op().await {
            Ok(result) => {
                if attempt > 1 {
                    eprintln!("[{operation}] succeeded on attempt {attempt}/{MAX_ATTEMPTS}");
                }
                return Ok(result);
            }
            Err(e) if !is_transient(&e) => return Err(e),
            Err(e) => {
                if attempt < MAX_ATTEMPTS {
                    let delay_ms = RETRY_DELAYS_MS[(attempt - 1) as usize];
                    eprintln!(
                        "[{operation}] transient error on attempt \
                         {attempt}/{MAX_ATTEMPTS}: {e}. \
                         Retrying in {delay_ms}ms…"
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                last_error = Some(e);
            }
        }
    }
    Err(last_error.expect("loop runs at least once"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn connection_transient_classification() {
        assert!(is_connection_transient(&ConnectionError::TimedOut));
        assert!(is_connection_transient(&ConnectionError::Reset));
        assert!(!is_connection_transient(&ConnectionError::LocallyClosed));
        assert!(!is_connection_transient(&ConnectionError::VersionMismatch));
    }

    #[test]
    fn protocol_errors_are_final() {
        let err = QuicSendError::Read(PeerProtocolError::InvalidJson("bad".into()));
        assert!(!err.is_transient());

        let err = QuicSendError::Read(PeerProtocolError::MessageTooLarge {
            size: 100,
            max: 50,
        });
        assert!(!err.is_transient());
    }

    #[test]
    fn stream_read_failure_is_transient() {
        let err = QuicSendError::Read(PeerProtocolError::Read("eof".into()));
        assert!(err.is_transient());
    }

    #[test]
    fn connect_timeout_is_transient() {
        assert!(QuicSendError::ConnectTimeout(10).is_transient());
    }

    #[tokio::test]
    async fn retry_succeeds_after_transient_failure() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_clone = calls.clone();
        let result = retry_transient(
            "test",
            move || {
                let c = calls_clone.clone();
                async move {
                    let n = c.fetch_add(1, Ordering::SeqCst) + 1;
                    if n < 2 {
                        Err("transient".to_string())
                    } else {
                        Ok(42)
                    }
                }
            },
            |_| true,
        )
        .await;
        assert_eq!(result, Ok(42));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retry_fails_fast_on_non_transient() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_clone = calls.clone();
        let result: Result<i32, String> = retry_transient(
            "test",
            move || {
                let c = calls_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err("final".to_string())
                }
            },
            |_| false,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
