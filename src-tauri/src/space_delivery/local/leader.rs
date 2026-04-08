//! Leader-side delivery: connection handler, request dispatch, state management.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use time::OffsetDateTime;
use tokio::sync::RwLock;

use tauri::AppHandle;

use crate::crdt::commands::{apply_remote_changes_to_db, RemoteColumnChange};
use crate::crdt::hlc::HlcService;
use crate::crdt::scanner::{scan_all_crdt_tables_for_local_changes, LocalColumnChange};
use crate::database::DbConnection;
use crate::peer_storage::endpoint::DeliveryConnectionHandler;

use super::buffer;
use super::error::DeliveryError;
use super::invite_tokens::{self, LocalInviteToken};
use super::protocol::{self, MlsMessageEntry, Notification, Request, Response};
use super::push_invite;
use super::types::{ConnectedPeer, PeerClaim};
use serde_json::Value as JsonValue;

// ============================================================================
// State
// ============================================================================

/// State held by the leader for active delivery sessions.
pub struct LeaderState {
    /// Database connection
    pub db: DbConnection,
    /// HLC service for CRDT-synced writes
    pub hlc: Arc<Mutex<HlcService>>,
    /// Tauri AppHandle for emitting events to the frontend
    pub app_handle: AppHandle,
    /// Space ID this leader serves
    pub space_id: String,
    /// Currently connected peers (endpoint_id → peer info) — IN-MEMORY ONLY, never persisted
    pub connected_peers: Arc<RwLock<HashMap<String, ConnectedPeer>>>,
    /// Notification senders for connected peers (endpoint_id → sender)
    pub notification_senders:
        Arc<RwLock<HashMap<String, tokio::sync::mpsc::Sender<Notification>>>>,
    /// In-memory invite tokens (loaded from DB on start, synced back on changes)
    pub invite_tokens: Arc<RwLock<Vec<LocalInviteToken>>>,
}

// ============================================================================
// Connection handler
// ============================================================================

fn base64_encode(data: &[u8]) -> String {
    BASE64.encode(data)
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    BASE64.decode(s).map_err(|e| format!("base64 decode error: {e}"))
}

/// Connection handler for the leader side of space delivery.
pub struct LeaderConnectionHandler {
    pub state: Arc<LeaderState>,
}

impl DeliveryConnectionHandler for LeaderConnectionHandler {
    fn handle_connection(
        &self,
        conn: iroh::endpoint::Connection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(self.handle_connection_inner(conn))
    }
}

impl LeaderConnectionHandler {
    async fn handle_connection_inner(&self, conn: iroh::endpoint::Connection) {
        let remote = conn.remote_id();
        let remote_str = remote.to_string();

        loop {
            match conn.accept_bi().await {
                Ok((send, mut recv)) => {
                    let state = self.state.clone();
                    let peer_endpoint_id = remote_str.clone();
                    tokio::spawn(async move {
                        if let Err(e) =
                            handle_delivery_stream(send, &mut recv, &state, &peer_endpoint_id).await
                        {
                            eprintln!(
                                "[SpaceDelivery] Stream error from {peer_endpoint_id}: {e}"
                            );
                        }
                    });
                }
                Err(_) => {
                    eprintln!("[SpaceDelivery] Connection from {remote_str} closed");
                    break;
                }
            }
        }

        // Clean up peer state on disconnect
        self.state
            .connected_peers
            .write()
            .await
            .remove(&remote_str);
        self.state
            .notification_senders
            .write()
            .await
            .remove(&remote_str);
    }
}

/// Look up the DID for a connected peer by endpoint_id.
async fn lookup_peer_did(
    state: &LeaderState,
    endpoint_id: &str,
) -> Result<String, DeliveryError> {
    let peers = state.connected_peers.read().await;
    peers
        .get(endpoint_id)
        .map(|p| p.did.clone())
        .ok_or_else(|| DeliveryError::ProtocolError {
            reason: "peer has not announced yet".to_string(),
        })
}

