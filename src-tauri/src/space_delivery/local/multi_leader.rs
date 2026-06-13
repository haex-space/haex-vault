//! Unified QUIC connection handler that multiplexes across multiple spaces.
//!
//! Registered once when the QUIC endpoint starts and stays permanent.
//! Leader start/stop only modifies the internal leader map — no handler swap needed.
//! Handles PushInvite directly (no leader required), routes all other requests
//! to the appropriate LeaderState by space_id.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::AppHandle;

use crate::crdt::hlc::HlcService;
use crate::database::DbConnection;
use crate::peer_storage::endpoint::{DeliveryConnectionHandler, OwnIdentity};

use super::error::DeliveryError;
use super::leader::LeaderState;
use super::protocol::{self, Request, Response};
use super::push_invite;

/// Upper bound on consecutive 10-second "waiting for stream" heartbeats.
/// At 6 the connection is closed (60 s of zero streams). Without this, a
/// peer that completes the QUIC handshake but never opens a bi-stream
/// occupies a tokio task indefinitely and spams the log forever.
const MAX_IDLE_HEARTBEATS: u32 = 6;

/// Single QUIC handler that routes incoming requests to the correct LeaderState.
pub struct MultiSpaceLeaderHandler {
    pub leaders: Arc<tokio::sync::RwLock<HashMap<String, Arc<LeaderState>>>>,
    pub db: DbConnection,
    pub hlc: Arc<Mutex<HlcService>>,
    pub app_handle: AppHandle,
    /// Iroh endpoint id of this vault's PeerEndpoint, captured at handler
    /// construction. Used as `server_endpoint_id` in the quic_did_auth
    /// handshake initiated on every incoming delivery connection.
    pub own_endpoint_id: String,
    /// Server-side handshake precondition: the vault must have a configured
    /// device identity (`haex_devices.owner_did` + corresponding private key)
    /// before accepting delivery connections. Mirrors the same invariant on
    /// `PeerEndpoint::own_identity` — declining to answer when half-configured
    /// catches the "identity row missing for the active endpoint" failure
    /// mode early instead of letting a peer hit a confusing UCAN-audience
    /// mismatch downstream.
    pub own_identity: Arc<Mutex<Option<OwnIdentity>>>,
    /// Cryptographically verified DID per connected remote endpoint id,
    /// populated by the quic_did_auth handshake at connection-accept time.
    /// Read by request handlers to gate UCAN audience checks against the
    /// connection-bound DID rather than the request-payload `did` field.
    /// Cleared when the connection closes.
    pub endpoint_dids: Arc<tokio::sync::RwLock<HashMap<String, String>>>,
}

impl DeliveryConnectionHandler for MultiSpaceLeaderHandler {
    fn handle_connection(
        &self,
        conn: iroh::endpoint::Connection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(self.handle_connection_inner(conn))
    }
}

impl MultiSpaceLeaderHandler {
    /// Install the device identity used by the quic_did_auth handshake.
    /// Symmetric with `PeerEndpoint::set_own_identity`: must be set before
    /// the first delivery connection is accepted, otherwise the handshake
    /// precondition check closes the connection.
    pub fn set_own_identity(&self, identity: OwnIdentity) {
        if let Ok(mut slot) = self.own_identity.lock() {
            *slot = Some(identity);
        }
    }

