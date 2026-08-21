//! Peer-side logic: connecting to leader, sending/receiving sync data.

use std::time::Duration;

use crate::database::DbConnection;

use super::error::DeliveryError;
use super::protocol::{self, Request, Response};
use super::quic_retry::READ_TIMEOUT_SECS;

/// How long to wait before re-sending a request the leader rate-limited.
///
/// Slightly longer than the L5 accounting window (1 s, set where the
/// Leader's `RejectRateTracker` is constructed in
/// `space_delivery::local::commands::lifecycle`) so the peer's bucket has
/// certainly slid past the rejected burst rather than landing on the edge.
pub(super) const RATE_LIMIT_RETRY_DELAY: Duration = Duration::from_millis(1100);

/// Attempts spent waiting out a rate limit before the error is propagated.
/// Bounds the added latency at ~3.3 s, which is still well under the
/// reconnect backoff a propagated error would trigger.
pub(super) const RATE_LIMIT_MAX_RETRIES: u32 = 3;

/// Whether a leader response is the L5 rate-limit marker rather than a real
/// failure. Split out from `PeerSession::request` so the wire contract with
/// `leader::dispatch` can be unit-tested without a live QUIC pair — the two
/// sides agree only through [`protocol::RATE_LIMITED_PREFIX`], and a silent
/// drift there turns a retryable pause back into a session teardown.
pub(super) fn is_rate_limited(resp: &Response) -> bool {
    matches!(
        resp,
        Response::Error { message } if message.starts_with(protocol::RATE_LIMITED_PREFIX)
    )
}
use super::ucan::load_active_ucan_for_audience;
use crate::ucan::Cap;

/// A connected peer session with the leader.
///
/// The UCAN token resolved at `connect` is only authoritative on the
/// receiver for the initial `Announce` request — that bootstrap populates
/// the leader's `ConnectedPeer::validated_ucan` cache which the unified
/// AuthGate consumes for every subsequent request. The same token is still
/// attached to `SyncPush` / `SyncPull` / `RequestRejoin` /
/// `SubmitExternalCommit` purely for forward-compat with pre-AuthGate
/// receivers; new receivers ignore the wire field and use the cached
/// `ValidatedUcan`. See `space_delivery/local/auth_gate.rs` for the pipeline
/// and `protocol.rs::Request::*::ucan_token` for the 3-step removal plan.
pub struct PeerSession {
    conn: iroh::endpoint::Connection,
    /// UCAN attached to the deprecated `ucan_token` wire field on
    /// `SyncPush` / `SyncPull` / `RequestRejoin` / `SubmitExternalCommit`.
    ///
    /// `Some(token)` for a regular space session (resolved at `connect`).
    /// `None` for an owner-mesh session (`connect_owner`): owner devices have
    /// no UCAN for themselves, so every request emits `ucan_token: None`.
    ucan_token: Option<String>,
}

