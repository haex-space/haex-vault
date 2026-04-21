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
use crate::crdt::scanner::{
    is_space_scoped_table, scan_space_scoped_tables_for_local_changes, LocalColumnChange,
};
use crate::ucan::{require_capability, validate_token, CapabilityLevel, ValidatedUcan};
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

/// Validate a UCAN token carried in a space-delivery request and return a
/// structured Error response on any failure. This is the first gate for
/// sync-level operations — signature, expiry, structure all checked here.
fn require_valid_ucan(ucan_token: &str, op: &str) -> Result<ValidatedUcan, Response> {
    validate_token(ucan_token).map_err(|e| {
        eprintln!("[SpaceDelivery] {op}: UCAN validation failed: {e}");
        Response::Error {
            message: format!("UCAN validation failed: {e}"),
        }
    })
}

/// Check that a validated UCAN grants the required capability for `space_id`
/// **and** that the UCAN audience is still an active member of the space.
///
/// Membership is the revocation kill-switch: when the admin removes a
/// member (tombstones the `haex_space_members` row + MLS commit) the UCAN
/// remains cryptographically valid but this check rejects every request.
/// A removed member cannot even see metadata like the member list or
/// peer-share titles after the tombstone has propagated.
///
/// Returns an Error response on either failure path.
fn require_ucan_capability(
    validated: &ValidatedUcan,
    space_id: &str,
    required: CapabilityLevel,
    op: &str,
    db: &crate::database::DbConnection,
) -> Result<(), Response> {
    require_capability(validated, space_id, required).map_err(|e| {
        eprintln!("[SpaceDelivery] {op}: capability check failed: {e}");
        Response::Error {
            message: format!("Access denied: {e}"),
        }
    })?;

    match super::ucan::is_active_space_member(db, space_id, &validated.audience) {
        Ok(true) => Ok(()),
        Ok(false) => {
            eprintln!(
                "[SpaceDelivery] {op}: audience {} is not an active member of space {}",
                validated.audience, space_id
            );
            Err(Response::Error {
                message: "Access denied: not an active member of this space".to_string(),
            })
        }
        Err(e) => {
            eprintln!("[SpaceDelivery] {op}: membership check failed: {e}");
            Err(Response::Error {
                message: format!("Membership check failed: {e}"),
            })
        }
    }
}