/// Check if a peer has write capability for the given space.
/// Returns Ok(()) if the peer has space/write or space/admin, Err otherwise.
fn check_write_capability(
    db: &crate::database::DbConnection,
    peer_did: &str,
    space_id: &str,
) -> Result<(), DeliveryError> {
    let rows = crate::database::core::select_with_crdt(
        "SELECT capability FROM haex_ucan_tokens WHERE space_id = ?1 AND audience_did = ?2"
            .to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(peer_did.to_string()),
        ],
        db,
    )
    .unwrap_or_default();

    for row in &rows {
        if let Some(cap) = row.first().and_then(|v| v.as_str()) {
            if cap == "space/write" || cap == "space/admin" {
                return Ok(());
            }
        }
    }

    Err(DeliveryError::AccessDenied {
        reason: format!(
            "Peer {peer_did} does not have space/write capability for space {space_id}"
        ),
    })
}

// ============================================================================
// ClaimInvite handler (used by both LeaderConnectionHandler and InviteReceiverHandler)
// ============================================================================

/// Handle a ClaimInvite request. Extracted so the InviteReceiverHandler can
/// forward ClaimInvite to the active leader without replacing the whole handler.
pub async fn handle_claim_invite(state: &LeaderState, request: Request) -> Response {
    let (space_id, token, did, endpoint_id, key_packages, label, public_key) = match request {
        Request::ClaimInvite {
            space_id,
            token,
            did,
            endpoint_id,
            key_packages,
            label,
            public_key,
        } => (space_id, token, did, endpoint_id, key_packages, label, public_key),
        _ => {
            return Response::Error {
                message: "Expected ClaimInvite request".to_string(),
            }
        }
    };

    if space_id != state.space_id {
        return Response::Error {
            message: format!("Wrong space: expected {}", state.space_id),
        };
    }

    // 1. Validate and consume invite token
    let (capability, pre_ucan) =
        match invite_tokens::validate_and_consume_invite(
            &state.db,
            &state.hlc,
            &state.invite_tokens,
            &token,
            &did,
        )
        .await
        {
            Ok(result) => result,
            Err(e) => {
                return Response::Error {
                    message: e.to_string(),
                }
            }
        };

    // 2. Determine UCAN: use pre-created (contact) or create now (conference)
    let ucan_token = match pre_ucan {
        Some(ucan) => ucan,
        None => {
            let admin = match super::ucan::load_admin_identity(&state.db, &space_id) {
                Ok(a) => a,
                Err(e) => {
                    return Response::Error {
                        message: format!("Failed to load admin identity: {e}"),
                    }
                }
            };
            match super::ucan::create_delegated_ucan(
                &admin.did,
                &admin.private_key_base64,
                &did,
                &space_id,
                &capability,
                Some(&admin.root_ucan),
                86400 * 365,
            ) {
                Ok(t) => t,
                Err(e) => {
                    return Response::Error {
                        message: format!("Failed to create UCAN: {e}"),
                    }
                }
            }
        }
    };

    // 3. Store key packages from invitee
    for pkg_b64 in &key_packages {
        if let Ok(blob) = base64_decode(pkg_b64) {
            let _ = buffer::store_key_package(&state.db, &space_id, &did, &blob);
        }
    }

    // 4. Consume one key package for MLS add_member
    let key_package_blob = match buffer::consume_key_package(&state.db, &space_id, &did) {
        Ok(Some(blob)) => blob,
        Ok(None) => {
            return Response::Error {
                message: "No key package available after upload".to_string(),
            }
        }
        Err(e) => {
            return Response::Error {
                message: format!("Key package error: {e}"),
            }
        }
    };

    // 5. MLS add_member
    let mls_manager = crate::mls::manager::MlsManager::new(state.db.0.clone());
    let bundle = match mls_manager.add_member(&space_id, &key_package_blob) {
        Ok(b) => b,
        Err(e) => {
            return Response::Error {
                message: format!("MLS add_member failed: {e}"),
            }
        }
    };

    // 6. Store and broadcast commit to existing members
    if !bundle.commit.is_empty() {
        let _ = buffer::store_message(&state.db, &space_id, &did, "commit", &bundle.commit);
        let senders = state.notification_senders.read().await;
        for (_, sender) in senders.iter() {
            let _ = sender.try_send(Notification::Mls {
                space_id: space_id.clone(),
                message_type: "commit".to_string(),
            });
        }
    }

    // 7. Register peer as connected
    let member_label = label.clone();
    state.connected_peers.write().await.insert(
        endpoint_id.clone(),
        ConnectedPeer {
            endpoint_id,
            did: did.clone(),
            label,
            claims: vec![],
            connected_at: OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        },
    );

    // 7b. Persist new member to haex_space_members (CRDT-synced to all devices)
    if let Some(ref pk) = public_key {
        let hlc = state.hlc.lock().map_err(|e| format!("HLC lock error: {e}")).ok();
        if let Some(ref hlc_guard) = hlc {
            let member_id = uuid::Uuid::new_v4().to_string();
            let now = OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default();
            let resolved_label = member_label
                .unwrap_or_else(|| did.chars().take(16).collect());

            let sql = "INSERT OR IGNORE INTO haex_space_members \
                (id, space_id, member_did, member_public_key, label, role, joined_at) \
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)".to_string();

            let params = vec![
                JsonValue::String(member_id),
                JsonValue::String(space_id.clone()),
                JsonValue::String(did.clone()),
                JsonValue::String(pk.clone()),
                JsonValue::String(resolved_label),
                JsonValue::String(capability.clone()),
                JsonValue::String(now),
            ];

            if let Err(e) = crate::database::core::execute_with_crdt(
                sql, params, &state.db, hlc_guard,
            ) {
                eprintln!("[SpaceDelivery] Failed to persist space member: {e}");
            }
        }
    }

    // 8. Return welcome + UCAN
    match bundle.welcome {
        Some(welcome) => Response::InviteClaimed {
            welcome: base64_encode(&welcome),
            ucan: ucan_token,
            capability,
        },
        None => Response::Error {
            message: "MLS add_member produced no welcome".to_string(),
        },
    }
}