    async fn handle_connection_inner(&self, conn: iroh::endpoint::Connection) {
        let remote = conn.remote_id();
        let remote_str = remote.to_string();

        // NOTE: the "Connection accepted" log is intentionally emitted *after*
        // the auth stream below, not here. `log_to_db` is a synchronous write
        // that takes the process-wide DB lock; placed before `open_bi` it can,
        // under CI load with concurrent CRDT writers, delay the Challenge past
        // the client's 5s `accept_bi` deadline (peer_storage/endpoint.rs:591),
        // surfacing as "accept auth stream timed out after 5s". Keeping the
        // critical accept→open_bi path free of blocking DB work avoids that.

        // -- Phase 1: DID challenge --
        //
        // The first bidirectional stream of every delivery connection is the
        // quic_did_auth handshake, server-initiated via `open_bi`. Until it
        // succeeds the connection holds no state for this peer; on success
        // the verified DID is cached in `endpoint_dids` and consumed by the
        // request handlers (C4+ commits) to gate UCAN audience checks against
        // the connection-bound DID rather than the request-payload `did`
        // field. Mirrors `peer_storage/endpoint.rs:955-1010`.
        let identity_snapshot = self
            .own_identity
            .lock()
            .ok()
            .and_then(|g| g.clone());
        if identity_snapshot.is_none() {
            crate::logging::log_to_db(
                &self.db,
                &self.hlc,
                "error",
                "MultiLeader",
                &format!(
                    "Rejecting delivery connection from {remote_str}: own identity \
                     not configured (set_own_identity must run before start)"
                ),
                None,
            );
            conn.close(3u32.into(), b"no own identity");
            return;
        }

        // Server initiates the auth stream so it can write the Challenge
        // first — `open_bi` materialises the stream on the wire as soon as
        // the server writes, which avoids a both-sides-blocked-on-read
        // deadlock that would otherwise occur if both endpoints tried to
        // read first. Same rationale as peer_storage.
        let verified_did = match conn.open_bi().await {
            Ok((mut send, mut recv)) => {
                match crate::quic_did_auth::challenge_and_verify(
                    &mut send,
                    &mut recv,
                    &self.own_endpoint_id,
                    &remote_str,
                )
                .await
                {
                    Ok(did) => did,
                    Err(e) => {
                        crate::logging::log_to_db(
                            &self.db,
                            &self.hlc,
                            "warn",
                            "MultiLeader",
                            &format!("DID-auth failed for {remote_str}: {e}"),
                            None,
                        );
                        conn.close(2u32.into(), b"did-auth failed");
                        return;
                    }
                }
            }
            Err(e) => {
                crate::logging::log_to_db(
                    &self.db,
                    &self.hlc,
                    "warn",
                    "MultiLeader",
                    &format!("Failed to open auth stream to {remote_str}: {e}"),
                    None,
                );
                conn.close(2u32.into(), b"auth stream open failed");
                return;
            }
        };

        // Mirror the eprintln in endpoint.rs into haex_logs so production builds
        // (where stderr is /dev/null) can correlate accept→stream→handler events.
        // Emitted here (post-open_bi) rather than on accept so the blocking DB
        // write stays off the accept→Challenge critical path (see NOTE above).
        crate::logging::log_to_db(
            &self.db,
            &self.hlc,
            "info",
            "MultiLeader",
            &format!("Connection accepted from {remote_str}"),
            None,
        );

        let verified_short: String = verified_did.chars().take(24).collect();
        crate::logging::log_to_db(
            &self.db,
            &self.hlc,
            "info",
            "MultiLeader",
            &format!("DID-auth ok: {remote_str} -> {verified_short}"),
            None,
        );

        self.endpoint_dids
            .write()
            .await
            .insert(remote_str.clone(), verified_did.clone());

        // -- Phase 2: normal request loop --

        let mut stream_count: u32 = 0;
        let mut idle_heartbeats: u32 = 0;
        let connection_start = std::time::Instant::now();
        loop {
            // Heartbeat every 10s while waiting for a stream — surfaces the
            // "QUIC TLS handshake done, but no bi-stream ever opens" failure
            // mode where the sender silently drops after its 10s send-timeout.
            let accept = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                conn.accept_bi(),
            )
            .await;

            match accept {
                Ok(Ok((send, mut recv))) => {
                    stream_count += 1;
                    idle_heartbeats = 0;
                    let leaders = self.leaders.clone();
                    let db = DbConnection(self.db.0.clone());
                    let hlc = self.hlc.clone();
                    let app_handle = self.app_handle.clone();
                    let peer_endpoint_id = remote_str.clone();
                    let verified_did_for_stream = verified_did.clone();
                    let stream_index = stream_count;
                    tokio::spawn(async move {
                        if let Err(e) = handle_stream(
                            send,
                            &mut recv,
                            &leaders,
                            &db,
                            &hlc,
                            &app_handle,
                            &peer_endpoint_id,
                            &verified_did_for_stream,
                        )
                        .await
                        {
                            let msg = format!(
                                "Stream {stream_index} error from {peer_endpoint_id}: {e}"
                            );
                            crate::logging::log_to_db(&db, &hlc, "error", "MultiLeader", &msg, None);
                        }
                    });
                }
                Ok(Err(e)) => {
                    let msg = format!(
                        "Connection from {remote_str} closed after {streams} stream(s), {secs}s: {e}",
                        streams = stream_count,
                        secs = connection_start.elapsed().as_secs()
                    );
                    crate::logging::log_to_db(&self.db, &self.hlc, "info", "MultiLeader", &msg, None);
                    break;
                }
                Err(_) => {
                    idle_heartbeats += 1;
                    // Heartbeat: connection still open but no stream opened yet.
                    let msg = format!(
                        "Waiting for stream from {remote_str} ({}s elapsed, {} streams handled, idle={}/{})",
                        connection_start.elapsed().as_secs(),
                        stream_count,
                        idle_heartbeats,
                        MAX_IDLE_HEARTBEATS,
                    );
                    crate::logging::log_to_db(&self.db, &self.hlc, "warn", "MultiLeader", &msg, None);

                    if idle_heartbeats >= MAX_IDLE_HEARTBEATS {
                        // Misbehaving client occupies a tokio task and spams
                        // the log indefinitely. Cap the wait and close.
                        let msg = format!(
                            "Closing idle connection from {remote_str} after {}s ({} idle heartbeats, 0 streams)",
                            connection_start.elapsed().as_secs(),
                            idle_heartbeats,
                        );
                        crate::logging::log_to_db(&self.db, &self.hlc, "warn", "MultiLeader", &msg, None);
                        conn.close(0u32.into(), b"idle timeout");
                        break;
                    }
                }
            }
        }