// `check_space_membership` and `check_write_capability` have been removed.
// They authorised peers by the DID they announced and a lookup against
// `haex_ucan_tokens` — trusting an unauthenticated self-declaration. The
// capability enforcement now happens via `require_valid_ucan` +
// `require_ucan_capability` above, which verify the UCAN signature on
// every request.

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
///
/// Flow is designed to be **idempotent under retry**: the only step that
/// mutates the invite token's `current_uses` is [`invite_tokens::consume_invite`],
/// and it runs at the very end, after MLS add_member and welcome buffering
/// have succeeded. If a previous attempt already completed the MLS add_member
/// but the response was lost in flight, the retry takes the fast path:
/// load the existing UCAN from DB, re-serve the buffered Welcome, and
/// **do not re-consume the token or re-call MLS add_member** (which would
/// fail for an already-added DID).
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

    // 1. Idempotency check — has this DID already been fully claimed once?
    //    If the MLS add_member already ran in a prior attempt (e.g. the
    //    invitee never received the response due to a dropped QUIC stream),
    //    the UCAN is persisted in haex_ucan_tokens and the Welcome is in
    //    the buffer. Re-serve that state without re-consuming the token.
    if let Some((existing_cap, existing_ucan)) =
        load_existing_claim(&state.db, &space_id, &did)
    {
        if let Some(welcome_b64) = fetch_buffered_welcome(&state.db, &space_id, &did) {
            eprintln!(
                "[SpaceDelivery] ClaimInvite: idempotent retry for {} in space {} — re-serving buffered welcome",
                &did[..20.min(did.len())],
                &space_id[..12.min(space_id.len())],
            );
            return Response::InviteClaimed {
                welcome: welcome_b64,
                ucan: existing_ucan,
                capability: existing_cap,
            };
        }
        // UCAN exists but no buffered welcome means the original welcome was
        // already consumed by the invitee in a previous successful attempt.
        // Fall through to error — the invitee cannot re-join the MLS group
        // via the same invite once the welcome is consumed.
        return Response::Error {
            message: "This invite has already been fully claimed".to_string(),
        };
    }

    // 2. Read-only validate — does not consume the token yet.
    let (capability, pre_ucan) =
        match invite_tokens::validate_invite(
            &state.db,
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

    // 3. Determine UCAN: use pre-created (contact) or create now (conference)
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
                super::ucan::MEMBER_UCAN_EXPIRES_IN_SECONDS,
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

    // 4. Store key packages from invitee
    for pkg_b64 in &key_packages {
        if let Ok(blob) = base64_decode(pkg_b64) {
            let _ = buffer::store_key_package(&state.db, &space_id, &did, &blob);
        }
    }

    // 5. Consume one key package for MLS add_member
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

    // 6. MLS add_member
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

    // 7. Store and broadcast commit to existing members
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

    // 8. Extract welcome — required for MLS group entry
    let welcome_blob = match bundle.welcome {
        Some(w) => w,
        None => {
            return Response::Error {
                message: "MLS add_member produced no welcome".to_string(),
            };
        }
    };

    // 9. Buffer welcome for retry idempotency. If the invitee never receives
    //    the response due to a dropped stream, the next ClaimInvite attempt
    //    hits the idempotency fast path above and re-serves this buffered
    //    welcome instead of re-running MLS add_member.
    if let Err(e) = buffer::store_welcome(&state.db, &space_id, &did, &welcome_blob) {
        eprintln!("[SpaceDelivery] Failed to buffer welcome: {e}");
    }

    // 10. Persist UCAN token to admin's local DB (CRDT-synced). Needed so
    //     future invite retries by this DID can recognize the already-claimed
    //     state (see step 1 idempotency check).
    persist_admin_ucan(state, &space_id, &did, &capability, &ucan_token);

    // 11. Register peer as connected
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

    // 12. Persist new member to haex_space_members (CRDT-synced to all devices).
    //     Members reference an identity row by `identity_id`; the DID + public
    //     key live on `haex_identities`. We upsert the identity first (no-op if
    //     UI already imported the contact) and then join by DID to pick up the
    //     actual id — a fresh UUID is only used when the INSERT OR IGNORE
    //     actually created the row.
    //
    // Scope-locked so the HlcService MutexGuard is dropped before the
    // subsequent `.await` on step 13 — otherwise this future would fail the
    // `Send` bound required by `tokio::spawn` further up the call chain.
    let _ = public_key.as_ref();
    {
        let hlc_guard = match state.hlc.lock() {
            Ok(guard) => guard,
            Err(e) => {
                return Response::Error {
                    message: format!("Failed to persist new member: HLC lock error: {e}"),
                };
            }
        };
        let now = OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        let resolved_label = member_label
            .unwrap_or_else(|| did.chars().take(16).collect());

        let ensure_identity_sql = "INSERT OR IGNORE INTO haex_identities \
            (id, did, name, source) VALUES (?1, ?2, ?3, 'contact')"
            .to_string();
        let ensure_identity_params = vec![
            JsonValue::String(uuid::Uuid::new_v4().to_string()),
            JsonValue::String(did.clone()),
            JsonValue::String(resolved_label),
        ];
        if let Err(e) = crate::database::core::execute_with_crdt(
            ensure_identity_sql,
            ensure_identity_params,
            &state.db,
            &hlc_guard,
        ) {
            return Response::Error {
                message: format!("Failed to persist member identity: {e}"),
            };
        }

        let insert_member_sql = "INSERT OR IGNORE INTO haex_space_members \
            (id, space_id, identity_id, role, joined_at) \
            SELECT ?1, ?2, id, ?3, ?4 FROM haex_identities WHERE did = ?5"
            .to_string();
        let member_params = vec![
            JsonValue::String(uuid::Uuid::new_v4().to_string()),
            JsonValue::String(space_id.clone()),
            JsonValue::String(capability.clone()),
            JsonValue::String(now),
            JsonValue::String(did.clone()),
        ];
        if let Err(e) = crate::database::core::execute_with_crdt(
            insert_member_sql,
            member_params,
            &state.db,
            &hlc_guard,
        ) {
            return Response::Error {
                message: format!("Failed to persist space member: {e}"),
            };
        }
    }

    // 13. Consume the token — **only now**, after the claim has fully
    //     succeeded. If anything above failed, the token is still unspent
    //     and the invitee can retry without a manually re-issued invite.
    if let Err(e) = invite_tokens::consume_invite(
        &state.db,
        &state.hlc,
        &state.invite_tokens,
        &token,
    )
    .await
    {
        // Log but don't fail the response — the claim succeeded, only the
        // usage-count persistence failed. At worst the token is usable once
        // more, which is a recoverable soft failure.
        eprintln!("[SpaceDelivery] Failed to consume invite token: {e}");
    }

    // 14. Return welcome + UCAN
    Response::InviteClaimed {
        welcome: base64_encode(&welcome_blob),
        ucan: ucan_token,
        capability,
    }
}

