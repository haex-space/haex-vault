//! Peer-side logic: connecting to leader, sending/receiving sync data.

use std::time::Duration;

use crate::database::DbConnection;

use super::error::DeliveryError;
use super::protocol::{self, Request, Response};
use super::quic_retry::READ_TIMEOUT_SECS;
use super::ucan::load_active_ucan_for_audience;

/// A connected peer session with the leader.
///
/// The UCAN token passed on `connect` is stored and attached to every
/// space-scoped request (Announce, SyncPush, SyncPull). The leader verifies
/// the token against the target space on each call — so even a hijacked
/// connection cannot pull or push data without a valid delegation.
pub struct PeerSession {
    conn: iroh::endpoint::Connection,
    ucan_token: String,
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
        let ucan_token = load_active_ucan_for_audience(db, space_id, our_did)?
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

        let session = Self { conn, ucan_token };

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

    /// Send a request and read the response.
    async fn request(&self, req: Request) -> Result<Response, DeliveryError> {
        let (mut send, mut recv) = self
            .conn
            .open_bi()
            .await
            .map_err(|e| DeliveryError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        let bytes =
            protocol::encode(&req).map_err(|e| DeliveryError::ProtocolError {
                reason: e.to_string(),
            })?;

        send.write_all(&bytes)
            .await
            .map_err(|e| DeliveryError::ConnectionFailed {
                reason: e.to_string(),
            })?;


        send.finish()
            .map_err(|e| DeliveryError::ConnectionFailed {
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

    /// Push local CRDT changes to the leader.
    pub async fn push_changes(
        &self,
        space_id: &str,
        changes: serde_json::Value,
    ) -> Result<(), DeliveryError> {
        let req = Request::SyncPush {
            space_id: space_id.to_string(),
            changes,
            ucan_token: self.ucan_token.clone(),
        };
        match self.request(req).await? {
            Response::Ok => Ok(()),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to SyncPush".to_string(),
            }),
        }
    }

    /// Pull CRDT changes from the leader.
    pub async fn pull_changes(
        &self,
        space_id: &str,
        after_timestamp: Option<&str>,
    ) -> Result<serde_json::Value, DeliveryError> {
        let req = Request::SyncPull {
            space_id: space_id.to_string(),
            after_timestamp: after_timestamp.map(|s| s.to_string()),
            ucan_token: self.ucan_token.clone(),
        };
        match self.request(req).await? {
            Response::SyncChanges { changes } => Ok(changes),
            Response::Error { message } => Err(DeliveryError::ProtocolError { reason: message }),
            _ => Err(DeliveryError::ProtocolError {
                reason: "unexpected response to SyncPull".to_string(),
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
    ) -> Result<(), DeliveryError> {
        let req = Request::MlsUploadKeyPackages {
            space_id: space_id.to_string(),
            packages,
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
    pub async fn fetch_welcomes(
        &self,
        space_id: &str,
    ) -> Result<Vec<String>, DeliveryError> {
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
    pub async fn request_rejoin(
        &self,
        space_id: &str,
    ) -> Result<String, DeliveryError> {
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
mod read_timeout_tests {
    //! Regression guard: PeerSession::request must bound the response wait.
    //!
    //! Behavioural verification requires a live iroh::Endpoint pair and a
    //! controllable path-degradation, which doesn't exist as a test fixture.
    //! A source-level guard catches accidental removal of the timeout.

    #[test]
    fn request_must_apply_read_timeout() {
        let source = include_str!("peer.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(p, _)| p)
            .unwrap_or(source);

        assert!(
            production.contains("tokio::time::timeout"),
            "PeerSession::request must wrap protocol::read_response in \
             tokio::time::timeout; otherwise a degraded QUIC path blocks \
             read for ~150s until the connection's idle timer fires"
        );
        assert!(
            production.contains("READ_TIMEOUT_SECS"),
            "the bound should reuse READ_TIMEOUT_SECS from quic_retry for \
             consistency with the invite-flow timeout"
        );
    }
}