        // Drop the cached verified DID — once the QUIC connection is gone
        // the (endpoint_id -> DID) binding established by the handshake no
        // longer applies. A future reconnect repeats the handshake.
        self.endpoint_dids.write().await.remove(&remote_str);

        // Clean up peer state across all active leaders
        let leaders = self.leaders.read().await;
        for leader in leaders.values() {
            leader.connected_peers.write().await.remove(&remote_str);
            leader.notification_senders.write().await.remove(&remote_str);
        }
    }
}

/// Extract the space_id from a request, if it carries one.
fn extract_space_id(request: &Request) -> Option<&str> {
    match request {
        Request::MlsUploadKeyPackages { space_id, .. }
        | Request::MlsFetchKeyPackage { space_id, .. }
        | Request::MlsSendMessage { space_id, .. }
        | Request::MlsFetchMessages { space_id, .. }
        | Request::MlsSendWelcome { space_id, .. }
        | Request::MlsFetchWelcomes { space_id }
        | Request::SyncPush { space_id, .. }
        | Request::SyncPull { space_id, .. }
        | Request::Announce { space_id, .. }
        | Request::ClaimInvite { space_id, .. }
        | Request::MlsAckCommit { space_id, .. }
        | Request::MlsKeyPackageCount { space_id, .. }
        | Request::RequestRejoin { space_id, .. }
        | Request::SubmitExternalCommit { space_id, .. } => Some(space_id.as_str()),
        Request::PushInvite { .. } => None,
    }
}