// ============================================================================
// Request dispatcher
// ============================================================================

/// Process a single request/response exchange on a QUIC bidirectional stream.
async fn handle_delivery_stream(
    mut send: iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    state: &LeaderState,
    peer_endpoint_id: &str,
) -> Result<(), DeliveryError> {
    let request =
        protocol::read_request(recv)
            .await
            .map_err(|e| DeliveryError::ProtocolError {
                reason: e.to_string(),
            })?;

    let response = match request {
        Request::Announce {
            did,
            endpoint_id,
            label,
            claims,
        } => {
            let peer = ConnectedPeer {
                endpoint_id: endpoint_id.clone(),
                did,
                label,
                claims: claims
                    .unwrap_or_default()
                    .into_iter()
                    .map(|c| PeerClaim {
                        claim_type: c.claim_type,
                        value: c.value,
                    })
                    .collect(),
                connected_at: OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            };
            state
                .connected_peers
                .write()
                .await
                .insert(endpoint_id, peer);
            Response::Ok
        }

        // -- MLS Key Packages --
        Request::MlsUploadKeyPackages {
            space_id,
            packages,
        } => {
            if space_id != state.space_id {
                return send_response(
                    &mut send,
                    &Response::Error {
                        message: format!("Wrong space: expected {}", state.space_id),
                    },
                )
                .await;
            }
            let did = lookup_peer_did(state, peer_endpoint_id).await?;
            for pkg_b64 in &packages {
                if let Ok(blob) = base64_decode(pkg_b64) {
                    let _ = buffer::store_key_package(&state.db, &space_id, &did, &blob);
                }
            }
            Response::Ok
        }

        Request::MlsFetchKeyPackage {
            space_id,
            target_did,
        } => {
            if space_id != state.space_id {
                return send_response(
                    &mut send,
                    &Response::Error {
                        message: format!("Wrong space: expected {}", state.space_id),
                    },
                )
                .await;
            }
            match buffer::consume_key_package(&state.db, &space_id, &target_did) {
                Ok(Some(blob)) => Response::KeyPackage {
                    package: base64_encode(&blob),
                },
                Ok(None) => Response::Error {
                    message: format!("No key package for {target_did}"),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        // -- MLS Messages --
        Request::MlsSendMessage {
            space_id,
            message,
            message_type,
        } => {
            if space_id != state.space_id {
                return send_response(
                    &mut send,
                    &Response::Error {
                        message: format!("Wrong space: expected {}", state.space_id),
                    },
                )
                .await;
            }
            let did = lookup_peer_did(state, peer_endpoint_id).await?;
            match base64_decode(&message) {
                Ok(blob) => {
                    match buffer::store_message(&state.db, &space_id, &did, &message_type, &blob) {
                        Ok(id) => {
                            // Notify all connected peers
                            let senders = state.notification_senders.read().await;
                            for (_, sender) in senders.iter() {
                                let _ = sender.try_send(Notification::Mls {
                                    space_id: space_id.clone(),
                                    message_type: message_type.clone(),
                                });
                            }
                            Response::MessageStored { message_id: id }
                        }
                        Err(e) => Response::Error {
                            message: e.to_string(),
                        },
                    }
                }
                Err(e) => Response::Error { message: e },
            }
        }

        Request::MlsFetchMessages {
            space_id,
            after_id,
        } => {
            if space_id != state.space_id {
                return send_response(
                    &mut send,
                    &Response::Error {
                        message: format!("Wrong space: expected {}", state.space_id),
                    },
                )
                .await;
            }
            match buffer::fetch_messages(&state.db, &space_id, after_id) {
                Ok(msgs) => {
                    let entries: Vec<MlsMessageEntry> = msgs
                        .into_iter()
                        .map(|(id, sender_did, msg_type, blob, created_at)| MlsMessageEntry {
                            id,
                            sender_did,
                            message_type: msg_type,
                            message: base64_encode(&blob),
                            created_at,
                        })
                        .collect();
                    Response::Messages { messages: entries }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        // -- MLS Welcomes --
        Request::MlsSendWelcome {
            space_id,
            recipient_did,
            welcome,
        } => {
            if space_id != state.space_id {
                return send_response(
                    &mut send,
                    &Response::Error {
                        message: format!("Wrong space: expected {}", state.space_id),
                    },
                )
                .await;
            }
            match base64_decode(&welcome) {
                Ok(blob) => {
                    match buffer::store_welcome(&state.db, &space_id, &recipient_did, &blob) {
                        Ok(_) => Response::Ok,
                        Err(e) => Response::Error {
                            message: e.to_string(),
                        },
                    }
                }
                Err(e) => Response::Error { message: e },
            }
        }

        Request::MlsFetchWelcomes { space_id } => {
            if space_id != state.space_id {
                return send_response(
                    &mut send,
                    &Response::Error {
                        message: format!("Wrong space: expected {}", state.space_id),
                    },
                )
                .await;
            }
            let did = lookup_peer_did(state, peer_endpoint_id).await?;
            match buffer::consume_welcomes(&state.db, &space_id, &did) {
                Ok(blobs) => {
                    let encoded: Vec<String> = blobs.iter().map(|b| base64_encode(b)).collect();
                    Response::Welcomes { welcomes: encoded }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        // -- CRDT Sync --
        Request::SyncPush { space_id, changes } => {
            if space_id != state.space_id {
                return send_response(
                    &mut send,
                    &Response::Error {
                        message: format!("Wrong space: expected {}", state.space_id),
                    },
                )
                .await;
            }

            // Capability enforcement: only peers with space/write or space/admin
            // may push CRDT changes. Read-only peers are rejected.
            match lookup_peer_did(state, peer_endpoint_id).await {
                Ok(did) => {
                    if let Err(e) = check_write_capability(&state.db, &did, &space_id) {
                        eprintln!("[SpaceDelivery] SyncPush REJECTED: {e}");
                        return send_response(
                            &mut send,
                            &Response::Error {
                                message: format!("Access denied: {e}"),
                            },
                        )
                        .await;
                    }
                }
                Err(e) => {
                    eprintln!("[SpaceDelivery] SyncPush: peer not announced: {e}");
                    return send_response(
                        &mut send,
                        &Response::Error {
                            message: "Peer has not announced — cannot verify write capability"
                                .to_string(),
                        },
                    )
                    .await;
                }
            }

            // 1. Parse changes JSON into Vec<LocalColumnChange>
            let local_changes: Vec<LocalColumnChange> = match serde_json::from_value(changes) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[SpaceDelivery] SyncPush: failed to parse changes: {e}");
                    return send_response(
                        &mut send,
                        &Response::Error {
                            message: format!("Invalid changes JSON: {e}"),
                        },
                    )
                    .await;
                }
            };

            if local_changes.is_empty() {
                return send_response(&mut send, &Response::Ok).await;
            }

            // 2. Convert to RemoteColumnChange
            let batch_id = uuid::Uuid::new_v4().to_string();
            let total = local_changes.len();
            let remote_changes: Vec<RemoteColumnChange> = local_changes
                .iter()
                .enumerate()
                .map(|(i, local)| {
                    super::sync_loop::local_to_remote_change(local, &batch_id, i + 1, total)
                })
                .collect();

            // Collect affected table names and max HLC before applying
            let affected_tables: Vec<String> = local_changes
                .iter()
                .map(|c| c.table_name.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            // 3. Apply changes to DB (HLC clock is advanced internally)
            let hlc_service = state.hlc.lock().ok().map(|guard| guard.clone());
            if let Err(e) = apply_remote_changes_to_db(
                &state.db,
                remote_changes,
                None,
                hlc_service.as_ref(),
            ) {
                eprintln!("[SpaceDelivery] SyncPush: failed to apply changes: {e}");
                return send_response(
                    &mut send,
                    &Response::Error {
                        message: format!("Failed to apply changes: {e}"),
                    },
                )
                .await;
            }

            // 6. Notify other connected peers (except the sender)
            {
                let senders = state.notification_senders.read().await;
                for (endpoint_id, sender) in senders.iter() {
                    if endpoint_id != peer_endpoint_id {
                        let _ = sender.try_send(Notification::Sync {
                            space_id: space_id.clone(),
                            tables: affected_tables.clone(),
                        });
                    }
                }
            }

            Response::Ok
        }

        Request::SyncPull {
            space_id,
            after_timestamp,
        } => {
            if space_id != state.space_id {
                return send_response(
                    &mut send,
                    &Response::Error {
                        message: format!("Wrong space: expected {}", state.space_id),
                    },
                )
                .await;
            }

            let device_id = "leader";
            match scan_all_crdt_tables_for_local_changes(
                &state.db,
                after_timestamp.as_deref(),
                device_id,
            ) {
                Ok(changes) => match serde_json::to_value(&changes) {
                    Ok(json) => Response::SyncChanges { changes: json },
                    Err(e) => {
                        eprintln!("[SpaceDelivery] SyncPull: failed to serialize changes: {e}");
                        Response::Error {
                            message: format!("Failed to serialize changes: {e}"),
                        }
                    }
                },
                Err(e) => {
                    eprintln!("[SpaceDelivery] SyncPull: failed to scan changes: {e}");
                    Response::Error {
                        message: format!("Failed to scan changes: {e}"),
                    }
                }
            }
        }

        // -- Invites (ClaimInvite) --
        req @ Request::ClaimInvite { .. } => {
            handle_claim_invite(state, req).await
        }

        // -- Push Invites (peer-to-peer, invitee side) --
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
    };

    send_response(&mut send, &response).await
}

/// Encode and send a response on the QUIC send stream, then finish.
async fn send_response(
    send: &mut iroh::endpoint::SendStream,
    response: &Response,
) -> Result<(), DeliveryError> {
    let bytes = protocol::encode(response).map_err(|e| DeliveryError::ProtocolError {
        reason: format!("Failed to encode response: {e}"),
    })?;
    send.write_all(&bytes)
        .await
        .map_err(|e| DeliveryError::ProtocolError {
            reason: format!("Failed to send response: {e}"),
        })?;
    send.finish().map_err(|e| DeliveryError::ProtocolError {
        reason: format!("Failed to finish send: {e}"),
    })?;
    Ok(())
}
