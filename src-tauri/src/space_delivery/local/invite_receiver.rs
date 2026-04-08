//! Lightweight connection handler that accepts PushInvite requests and
//! forwards ClaimInvite to the active leader (if any).
//!
//! Registered automatically when the QUIC endpoint starts so that
//! every device can receive space invitations — regardless of whether
//! the device is currently running in leader mode.
//!
//! When leader mode starts, the `LeaderConnectionHandler` replaces this
//! handler (it handles PushInvite too). When leader mode stops, this
//! handler is restored.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::AppHandle;

use crate::crdt::hlc::HlcService;
use crate::database::DbConnection;
use crate::peer_storage::endpoint::DeliveryConnectionHandler;

use super::leader::LeaderState;
use super::protocol::{self, Request, Response};
use super::push_invite;

/// Shared state for the invite receiver (subset of LeaderState).
pub struct InviteReceiverState {
    pub db: DbConnection,
    pub hlc: Arc<Mutex<HlcService>>,
    pub app_handle: AppHandle,
    /// Shared reference to the active leader states, keyed by space_id.
    /// Allows forwarding ClaimInvite requests even when this lightweight handler
    /// is registered instead of the full LeaderConnectionHandler.
    pub leader_states: Arc<tokio::sync::Mutex<HashMap<String, Arc<LeaderState>>>>,
}

/// Connection handler that handles PushInvite directly and forwards
/// ClaimInvite to the active leader if one is running.
pub struct InviteReceiverHandler {
    pub state: Arc<InviteReceiverState>,
}

impl DeliveryConnectionHandler for InviteReceiverHandler {
    fn handle_connection(
        &self,
        conn: iroh::endpoint::Connection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(self.handle_connection_inner(conn))
    }
}

impl InviteReceiverHandler {
    async fn handle_connection_inner(&self, conn: iroh::endpoint::Connection) {
        let remote = conn.remote_id();
        let remote_str = remote.to_string();

        loop {
            match conn.accept_bi().await {
                Ok((send, mut recv)) => {
                    let state = self.state.clone();
                    let peer = remote_str.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_stream(send, &mut recv, &state, &peer).await {
                            eprintln!("[InviteReceiver] Stream error from {peer}: {e}");
                        }
                    });
                }
                Err(_) => {
                    break;
                }
            }
        }
    }
}

async fn handle_stream(
    mut send: iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    state: &InviteReceiverState,
    _peer: &str,
) -> Result<(), String> {
    let request = protocol::read_request(recv)
        .await
        .map_err(|e| format!("Read error: {e}"))?;

    let response = match request {
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
        } => push_invite::handle_push_invite(
            &state.db,
            &state.hlc,
            &state.app_handle,
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
        ),
        Request::ClaimInvite { ref space_id, .. } => {
            // Forward to the leader for this space if one is running
            let sid = space_id.clone();
            let leaders = state.leader_states.lock().await;
            match leaders.get(&sid) {
                Some(leader_state) => {
                    let ls = leader_state.clone();
                    drop(leaders);
                    super::leader::handle_claim_invite(&ls, request).await
                }
                None => Response::Error {
                    message: format!("No leader running for space {sid}"),
                },
            }
        }
        _ => Response::Error {
            message: "Unsupported request type".to_string(),
        },
    };

    let bytes = protocol::encode(&response).map_err(|e| format!("Encode error: {e}"))?;
    send.write_all(&bytes)
        .await
        .map_err(|e| format!("Send error: {e}"))?;
    send.finish().map_err(|e| format!("Finish error: {e}"))?;

    Ok(())
}