/// Handle a single bidirectional QUIC stream: read request, route, respond.
///
/// `verified_did` is the cryptographically authenticated DID of the connected
/// peer, established by the server-initiated quic_did_auth handshake at
/// connection-accept time. It is plumbed into every request handler so the
/// later commits in this PR can gate auth checks on it (claim binding,
/// announce binding, UCAN audience match, inviter spoofing reject).
async fn handle_stream(
    mut send: iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    leaders: &tokio::sync::RwLock<HashMap<String, Arc<LeaderState>>>,
    db: &DbConnection,
    hlc: &Arc<Mutex<HlcService>>,
    app_handle: &AppHandle,
    peer_endpoint_id: &str,
    verified_did: &str,
) -> Result<(), DeliveryError> {
    let request = protocol::read_request(recv)
        .await
        .map_err(|e| DeliveryError::ProtocolError {
            reason: e.to_string(),
        })?;

    let response = match request {
        // PushInvite needs no leader — handle directly
        Request::PushInvite {
            space_id,
            space_name,
            space_type,
            token_id,
            capabilities,
            include_history,
            inviter_did,
            inviter_label,
            inviter_avatar,
            inviter_avatar_options,
            space_endpoints,
            origin_url,
            expires_at: _,
            inviter_relay_url,
        } => {
            crate::logging::log_to_db(db, hlc, "info", "MultiLeader", &format!(
                "PushInvite received from {peer_endpoint_id} → space={} inviter={}",
                &space_id[..8.min(space_id.len())], &inviter_did[..24.min(inviter_did.len())]
            ), None);
            push_invite::handle_push_invite(
                db,
                hlc,
                app_handle,
                &space_id,
                &space_name,
                &space_type,
                &token_id,
                &capabilities,
                include_history,
                &inviter_did,
                inviter_label.as_deref(),
                inviter_avatar.as_deref(),
                inviter_avatar_options.as_deref(),
                &space_endpoints,
                origin_url.as_deref(),
                inviter_relay_url.as_deref(),
                verified_did,
            )
        }

        // ClaimInvite — look up the leader for the space
        Request::ClaimInvite { ref space_id, .. } => {
            crate::logging::log_to_db(db, hlc, "info", "MultiLeader", &format!(
                "ClaimInvite received from {peer_endpoint_id} for space {}",
                &space_id[..8.min(space_id.len())]
            ), None);
            let map = leaders.read().await;
            let active_spaces: Vec<String> = map.keys().map(|k| k[..8.min(k.len())].to_string()).collect();
            match map.get(space_id.as_str()) {
                Some(leader) => {
                    crate::logging::log_to_db(db, hlc, "info", "MultiLeader", &format!(
                        "Routing ClaimInvite to leader for space {}", &space_id[..8.min(space_id.len())]
                    ), None);
                    super::leader::handle_claim_invite(leader, request, verified_did).await
                }
                None => {
                    crate::logging::log_to_db(db, hlc, "error", "MultiLeader", &format!(
                        "No leader active for space {} (active: {:?})", space_id, active_spaces
                    ), None);
                    Response::Error {
                        message: format!("No leader active for space {space_id}"),
                    }
                }
            }
        }

        // All other requests require a leader for the space
        other => {
            let space_id = extract_space_id(&other);
            match space_id {
                Some(sid) => {
                    let map = leaders.read().await;
                    match map.get(sid) {
                        Some(leader) => {
                            super::leader::handle_delivery_request(
                                leader,
                                other,
                                peer_endpoint_id,
                                verified_did,
                            )
                            .await
                        }
                        None => Response::Error {
                            message: format!("No leader active for space {sid}"),
                        },
                    }
                }
                None => Response::Error {
                    message: "Request type not supported".to_string(),
                },
            }
        }
    };

    super::leader::send_response(&mut send, &response).await
}

#[cfg(test)]
mod idle_close_tests {
    use super::*;

    #[test]
    fn max_idle_heartbeats_is_bounded_and_finite() {
        assert!(MAX_IDLE_HEARTBEATS > 0, "must be a positive bound");
        assert!(
            MAX_IDLE_HEARTBEATS <= 60,
            "60 heartbeats = 10 minutes; anything beyond that defeats the \
             purpose of the bound (preventing log-spam from misbehaving peers)"
        );
    }

    /// Regression guard: the connection-accept loop in
    /// MultiSpaceLeaderHandler must break out once MAX_IDLE_HEARTBEATS is
    /// reached. Without this, a peer that completes the handshake but
    /// never opens a stream wedges a tokio task and spams the log.
    #[test]
    fn accept_loop_closes_on_idle_bound() {
        let source = include_str!("multi_leader.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(p, _)| p)
            .unwrap_or(source);

        assert!(
            production.contains("idle_heartbeats >= MAX_IDLE_HEARTBEATS"),
            "the heartbeat branch must check the idle counter against \
             MAX_IDLE_HEARTBEATS and break the loop"
        );
        assert!(
            production.contains("conn.close(0u32.into(), b\"idle timeout\")"),
            "the loop must close the QUIC connection on the idle path so \
             the peer learns the channel is gone instead of hanging"
        );
    }
}
