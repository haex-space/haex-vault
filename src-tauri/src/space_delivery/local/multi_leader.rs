//! Unified QUIC connection handler that multiplexes across multiple spaces.
//!
//! Registered once when the QUIC endpoint starts and stays permanent.
//! Leader start/stop only modifies the internal leader map — no handler swap needed.
//! Handles PushInvite directly (no leader required), routes all other requests
//! to the appropriate LeaderState by space_id.

use std::collections::HashMap;
use std::sync::Arc;

use tauri::AppHandle;

use crate::crdt::hlc::HlcService;
use crate::database::DbConnection;
use crate::peer_storage::endpoint::DeliveryConnectionHandler;

use super::error::DeliveryError;
use super::leader::LeaderState;
use super::protocol::{self, Request, Response};
use super::push_invite;

/// Single QUIC handler that routes incoming requests to the correct LeaderState.
pub struct MultiSpaceLeaderHandler {
    pub leaders: Arc<tokio::sync::RwLock<HashMap<String, Arc<LeaderState>>>>,
    pub db: DbConnection,
    pub hlc: Arc<std::sync::Mutex<HlcService>>,
    pub app_handle: AppHandle,
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
    async fn handle_connection_inner(&self, conn: iroh::endpoint::Connection) {
        let remote = conn.remote_id();
        let remote_str = remote.to_string();

        loop {
            match conn.accept_bi().await {
                Ok((send, mut recv)) => {
                    let leaders = self.leaders.clone();
                    let db = DbConnection(self.db.0.clone());
                    let hlc = self.hlc.clone();
                    let app_handle = self.app_handle.clone();
                    let peer_endpoint_id = remote_str.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_stream(
                            send,
                            &mut recv,
                            &leaders,
                            &db,
                            &hlc,
                            &app_handle,
                            &peer_endpoint_id,
                        )
                        .await
                        {
                            eprintln!("[MultiLeader] Stream error from {peer_endpoint_id}: {e}");
                        }
                    });
                }
                Err(_) => {
                    eprintln!("[MultiLeader] Connection from {remote_str} closed");
                    break;
                }
            }
        }

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
        | Request::ClaimInvite { space_id, .. } => Some(space_id.as_str()),
        Request::PushInvite { .. } => None,
    }
}

/// Handle a single bidirectional QUIC stream: read request, route, respond.
async fn handle_stream(
    mut send: iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    leaders: &tokio::sync::RwLock<HashMap<String, Arc<LeaderState>>>,
    db: &DbConnection,
    hlc: &Arc<std::sync::Mutex<HlcService>>,
    app_handle: &AppHandle,
    peer_endpoint_id: &str,
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
            space_endpoints,
            origin_url,
            expires_at: _,
        } => {
            crate::logging::log_to_db(db, hlc, "info", "MultiLeader", &format!(
                "PushInvite received from {peer_endpoint_id} → space={} inviter={}",
                &space_id[..8.min(space_id.len())], &inviter_did[..24.min(inviter_did.len())]
            ));
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
                &space_endpoints,
                origin_url.as_deref(),
            )
        }

        // ClaimInvite — look up the leader for the space
        Request::ClaimInvite { ref space_id, .. } => {
            crate::logging::log_to_db(db, hlc, "info", "MultiLeader", &format!(
                "ClaimInvite received from {peer_endpoint_id} for space {}",
                &space_id[..8.min(space_id.len())]
            ));
            let map = leaders.read().await;
            let active_spaces: Vec<String> = map.keys().map(|k| k[..8.min(k.len())].to_string()).collect();
            match map.get(space_id.as_str()) {
                Some(leader) => {
                    crate::logging::log_to_db(db, hlc, "info", "MultiLeader", &format!(
                        "Routing ClaimInvite to leader for space {}", &space_id[..8.min(space_id.len())]
                    ));
                    super::leader::handle_claim_invite(leader, request).await
                }
                None => {
                    crate::logging::log_to_db(db, hlc, "error", "MultiLeader", &format!(
                        "No leader active for space {} (active: {:?})", space_id, active_spaces
                    ));
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