/// Look up an already-granted UCAN for this DID in this space, if any.
/// Returns (capability, ucan_token) so the idempotency path can re-serve
/// exactly the same values a previous successful claim produced.
fn load_existing_claim(
    db: &crate::database::DbConnection,
    space_id: &str,
    claimer_did: &str,
) -> Option<(String, String)> {
    let rows = crate::database::core::select_with_crdt(
        "SELECT capability, token FROM haex_ucan_tokens \
         WHERE space_id = ?1 AND audience_did = ?2 \
         ORDER BY issued_at DESC LIMIT 1"
            .to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(claimer_did.to_string()),
        ],
        db,
    )
    .ok()?;

    let row = rows.first()?;
    let capability = row.first()?.as_str()?.to_string();
    let ucan = row.get(1)?.as_str()?.to_string();
    Some((capability, ucan))
}

/// Fetch an unconsumed buffered welcome for this recipient in this space,
/// base64-encoded for transport. Returns `None` if none is buffered.
fn fetch_buffered_welcome(
    db: &crate::database::DbConnection,
    space_id: &str,
    recipient_did: &str,
) -> Option<String> {
    let entries = buffer::fetch_welcomes(db, space_id, recipient_did).ok()?;
    let (_id, blob) = entries.into_iter().next()?;
    Some(base64_encode(&blob))
}

