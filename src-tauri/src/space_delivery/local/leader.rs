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
use super::buffer;
use super::error::DeliveryError;
use super::invite_tokens::{self, LocalInviteToken};
use super::protocol::{self, MlsMessageEntry, Notification, Request, Response};
use super::push_invite;
use super::types::{ConnectedPeer, PeerClaim};
use serde_json::Value as JsonValue;

/// Target number of key packages the leader wants each peer to maintain.
const TARGET_KEY_PACKAGES_PER_PEER: u32 = 10;

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
// Helpers
// ============================================================================

fn base64_encode(data: &[u8]) -> String {
    BASE64.encode(data)
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    BASE64.decode(s).map_err(|e| format!("base64 decode error: {e}"))
}

/// Look up the DID for a connected peer, returning an error Response on failure.
async fn require_peer_did(state: &LeaderState, endpoint_id: &str) -> Result<String, Response> {
    state
        .connected_peers
        .read()
        .await
        .get(endpoint_id)
        .map(|p| p.did.clone())
        .ok_or_else(|| Response::Error {
            message: "Peer has not announced".to_string(),
        })
}

/// Check if a peer has any UCAN capability for the given space.
/// Returns Ok(()) if the peer has any valid UCAN (read, write, invite, or admin).
fn check_space_membership(
    db: &crate::database::DbConnection,
    peer_did: &str,
    space_id: &str,
) -> Result<(), DeliveryError> {
    let rows = crate::database::core::select_with_crdt(
        "SELECT COUNT(*) FROM haex_ucan_tokens WHERE space_id = ?1 AND audience_did = ?2"
            .to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(peer_did.to_string()),
        ],
        db,
    )
    .unwrap_or_default();

    let count = rows
        .first()
        .and_then(|row| row.first())
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    if count > 0 {
        Ok(())
    } else {
        Err(DeliveryError::AccessDenied {
            reason: format!(
                "Peer {peer_did} has no UCAN for space {space_id}"
            ),
        })
    }
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

/// Broadcast an MLS notification to all connected peers.
async fn notify_all_mls(state: &LeaderState, space_id: &str, message_type: &str) {
    let senders = state.notification_senders.read().await;
    for (_, sender) in senders.iter() {
        let _ = sender.try_send(Notification::Mls {
            space_id: space_id.to_string(),
            message_type: message_type.to_string(),
        });
    }
}

/// Broadcast a sync notification to all peers except the sender.
async fn notify_others_sync(
    state: &LeaderState,
    space_id: &str,
    tables: &[String],
    exclude_endpoint: &str,
) {
    let senders = state.notification_senders.read().await;
    for (endpoint_id, sender) in senders.iter() {
        if endpoint_id != exclude_endpoint {
            let _ = sender.try_send(Notification::Sync {
                space_id: space_id.to_string(),
                tables: tables.to_vec(),
            });
        }
    }
}

// ============================================================================
// ClaimInvite handler
// ============================================================================

/// Handle a ClaimInvite request.
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

    debug_assert_eq!(space_id, state.space_id, "ClaimInvite routed to wrong leader");

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
    eprintln!(
        "[SpaceDelivery] ClaimInvite: adding {} to MLS group {} (key_package {} bytes)",
        &did[..20.min(did.len())],
        &space_id[..12.min(space_id.len())],
        key_package_blob.len(),
    );
    let bundle = match crate::mls::blocking::add_member(
        state.db.0.clone(),
        space_id.clone(),
        key_package_blob,
    )
    .await
    {
        Ok(b) => b,
        Err(e) => {
            return Response::Error {
                message: format!("MLS add_member failed: {e}"),
            }
        }
    };

    // 6. Store and broadcast commit to existing members
    if !bundle.commit.is_empty() {
        let msg_id = match buffer::store_message(&state.db, &space_id, &did, "commit", &bundle.commit) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("[SpaceDelivery] Failed to store commit: {e}");
                0
            }
        };

        // Track pending ACKs from all space members (not just connected peers)
        if msg_id > 0 {
            let expected_dids: Vec<String> = buffer::get_space_member_dids(&state.db, &space_id)
                .unwrap_or_default()
                .into_iter()
                .filter(|d| d != &did) // exclude the new member (gets Welcome, not commit)
                .collect();
            if !expected_dids.is_empty() {
                let _ = buffer::store_pending_commit(&state.db, &space_id, msg_id, &expected_dids);
            }
        }

        notify_all_mls(state, &space_id, "commit").await;
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