impl PeerSession {
    /// Connect to a leader and announce our identity.
    ///
    /// The UCAN token is resolved from `db` at the moment of connect. This
    /// means a reconnect after UCAN expiry picks up the freshly delegated
    /// token automatically, without any process restart or cache warming.
    pub async fn connect(
        iroh_endpoint: &iroh::Endpoint,
        leader_endpoint_id: &str,
        leader_relay_url: Option<&str>,
        space_id: &str,
        our_did: &str,
        our_signing_key: &ed25519_dalek::SigningKey,
        our_endpoint_id: &str,
        label: Option<&str>,
        db: &DbConnection,
    ) -> Result<Self, DeliveryError> {
        let ucan_token = load_active_ucan_for_audience(db, space_id, our_did, &[Cap::Read])?
            .ok_or_else(|| DeliveryError::AccessDenied {
                reason: format!(
                    "No active UCAN token for space {} audience {} — cannot connect",
                    space_id, our_did
                ),
            })?;

        // Relay-URL fallback shared with the other QUIC entry points
        // (claim-invite, push-invite). Without the live-relay fallback
        // sync-loop connects fail in docker-split-network setups —
        // see `project_share_visibility_after_accept`.
        // PeerSession has no `peer_storage` handle so it can't reach
        // `configured_relay_url`; the live one from `endpoint.addr()`
        // is enough in practice (peer_storage must be running for the
        // sync loop to even reach this code).
        let addr = super::quic_retry::build_endpoint_addr(
            iroh_endpoint,
            leader_endpoint_id,
            leader_relay_url,
            None,
        )
        .map_err(|reason| DeliveryError::ConnectionFailed { reason })?;

        let conn = iroh_endpoint
            .connect(addr, protocol::ALPN)
            .await
            .map_err(|e| DeliveryError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        // Server-initiated quic_did_auth handshake. The leader opens the first
        // bidirectional stream right after `accept` and writes a Challenge;
        // we accept that stream, sign the canonical payload with our identity,
        // and only then send the Announce on a fresh bi-stream. Without this
        // every announce after the C3 wire change would deadlock the leader
        // waiting for a Response that never arrives.
        super::quic_retry::complete_client_did_auth(
            &conn,
            our_did,
            our_signing_key,
            our_endpoint_id,
        )
        .await
        .map_err(|e| DeliveryError::ConnectionFailed {
            reason: format!("DID-auth: {e}"),
        })?;

        let session = Self {
            conn,
            ucan_token: Some(ucan_token),
        };

        // Send Announce request. The DID is no longer carried on the wire —
        // the leader reads it from the quic_did_auth handshake state for this
        // connection.
        let req = Request::Announce {
            endpoint_id: our_endpoint_id.to_string(),
            space_id: space_id.to_string(),
            label: label.map(|s| s.to_string()),
            claims: None,
            ucan_token: session.ucan_token.clone(),
        };

        let resp = session.request(req).await?;
        match resp {
            Response::Ok => Ok(session),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to Announce".to_string(),
            }),
        }
    }

    /// Connect to an owner-mesh leader (the owner's OWN device) without UCAN.
    ///
    /// The owner mesh syncs the owner's own vault across the owner's own
    /// devices. There is no delegation involved — the security gate is the
    /// same-owner DID-auth, so this mirrors [`connect`] EXCEPT:
    ///
    /// - DID-auth still runs (it is the whole security model — kept).
    /// - No UCAN is loaded (`load_active_ucan_for_audience` is skipped):
    ///   owner devices have no UCAN for themselves.
    /// - No `Announce` round-trip: the owner mesh skips it entirely.
    ///
    /// The resulting session holds `ucan_token: None`, so every request it
    /// builds emits `ucan_token: None` on the wire.
    pub async fn connect_owner(
        iroh_endpoint: &iroh::Endpoint,
        leader_endpoint_id: &str,
        leader_relay_url: Option<&str>,
        our_did: &str,
        our_signing_key: &ed25519_dalek::SigningKey,
        our_endpoint_id: &str,
    ) -> Result<Self, DeliveryError> {
        let addr = super::quic_retry::build_endpoint_addr(
            iroh_endpoint,
            leader_endpoint_id,
            leader_relay_url,
            None,
        )
        .map_err(|reason| DeliveryError::ConnectionFailed { reason })?;

        let conn = iroh_endpoint
            .connect(addr, protocol::ALPN)
            .await
            .map_err(|e| DeliveryError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        // Server-initiated quic_did_auth handshake — identical to `connect`.
        // This is the owner mesh's only security gate: the leader verifies the
        // signed challenge proves the same-owner DID before accepting any sync.
        super::quic_retry::complete_client_did_auth(
            &conn,
            our_did,
            our_signing_key,
            our_endpoint_id,
        )
        .await
        .map_err(|e| DeliveryError::ConnectionFailed {
            reason: format!("DID-auth: {e}"),
        })?;

        // No Announce: the owner mesh skips it. `haex_space_devices` bootstrap
        // and the AuthGate's cached `ValidatedUcan` are space-delegation
        // concerns that do not apply to the owner's own devices.
        Ok(Self {
            conn,
            ucan_token: None,
        })
    }

    /// Send a request and read the response.
    /// Send one request and await its response, retrying while the leader
    /// answers with the L5 rate-limit marker.
    ///
    /// The sync loops issue their requests back-to-back with no pacing:
    /// `run_pull_phase` pages until `has_more` is false and `run_push_phase`
    /// pushes every HLC chunk in a row, both propagating errors with `?`. A
    /// rate-limit reply surfacing as `DeliveryError::ProtocolError` therefore
    /// aborts the whole cycle and drops the session into the driver's
    /// reconnect backoff (5-60 s) — so a first join large enough to need more
    /// than `l5_handler_limits.sync_pull` pages would progress only a handful
    /// of pages per reconnect. Pausing for one window and re-sending is what
    /// keeps the gate a *rate* limit for honest clients while still costing an
    /// attacker its whole budget.
    ///
    /// Re-sending is safe because the gate rejects *before* the handler runs
    /// (see `leader::dispatch::handle_delivery_request`), so the retried
    /// request was never executed. Rejection also records nothing, so once the
    /// window slides the peer's bucket is empty and the retry passes.
    ///
    /// Bounded on purpose: after [`RATE_LIMIT_MAX_RETRIES`] the error is
    /// propagated as before, so a leader that answers `rate_limited:` forever
    /// cannot stall a peer indefinitely.
    async fn request(&self, req: Request) -> Result<Response, DeliveryError> {
        for _ in 0..RATE_LIMIT_MAX_RETRIES {
            let resp = self.request_once(&req).await?;
            if !is_rate_limited(&resp) {
                return Ok(resp);
            }
            eprintln!(
                "[SpaceDelivery] {} rate-limited by leader, retrying in {}ms",
                req.op_name(),
                RATE_LIMIT_RETRY_DELAY.as_millis(),
            );
            tokio::time::sleep(RATE_LIMIT_RETRY_DELAY).await;
        }
        self.request_once(&req).await
    }

    async fn request_once(&self, req: &Request) -> Result<Response, DeliveryError> {
        let (mut send, mut recv) =
            self.conn
                .open_bi()
                .await
                .map_err(|e| DeliveryError::ConnectionFailed {
                    reason: e.to_string(),
                })?;

        let bytes = protocol::encode(req).map_err(|e| DeliveryError::ProtocolError {
            reason: e.to_string(),
        })?;

        send.write_all(&bytes)
            .await
            .map_err(|e| DeliveryError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        send.finish().map_err(|e| DeliveryError::ConnectionFailed {
            reason: e.to_string(),
        })?;

        // Bound the response wait. A QUIC connection whose path silently
        // degrades after the handshake (e.g. relay-only after a direct-path
        // failure) leaves read_response hanging until the QUIC idle timer
        // fires (~150 s), wedging the sync loop. Mirrors the bound used by
        // quic_retry for the invite flows.
        match tokio::time::timeout(
            Duration::from_secs(READ_TIMEOUT_SECS),
            protocol::read_response(&mut recv),
        )
        .await
        {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(e)) => Err(DeliveryError::ProtocolError {
                reason: e.to_string(),
            }),
            Err(_) => Err(DeliveryError::ConnectionFailed {
                reason: format!("read timeout after {READ_TIMEOUT_SECS}s"),
            }),
        }
    }

    /// Build the `SyncPush` request, attaching `ucan_token` (`None` for an
    /// owner-mesh session). Extracted as an associated fn — independent of the
    /// live `conn` — so the owner-mode wiring is unit-testable without a QUIC
    /// endpoint.
    fn sync_push_request(
        ucan_token: &Option<String>,
        space_id: &str,
        changes: serde_json::Value,
    ) -> Request {
        Request::SyncPush {
            space_id: space_id.to_string(),
            changes,
            ucan_token: ucan_token.clone(),
        }
    }

    /// Build the `SyncPull` request, attaching `ucan_token` (`None` for an
    /// owner-mesh session). Extracted as an associated fn — independent of the
    /// live `conn` — so the owner-mode wiring is unit-testable without a QUIC
    /// endpoint.
    fn sync_pull_request(
        ucan_token: &Option<String>,
        space_id: &str,
        after_timestamp: Option<&str>,
    ) -> Request {
        Request::SyncPull {
            space_id: space_id.to_string(),
            after_timestamp: after_timestamp.map(|s| s.to_string()),
            ucan_token: ucan_token.clone(),
        }
    }

    /// Push local CRDT changes to the leader.
    pub async fn push_changes(
        &self,
        space_id: &str,
        changes: serde_json::Value,
    ) -> Result<(), DeliveryError> {
        let req = Self::sync_push_request(&self.ucan_token, space_id, changes);
        match self.request(req).await? {
            Response::Ok => Ok(()),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to SyncPush".to_string(),
            }),
        }
    }

    /// Pull one page of CRDT changes from the leader.
    ///
    /// Returns `(changes, has_more)`: `has_more` is `true` when the serve side
    /// paginated and more pages remain. The caller resumes the next page with
    /// `after_timestamp` = the MAX HLC of the returned changes.
    pub async fn pull_changes(
        &self,
        space_id: &str,
        after_timestamp: Option<&str>,
    ) -> Result<(serde_json::Value, bool), DeliveryError> {
        let req = Self::sync_pull_request(&self.ucan_token, space_id, after_timestamp);
        match self.request(req).await? {
            Response::SyncChanges { changes, has_more } => Ok((changes, has_more)),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to SyncPull".to_string(),
            }),
        }
    }

    /// Build the `SyncPullColumns` request, attaching `ucan_token` (`None` for an
    /// owner-mesh session). Extracted as an associated fn — independent of the
    /// live `conn` — so the owner-mode wiring is unit-testable without a QUIC
    /// endpoint.
    fn pull_columns_request(
        ucan_token: &Option<String>,
        space_id: &str,
        columns: &[(String, String)],
        after_row_pks: Option<&str>,
    ) -> Request {
        Request::SyncPullColumns {
            space_id: space_id.to_string(),
            columns: columns.to_vec(),
            after_row_pks: after_row_pks.map(|s| s.to_string()),
            ucan_token: ucan_token.clone(),
        }
    }

    /// Owner-mesh: recover values for columns this device skipped during a
    /// schema-skew apply. Pagination is not used yet (`after_row_pks: None`);
    /// the serving side returns all rows for the requested columns in one shot.
    pub async fn pull_columns(
        &self,
        space_id: &str,
        columns: &[(String, String)],
    ) -> Result<serde_json::Value, DeliveryError> {
        let req = Self::pull_columns_request(&self.ucan_token, space_id, columns, None);
        match self.request(req).await? {
            // The column-recovery dump is single-shot, so `has_more` is always
            // false here and is intentionally ignored.
            Response::SyncChanges { changes, .. } => Ok(changes),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to SyncPullColumns".to_string(),
            }),
        }
    }

    /// Fetch MLS messages from the leader after a given ID.
    pub async fn fetch_mls_messages(
        &self,
        space_id: &str,
        after_id: Option<i64>,
    ) -> Result<Vec<super::protocol::MlsMessageEntry>, DeliveryError> {
        let req = Request::MlsFetchMessages {
            space_id: space_id.to_string(),
            after_id,
        };
        match self.request(req).await? {
            Response::Messages { messages } => Ok(messages),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to MlsFetchMessages".to_string(),
            }),
        }
    }

    /// Acknowledge successfully processed MLS commits.
    pub async fn ack_commits(
        &self,
        space_id: &str,
        message_ids: Vec<i64>,
    ) -> Result<(), DeliveryError> {
        if message_ids.is_empty() {
            return Ok(());
        }
        let req = Request::MlsAckCommit {
            space_id: space_id.to_string(),
            message_ids,
        };
        match self.request(req).await? {
            Response::Ok => Ok(()),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to MlsAckCommit".to_string(),
            }),
        }
    }

    /// Upload key packages to the leader for this peer's DID.
    pub async fn upload_key_packages(
        &self,
        space_id: &str,
        packages: Vec<String>,
        pops: Vec<String>,
    ) -> Result<(), DeliveryError> {
        let req = Request::MlsUploadKeyPackages {
            space_id: space_id.to_string(),
            packages,
            pops,
        };
        match self.request(req).await? {
            Response::Ok => Ok(()),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to MlsUploadKeyPackages".to_string(),
            }),
        }
    }

    /// Query key package status: how many the leader has and how many more it needs.
    /// Returns (available, needed).
    pub async fn query_key_package_status(
        &self,
        space_id: &str,
    ) -> Result<(u32, u32), DeliveryError> {
        let req = Request::MlsKeyPackageCount {
            space_id: space_id.to_string(),
        };
        match self.request(req).await? {
            Response::KeyPackageCount { available, needed } => Ok((available, needed)),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to MlsKeyPackageCount".to_string(),
            }),
        }
    }

    /// Fetch unconsumed welcome messages from the leader.
    pub async fn fetch_welcomes(&self, space_id: &str) -> Result<Vec<String>, DeliveryError> {
        let req = Request::MlsFetchWelcomes {
            space_id: space_id.to_string(),
        };
        match self.request(req).await? {
            Response::Welcomes { welcomes } => Ok(welcomes),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to MlsFetchWelcomes".to_string(),
            }),
        }
    }

    /// Request rejoin via External Commit. Returns base64-encoded GroupInfo.
    pub async fn request_rejoin(&self, space_id: &str) -> Result<String, DeliveryError> {
        let req = Request::RequestRejoin {
            space_id: space_id.to_string(),
            ucan_token: self.ucan_token.clone(),
        };
        match self.request(req).await? {
            Response::GroupInfo { group_info } => Ok(group_info),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to RequestRejoin".to_string(),
            }),
        }
    }

    /// Submit an External Commit to rejoin a group.
    /// Returns the message ID assigned by the leader so the caller can advance
    /// its MLS cursor past the External Commit itself.
    pub async fn submit_external_commit(
        &self,
        space_id: &str,
        commit_b64: &str,
    ) -> Result<i64, DeliveryError> {
        let req = Request::SubmitExternalCommit {
            space_id: space_id.to_string(),
            commit: commit_b64.to_string(),
            ucan_token: self.ucan_token.clone(),
        };
        match self.request(req).await? {
            Response::MessageStored { message_id } => Ok(message_id),
            // Tolerate older leaders that still respond with Ok (no msg_id).
            Response::Ok => Ok(0),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to SubmitExternalCommit".to_string(),
            }),
        }
    }

    /// Close the connection gracefully.
    pub fn close(&self) {
        self.conn.close(0u32.into(), b"done");
    }
}

#[cfg(test)]
#[path = "peer_tests.rs"]
mod tests;