/// Persist the granted UCAN on the admin's side so subsequent idempotent
/// retries can recognize an already-claimed invite. Errors are logged and
/// swallowed: the UCAN was successfully delivered to the invitee regardless,
/// and losing this row only means the next retry will not take the fast path.
///
/// Skips insertion if a row for this `(space_id, audience_did)` already
/// exists — avoids duplicate entries when CRDT sync later propagates the
/// claimant-side self-issued UCAN row back to the admin.
fn persist_admin_ucan(
    state: &LeaderState,
    space_id: &str,
    audience_did: &str,
    capability: &str,
    ucan_token: &str,
) {
    if load_existing_claim(&state.db, space_id, audience_did).is_some() {
        return;
    }

    let admin = match super::ucan::load_admin_identity(&state.db, space_id) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[SpaceDelivery] persist_admin_ucan: load admin failed: {e}");
            return;
        }
    };

    let hlc_guard = match state.hlc.lock() {
        Ok(g) => g,
        Err(_) => {
            eprintln!("[SpaceDelivery] persist_admin_ucan: HLC lock poisoned");
            return;
        }
    };

    let ucan_id = uuid::Uuid::new_v4().to_string();
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let sql = "INSERT OR IGNORE INTO haex_ucan_tokens \
        (id, space_id, issuer_did, audience_did, capability, token, issued_at, expires_at) \
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
        .to_string();
    let params = vec![
        JsonValue::String(ucan_id),
        JsonValue::String(space_id.to_string()),
        JsonValue::String(admin.did),
        JsonValue::String(audience_did.to_string()),
        JsonValue::String(capability.to_string()),
        JsonValue::String(ucan_token.to_string()),
        JsonValue::Number(serde_json::Number::from(now_secs)),
        JsonValue::Number(serde_json::Number::from(
            now_secs + super::ucan::MEMBER_UCAN_EXPIRES_IN_SECONDS as i64,
        )),
    ];
    if let Err(e) =
        crate::database::core::execute_with_crdt(sql, params, &state.db, &hlc_guard)
    {
        eprintln!("[SpaceDelivery] persist_admin_ucan: insert failed: {e}");
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
            space_id,
            label,
            claims,
            ucan_token,
        } => {
            // Announce is the first authenticated boundary of a peer session.
            // Anyone can open a QUIC stream with the ALPN and claim a DID, so
            // we must verify the UCAN before trusting `did` and before
            // populating `connected_peers` (which later handlers rely on).
            let validated = match require_valid_ucan(&ucan_token, "Announce") {
                Ok(v) => v,
                Err(r) => return r,
            };
            if validated.audience != did {
                eprintln!(
                    "[SpaceDelivery] Announce REJECTED: UCAN audience {} does not match announced DID {}",
                    validated.audience, did
                );
                return Response::Error {
                    message: "UCAN audience does not match announced DID".to_string(),
                };
            }
            if let Err(r) = require_ucan_capability(
                &validated,
                &space_id,
                CapabilityLevel::Read,
                "Announce",
                &state.db,
            ) {
                return r;
            }

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
        Request::SyncPush {
            space_id,
            changes,
            ucan_token,
        } => {
            // Authenticate and authorise before touching any DB state. Read-
            // only peers must not be able to push — check Write capability.
            let validated = match require_valid_ucan(&ucan_token, "SyncPush") {
                Ok(v) => v,
                Err(r) => return r,
            };
            if let Err(r) = require_ucan_capability(
                &validated,
                &space_id,
                CapabilityLevel::Write,
                "SyncPush",
                &state.db,
            ) {
                return r;
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

            // Defense-in-depth: reject any change outside the whitelist of
            // space-scoped CRDT tables. Even a write-capable peer cannot
            // inject rows into `haex_identities`, `haex_sync_backends`, etc.
            for change in &local_changes {
                if !is_space_scoped_table(&change.table_name) {
                    eprintln!(
                        "[SpaceDelivery] SyncPush REJECTED: table {} is not space-scoped",
                        change.table_name
                    );
                    return Response::Error {
                        message: format!(
                            "Table {} is not allowed in space-scoped sync",
                            change.table_name
                        ),
                    };
                }
            }

            // 2. Convert to RemoteColumnChange (HLC is the grouping key)
            let remote_changes: Vec<RemoteColumnChange> = local_changes
                .iter()
                .map(super::sync_loop::local_to_remote_change)
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
            space_id,
            after_timestamp,
            ucan_token,
        } => {
            // Read capability is the minimum bar — any member (read / write /
            // invite / admin) may pull. Non-members get no data at all.
            let validated = match require_valid_ucan(&ucan_token, "SyncPull") {
                Ok(v) => v,
                Err(r) => return r,
            };
            if let Err(r) = require_ucan_capability(
                &validated,
                &space_id,
                CapabilityLevel::Read,
                "SyncPull",
                &state.db,
            ) {
                return r;
            }

            let device_id = "leader";
            match scan_space_scoped_tables_for_local_changes(
                &state.db,
                &space_id,
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
            inviter_avatar,
            inviter_avatar_options,
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
            inviter_avatar.as_deref(),
            inviter_avatar_options.as_deref(),
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
            ucan_token,
        } => {
            let validated = match require_valid_ucan(&ucan_token, "RequestRejoin") {
                Ok(v) => v,
                Err(r) => return r,
            };
            if let Err(r) = require_ucan_capability(
                &validated,
                &space_id,
                CapabilityLevel::Read,
                "RequestRejoin",
                &state.db,
            ) {
                return r;
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
            ucan_token,
        } => {
            let validated = match require_valid_ucan(&ucan_token, "SubmitExternalCommit") {
                Ok(v) => v,
                Err(r) => return r,
            };
            if let Err(r) = require_ucan_capability(
                &validated,
                &space_id,
                CapabilityLevel::Read,
                "SubmitExternalCommit",
                &state.db,
            ) {
                return r;
            }
            let peer_did = validated.audience.clone();

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