/// Dispatch an already-parsed request to the appropriate handler and return the response.
/// Called by `MultiSpaceLeaderHandler` after routing to the correct `LeaderState` by space_id.
pub(super) async fn handle_delivery_request(
    state: &LeaderState,
    request: Request,
    peer_endpoint_id: &str,
) -> Response {
    match request {
        Request::Announce {
            did,
            endpoint_id,
            space_id: _,
            label,
            claims,
        } => {
            let did_clone = did.clone();
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
                .insert(endpoint_id.clone(), peer);

            // Re-notify about unacked commits for this peer
            let unacked = buffer::get_unacked_message_ids_for_member(
                &state.db,
                &state.space_id,
                &did_clone,
            )
            .unwrap_or_default();

            if !unacked.is_empty() {
                eprintln!(
                    "[SpaceDelivery] Peer {} has {} unacked commits, re-notifying",
                    &did_clone[..20.min(did_clone.len())],
                    unacked.len(),
                );
                let senders = state.notification_senders.read().await;
                if let Some(sender) = senders.get(&endpoint_id) {
                    let _ = sender.try_send(Notification::Mls {
                        space_id: state.space_id.clone(),
                        message_type: "commit".to_string(),
                    });
                }
            }

            Response::Ok
        }

        // -- MLS Key Packages --
        Request::MlsUploadKeyPackages {
            space_id,
            packages,
        } => {
            let did = match require_peer_did(state, peer_endpoint_id).await {
                Ok(did) => did,
                Err(resp) => return resp,
            };
            for pkg_b64 in &packages {
                if let Ok(blob) = base64_decode(pkg_b64) {
                    let _ = buffer::store_key_package(&state.db, &space_id, &did, &blob);
                }
            }
            // Trim excess packages — keep only the target amount, discard oldest
            let _ = buffer::trim_key_packages(
                &state.db,
                &space_id,
                &did,
                TARGET_KEY_PACKAGES_PER_PEER,
            );
            Response::Ok
        }

        Request::MlsFetchKeyPackage {
            space_id,
            target_did,
        } => {
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
            let did = match require_peer_did(state, peer_endpoint_id).await {
                Ok(did) => did,
                Err(resp) => return resp,
            };
            match base64_decode(&message) {
                Ok(blob) => {
                    match buffer::store_message(&state.db, &space_id, &did, &message_type, &blob) {
                        Ok(id) => {
                            // Track pending ACKs for commits
                            if message_type == "commit" {
                                let expected_dids: Vec<String> = buffer::get_space_member_dids(&state.db, &space_id)
                                    .unwrap_or_default()
                                    .into_iter()
                                    .filter(|d| d != &did) // exclude sender
                                    .collect();
                                if !expected_dids.is_empty() {
                                    let _ = buffer::store_pending_commit(&state.db, &space_id, id, &expected_dids);
                                }
                            }

                            notify_all_mls(state, &space_id, &message_type).await;
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
            let did = match require_peer_did(state, peer_endpoint_id).await {
                Ok(did) => did,
                Err(resp) => return resp,
            };
            match buffer::fetch_welcomes(&state.db, &space_id, &did) {
                Ok(entries) => {
                    let encoded: Vec<String> = entries.iter().map(|(_, blob)| base64_encode(blob)).collect();
                    for (id, _) in &entries {
                        let _ = buffer::mark_welcome_consumed(&state.db, id);
                    }
                    Response::Welcomes { welcomes: encoded }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        // -- CRDT Sync --
        Request::SyncPush { space_id, changes } => {
            // Capability enforcement: only peers with space/write or space/admin
            // may push CRDT changes. Read-only peers are rejected.
            match require_peer_did(state, peer_endpoint_id).await {
                Ok(did) => {
                    if let Err(e) = check_write_capability(&state.db, &did, &space_id) {
                        eprintln!("[SpaceDelivery] SyncPush REJECTED: {e}");
                        return Response::Error {
                            message: format!("Access denied: {e}"),
                        };
                    }
                }
                Err(_) => {
                    eprintln!("[SpaceDelivery] SyncPush: peer not announced");
                    return Response::Error {
                        message: "Peer has not announced — cannot verify write capability"
                            .to_string(),
                    };
                }
            }

            // 1. Parse changes JSON into Vec<LocalColumnChange>
            let local_changes: Vec<LocalColumnChange> = match serde_json::from_value(changes) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[SpaceDelivery] SyncPush: failed to parse changes: {e}");
                    return Response::Error {
                        message: format!("Invalid changes JSON: {e}"),
                    };
                }
            };

            if local_changes.is_empty() {
                return Response::Ok;
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
                return Response::Error {
                    message: format!("Failed to apply changes: {e}"),
                };
            }

            notify_others_sync(state, &space_id, &affected_tables, peer_endpoint_id).await;

            Response::Ok
        }

        Request::SyncPull {
            space_id: _,
            after_timestamp,
        } => {
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
        Request::MlsAckCommit {
            space_id,
            message_ids,
        } => {
            let did = match require_peer_did(state, peer_endpoint_id).await {
                Ok(did) => did,
                Err(resp) => return resp,
            };

            match buffer::ack_commits(&state.db, &space_id, &did, &message_ids) {
                Ok(fully_acked) => {
                    if !fully_acked.is_empty() {
                        eprintln!(
                            "[SpaceDelivery] Commits fully acked, cleaning up {} messages",
                            fully_acked.len()
                        );
                        let _ = buffer::cleanup_acked_commits(&state.db, &space_id, &fully_acked);
                    }
                    Response::Ok
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::RequestRejoin {
            space_id,
            ucan_token: _,
        } => {
            // Validate the peer is a legitimate member of this space
            let did = match require_peer_did(state, peer_endpoint_id).await {
                Ok(did) => did,
                Err(resp) => return resp,
            };
            if let Err(e) = check_space_membership(&state.db, &did, &space_id) {
                return Response::Error {
                    message: format!("Rejoin denied: {e}"),
                };
            }

            // Export current GroupInfo with ratchet tree for External Commit
            match crate::mls::blocking::get_group_info(
                state.db.0.clone(),
                space_id.clone(),
            )
            .await
            {
                Ok(group_info_bytes) => Response::GroupInfo {
                    group_info: base64_encode(&group_info_bytes),
                },
                Err(e) => Response::Error {
                    message: format!("Failed to export GroupInfo: {e}"),
                },
            }
        }

        Request::SubmitExternalCommit {
            space_id,
            commit,
            ucan_token: _,
        } => {
            // Validate the peer is a legitimate member of this space
            let peer_did = match require_peer_did(state, peer_endpoint_id).await {
                Ok(did) => did,
                Err(resp) => return resp,
            };
            if let Err(e) = check_space_membership(&state.db, &peer_did, &space_id) {
                return Response::Error {
                    message: format!("External commit denied: {e}"),
                };
            }

            let commit_blob = match base64_decode(&commit) {
                Ok(b) => b,
                Err(_) => {
                    return Response::Error {
                        message: "Invalid base64 in commit".to_string(),
                    };
                }
            };

            // Store the external commit as a regular MLS message
            match buffer::store_message(&state.db, &space_id, &peer_did, "commit", &commit_blob) {
                Ok(msg_id) => {
                    // Track pending ACKs from all space members
                    let expected_dids = buffer::get_space_member_dids(&state.db, &space_id)
                        .unwrap_or_default();
                    if !expected_dids.is_empty() {
                        let _ = buffer::store_pending_commit(
                            &state.db,
                            &space_id,
                            msg_id,
                            &expected_dids,
                        );
                    }

                    notify_all_mls(state, &space_id, "commit").await;

                    eprintln!(
                        "[SpaceDelivery] External commit accepted for space {space_id} (msg_id={msg_id})"
                    );
                    Response::Ok
                }
                Err(e) => Response::Error {
                    message: format!("Failed to store external commit: {e}"),
                },
            }
        }

        Request::MlsKeyPackageCount { space_id } => {
            let did = match require_peer_did(state, peer_endpoint_id).await {
                Ok(did) => did,
                Err(resp) => return resp,
            };
            match buffer::count_key_packages_for_did(&state.db, &space_id, &did) {
                Ok(available) => {
                    let needed = TARGET_KEY_PACKAGES_PER_PEER.saturating_sub(available);
                    Response::KeyPackageCount { available, needed }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }
    }
}

/// Encode and send a response on the QUIC send stream, then finish.
pub(super) async fn send_response(
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
