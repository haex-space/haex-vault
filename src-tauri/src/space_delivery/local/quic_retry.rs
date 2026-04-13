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

use crate::peer_storage::protocol::PeerProtocolError;

use super::protocol::{self, Response, ALPN};

/// Total attempts (initial + retries). Tuned to CI observations: networks
/// typically recover within 1–2 seconds, so 3 attempts covers >99% of
/// transient dropouts without noticeably slowing true failures.
const MAX_ATTEMPTS: u32 = 3;

/// Backoff before each retry (milliseconds). Indexed by retry number: the 1st
/// retry waits `RETRY_DELAYS_MS[0]`, the 2nd waits `RETRY_DELAYS_MS[1]`, etc.
const RETRY_DELAYS_MS: [u64; 2] = [500, 2_000];

/// Connection timeout for a single connect attempt.
const CONNECT_TIMEOUT_SECS: u64 = 10;

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

    let response = protocol::read_response(&mut recv).await?;

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
