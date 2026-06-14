//! Leader-side delivery: connection handler, request dispatch, state management.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use time::OffsetDateTime;
use tokio::sync::RwLock;

use tauri::{AppHandle, Emitter, Manager};

use crate::crdt::commands::{apply_remote_changes_to_db, RemoteColumnChange};
use crate::crdt::hlc::HlcService;
use crate::crdt::scanner::{scan_space_scoped_tables_for_local_changes, LocalColumnChange};
use crate::critical::CriticalFailureCode;
use crate::ucan::{require_audience, require_capability, validate_token, CapabilityLevel, ValidatedUcan};
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

/// Check that a validated UCAN grants the required capability for `space_id`,
/// that the UCAN's `aud` matches the **announced peer DID** (replay-protection),
/// and that the audience is still an active member of the space.
///
/// Three concentric gates:
///
/// 1. **Audience match (`require_audience`)** — the UCAN must have been issued
///    *to the peer presenting it*. Without this check, a peer P who obtained
///    another member's UCAN (e.g. by snooping or replay) could present it
///    over its own authenticated QUIC channel; the capability+membership
///    checks below would both pass.
/// 2. **Capability (`require_capability`)** — the UCAN grants at least the
///    operation's minimum capability for `space_id`.
/// 3. **Active membership (`is_active_space_member`)** — revocation
///    kill-switch: when the admin tombstones a member, the UCAN remains
///    cryptographically valid but every request is rejected here.
///
/// Returns an Error response on any failure.
///
/// **Post-T6 usage.** After the unified `auth_gate` was wired in (T5) and
/// the per-arm redundant checks were removed (T6), this helper has exactly
/// one caller: the `Announce` arm. Announce is the gate's bypass —
/// `auth_gate` returns `Ok(None)` for it because Announce is what
/// *populates* the cached `ValidatedUcan` the gate reads on subsequent
/// requests. So the same three concentric checks still need to run, just
/// **here**, before the UCAN is cached. Every other request variant gets
/// these checks from the gate.
fn require_ucan_capability(
    validated: &ValidatedUcan,
    space_id: &str,
    required: CapabilityLevel,
    peer_did: &str,
    op: &str,
    db: &crate::database::DbConnection,
) -> Result<(), Response> {
    require_audience(validated, peer_did).map_err(|e| {
        eprintln!("[SpaceDelivery] {op}: audience mismatch: {e}");
        Response::Error {
            message: format!("Access denied: {e}"),
        }
    })?;

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
// `haex_ucan_tokens` — trusting an unauthenticated self-declaration.
// Capability enforcement now happens at the unified `auth_gate` for every
// non-bypass request (see `super::auth_gate::authorize_request`); the
// `require_valid_ucan` + `require_ucan_capability` helpers above are kept
// only for the `Announce` bypass, which must validate and cache the UCAN
// the gate later reads.

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
pub async fn handle_claim_invite(
    state: &LeaderState,
    request: Request,
    verified_did: &str,
) -> Response {
    let (space_id, token, endpoint_id, key_packages, label, public_key) = match request {
        Request::ClaimInvite {
            space_id,
            token,
            endpoint_id,
            key_packages,
            label,
            public_key,
        } => (space_id, token, endpoint_id, key_packages, label, public_key),
        _ => {
            return Response::Error {
                message: "Expected ClaimInvite request".to_string(),
            }
        }
    };

    // The connection-bound DID from the quic_did_auth handshake is the only
    // identity we trust for this claim. Carrying a `did` in the payload was
    // a trust hazard (plan §4.2 scenarios 1 + 2) — the field is dropped
    // from the wire format in this commit; all downstream code uses the
    // cryptographically authenticated `verified_did` instead.
    let did: String = verified_did.to_string();

    debug_assert_eq!(space_id, state.space_id, "ClaimInvite routed to wrong leader");

    // 1. Detect retry: a prior attempt may have already added the member to
    //    the MLS group and consumed the invite token. We do NOT short-circuit
    //    by re-serving the buffered Welcome — OpenMLS deletes the matched
    //    KeyPackage from the invitee's storage on welcome lookup (single-use
    //    semantics, see openmls creation.rs::keys_for_welcome). If the prior
    //    welcome processing failed downstream, that KP is gone and re-serving
    //    the same welcome loops forever on `NoMatchingKeyPackage`.
    //
    //    Instead we always regenerate the welcome from a *fresh* KP. The
    //    duplicate-leaf handling in `MlsManager::add_member` quietly removes
    //    the stale leaf from the prior attempt before re-adding, so the
    //    group ends up consistent at the cost of two extra epoch advances.
    let existing = load_existing_claim(&state.db, &space_id, &did);
    let is_retry = existing.is_some();
    if is_retry {
        eprintln!(
            "[SpaceDelivery] ClaimInvite: retry for {} in space {} — regenerating welcome with fresh KeyPackage",
            &did[..20.min(did.len())],
            &space_id[..12.min(space_id.len())],
        );
    }

    // 2. Resolve capability + UCAN.
    //    - Retry: reuse the previously-issued UCAN (token already consumed
    //      in the first attempt, no re-validation needed).
    //    - First attempt: read-only validate the token; consume happens at
    //      step 13 only after the rest of the flow succeeds.
    let (capability, ucan_token) = if let Some((existing_cap, existing_ucan)) = existing {
        (existing_cap, existing_ucan)
    } else {
        let (capability, pre_ucan) = match invite_tokens::validate_invite(
            &state.db,
            &state.invite_tokens,
            &token,
            verified_did,
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
        (capability, ucan_token)
    };

    // 4. Replace stale KeyPackages from prior attempts with the fresh batch.
    //    Without the clear, `consume_key_package` (FIFO) could pick a stale
    //    KP whose hash the invitee no longer has in their MLS storage — the
    //    same `NoMatchingKeyPackage` failure mode but at first-attempt time.
    let _ = buffer::clear_key_packages_for_did(&state.db, &space_id, &did);
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

    // 9. Buffer the freshly-generated welcome. Drop any stale buffered welcome
    //    from a prior attempt first — keeping it around would make a future
    //    `MlsFetchWelcomes` poll return an obsolete welcome whose KeyPackage
    //    hash the invitee no longer has, surfacing as `NoMatchingKeyPackage`.
    let _ = buffer::clear_welcomes_for_did(&state.db, &space_id, &did);
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
            // ClaimInvite issues the UCAN; the peer presents it on a
            // subsequent Announce, which populates this cache.
            validated_ucan: None,
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
        let app_state = state.app_handle.state::<crate::AppState>();
        let hlc_guard = match app_state.lock_or_fail(
            &state.hlc,
            CriticalFailureCode::HlcMutexPoisoned,
            "space_delivery::local::leader::handle_claim_invite::persist_new_member",
            serde_json::json!({}),
        ) {
            Ok(guard) => guard,
            Err(e) => {
                return Response::Error {
                    message: format!("Failed to persist new member: {e}"),
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
    //
    //     Skip on retry: the token was already consumed by the first
    //     attempt and incrementing again would (a) overshoot `max_uses` for
    //     single-use contact invites and (b) double-count for multi-use
    //     conference invites.
    if !is_retry {
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

/// Persist the granted UCAN on the admin's side so subsequent claim retries
/// for the same DID can be detected and routed through the regenerate path.
/// Errors are logged and swallowed: the UCAN was successfully delivered to
/// the invitee regardless, and losing this row only means the next retry
/// will be treated as a first attempt (still safe — the duplicate-leaf
/// handling in `add_member` covers it).
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

    let app_state = state.app_handle.state::<crate::AppState>();
    let hlc_guard = match app_state.lock_or_fail(
        &state.hlc,
        CriticalFailureCode::HlcMutexPoisoned,
        "space_delivery::local::leader::persist_admin_ucan",
        serde_json::json!({}),
    ) {
        Ok(g) => g,
        Err(_) => return,
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
    verified_did: &str,
) -> Response {
    // Unified auth choke point. Bypass requests (Announce, ClaimInvite,
    // PushInvite) return `Ok(None)` and proceed unchanged; every other
    // variant must come from a peer that already Announced on this
    // connection, carry a UCAN whose audience matches the
    // connection-authenticated DID, grant at least the per-request minimum
    // capability, and still resolve to an active member.
    //
    // For non-bypass arms the gate's `ValidatedUcan` is the single source of
    // UCAN truth — those arms read it via
    // `gate_ucan.as_ref().expect("non-bypass <arm> must have ValidatedUcan from gate")`
    // and **must not** re-validate the request's `ucan_token` field. The
    // wire-format `ucan_token` is now redundant for non-bypass requests;
    // removing it is left to a follow-up so this PR avoids a protocol break.
    //
    // Bypass arms (Announce, ClaimInvite, PushInvite) see `gate_ucan = None`
    // and still run their own UCAN handling — Announce in particular must
    // validate + cache the UCAN it just received before subsequent requests
    // on this connection can pass the gate.
    //
    // Audit logging: the gate writes a `warn` row to `haex_logs` (via
    // `log_to_db`, CRDT-synced to the owner) from every reject branch with
    // `source = Request::op_name`, restoring the in-app log visibility the
    // pre-T6 SyncPush / SyncPull arms used to emit directly.
    let gate_ucan = match super::auth_gate::authorize_request(
        &request,
        verified_did,
        peer_endpoint_id,
        &state.connected_peers,
        &state.db,
        &state.hlc,
    )
    .await
    {
        Ok(maybe) => maybe,
        Err(response) => return response,
    };

    match request {
        Request::Announce {
            endpoint_id,
            space_id,
            label,
            claims,
            ucan_token,
        } => {
            // The connection-bound DID from the quic_did_auth handshake is
            // the only identity we trust for this announce. The payload `did`
            // field has been removed from the wire in C10 — see plan §1.3 +
            // §4.2 for the spoofing vector that carrying it would re-enable.
            let did: String = verified_did.to_string();
            // Announce is the first authenticated boundary of a peer session.
            // Anyone can open a QUIC stream with the ALPN and claim a DID, so
            // we must verify the UCAN before trusting `did` and before
            // populating `connected_peers` (which later handlers rely on).
            crate::logging::log_to_db(
                &state.db, &state.hlc, "info", "Announce",
                &format!("received: space={} did={} peer={}",
                    &space_id[..8.min(space_id.len())],
                    &did[..24.min(did.len())],
                    peer_endpoint_id,
                ),
                None,
            );
            // Announce bootstraps the AuthGate cache, so its `ucan_token`
            // must be present even though the wire field is now
            // `Option<String>` (forward-compat shape for the other request
            // variants; see protocol.rs for the rationale).
            let ucan_token_str = match ucan_token.as_deref() {
                Some(t) => t,
                None => {
                    crate::logging::log_to_db(
                        &state.db, &state.hlc, "warn", "Announce",
                        &format!("missing ucan_token: space={} did={}",
                            &space_id[..8.min(space_id.len())], &did[..24.min(did.len())]),
                        None,
                    );
                    return Response::Error {
                        message: "Announce requires ucan_token".to_string(),
                    };
                }
            };
            let validated = match require_valid_ucan(ucan_token_str, "Announce") {
                Ok(v) => v,
                Err(r) => {
                    crate::logging::log_to_db(
                        &state.db, &state.hlc, "warn", "Announce",
                        &format!("UCAN validation failed: space={} did={}",
                            &space_id[..8.min(space_id.len())], &did[..24.min(did.len())]),
                        None,
                    );
                    return r;
                }
            };
            // Audience-vs-announced-DID is now enforced inside
            // require_ucan_capability via require_audience; no separate
            // pre-check needed.
            if let Err(r) = require_ucan_capability(
                &validated,
                &space_id,
                CapabilityLevel::Read,
                &did,
                "Announce",
                &state.db,
            ) {
                crate::logging::log_to_db(
                    &state.db, &state.hlc, "warn", "Announce",
                    &format!("capability/membership rejected: space={} audience={}",
                        &space_id[..8.min(space_id.len())],
                        &validated.audience[..24.min(validated.audience.len())]),
                    None,
                );
                return r;
            }
            crate::logging::log_to_db(
                &state.db, &state.hlc, "info", "Announce",
                &format!("accepted: space={} audience={}",
                    &space_id[..8.min(space_id.len())],
                    &validated.audience[..24.min(validated.audience.len())]),
                None,
            );

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
                validated_ucan: Some(validated.clone()),
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
            let did = verified_did.to_string();
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
            let did = verified_did.to_string();
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
            let did = verified_did.to_string();
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
            // `ucan_token` is now dead on the wire for SyncPush — the gate
            // authenticated this request against the cached UCAN from
            // Announce. Keeping the destructure-ignore avoids a protocol
            // break; a follow-up removes the field from `Request::SyncPush`.
            ..
        } => {
            // The gate proved this peer Announced, holds a valid UCAN whose
            // audience matches `verified_did`, has at least SyncPush's
            // capability for this space, and is still an active member.
            let validated = gate_ucan
                .as_ref()
                .expect("non-bypass SyncPush must have ValidatedUcan from gate");

            // Parse changes JSON into Vec<LocalColumnChange>
            let local_changes: Vec<LocalColumnChange> = match serde_json::from_value(changes) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[SpaceDelivery] SyncPush: failed to parse changes: {e}");
                    return Response::Error {
                        message: format!("Invalid changes JSON: {e}"),
                    };
                }
            };

            // Single authorisation entry point — handles capability,
            // membership, payload validation, origin attribution and
            // per-row ownership in one place. See
            // `super::inbound_sync::authorize_inbound_sync_push` for the
            // full pipeline.
            let local_changes = match super::inbound_sync::authorize_inbound_sync_push(
                &state.db,
                &space_id,
                peer_endpoint_id,
                validated,
                local_changes,
            ) {
                super::inbound_sync::InboundSyncPushOutcome::Accepted { changes } => changes,
                super::inbound_sync::InboundSyncPushOutcome::Rejected { reason } => {
                    eprintln!("[SpaceDelivery] SyncPush REJECTED: {reason}");
                    return Response::Error { message: reason };
                }
            };

            // Post-validation no-op: payload was empty (or contained only
            // client-supplied authored_by_did claims that the validator
            // strips). Nothing to apply, nothing to notify.
            if local_changes.is_empty() {
                return Response::Ok;
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

            // 3. Apply changes to DB (HLC clock is advanced internally).
            //    Previous code locked HLC with `.lock().ok().map(...)` and
            //    passed `None` on poison — that would apply remote changes
            //    WITHOUT advancing the local clock, producing stale local
            //    timestamps that lose merge conflicts on the next sync.
            //    `lock_or_fail` propagates a banner-visible failure instead.
            let app_state = state.app_handle.state::<crate::AppState>();
            // Clone the HlcService out under the lock so the guard is
            // dropped before the `.await` below — MutexGuard is `!Send`
            // and would otherwise break the `tokio::spawn` Send bound.
            let hlc_service = match app_state.lock_or_fail(
                &state.hlc,
                CriticalFailureCode::HlcMutexPoisoned,
                "space_delivery::local::leader::handle_delivery_request::sync_push_apply",
                serde_json::json!({}),
            ) {
                Ok(g) => g.clone(),
                Err(e) => {
                    return Response::Error {
                        message: format!("Failed to lock HLC for SyncPush apply: {e}"),
                    };
                }
            };
            if let Err(e) = apply_remote_changes_to_db(
                &state.db,
                remote_changes,
                None,
                Some(&hlc_service),
            ) {
                eprintln!("[SpaceDelivery] SyncPush: failed to apply changes: {e}");
                return Response::Error {
                    message: format!("Failed to apply changes: {e}"),
                };
            }

            notify_others_sync(state, &space_id, &affected_tables, peer_endpoint_id).await;

            // If the push touched haex_space_devices, reload allowed_peers now —
            // synchronously, before returning Ok. This ensures the new peer is
            // authorized before it can issue any peer-storage requests. The async
            // TS event chain (local-sync-completed → peer_storage_reload_shares)
            // runs in parallel but this Rust-side reload is the authoritative gate.
            if affected_tables.iter().any(|t| t == "haex_space_devices") {
                let app_state: tauri::State<'_, crate::AppState> = state.app_handle.state();
                let endpoint = app_state.peer_storage.read().await;
                if let Err(e) = crate::peer_storage::commands::reload_allowed_peers(
                    &app_state,
                    &endpoint,
                ).await {
                    eprintln!("[SpaceDelivery] Failed to reload allowed_peers after space_devices push: {e}");
                    return Response::Error {
                        message: format!("Failed to reload allowed_peers: {e}"),
                    };
                }
            }

            // Notify the leader's own frontend so UI stores (file browser peer
            // list, space devices) reload without waiting for the next cloud pull.
            // emit_to(label, …) keeps the event out of extension webviews.
            let _ = state.app_handle.emit_to(
                "main",
                "local-sync-completed",
                serde_json::json!({
                    "spaceId": &space_id,
                    "tables": &affected_tables,
                }),
            );

            Response::Ok
        }

        Request::SyncPull {
            space_id,
            after_timestamp,
            // `ucan_token` is now redundant on the wire — the gate
            // authenticated this request against the cached UCAN. Kept
            // as `..` to avoid a protocol break this PR.
            ..
        } => {
            // The gate proved Read+ capability and active membership.
            // `validated` is held only for the success-path audit log below
            // (`audience=…`); no further auth decision is made here.
            let validated = gate_ucan
                .as_ref()
                .expect("non-bypass SyncPull must have ValidatedUcan from gate");

            let device_id = "leader";
            // Origin filter is push-only (sync_loop). When *serving* a pull
            // the leader is the source of truth and must hand out every row
            // it has for this space, regardless of who originally wrote it.
            match scan_space_scoped_tables_for_local_changes(
                &state.db,
                &space_id,
                after_timestamp.as_deref(),
                device_id,
                None,
            ) {
                Ok(changes) => {
                    let by_table: std::collections::BTreeMap<&str, usize> =
                        changes.iter().fold(std::collections::BTreeMap::new(), |mut acc, c| {
                            *acc.entry(c.table_name.as_str()).or_insert(0) += 1;
                            acc
                        });
                    crate::logging::log_to_db(
                        &state.db, &state.hlc, "info", "SyncPull",
                        &format!("served: space={} audience={} count={} after={:?} tables={:?}",
                            &space_id[..8.min(space_id.len())],
                            &validated.audience[..24.min(validated.audience.len())],
                            changes.len(),
                            after_timestamp.as_deref(),
                            by_table,
                        ),
                        None,
                    );
                    match serde_json::to_value(&changes) {
                        Ok(json) => Response::SyncChanges { changes: json },
                        Err(e) => {
                            eprintln!("[SpaceDelivery] SyncPull: failed to serialize changes: {e}");
                            Response::Error {
                                message: format!("Failed to serialize changes: {e}"),
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[SpaceDelivery] SyncPull: failed to scan changes: {e}");
                    crate::logging::log_to_db(
                        &state.db, &state.hlc, "error", "SyncPull",
                        &format!("scan failed: space={} err={}",
                            &space_id[..8.min(space_id.len())], e),
                        None,
                    );
                    Response::Error {
                        message: format!("Failed to scan changes: {e}"),
                    }
                }
            }
        }

        // -- Invites (ClaimInvite) --
        req @ Request::ClaimInvite { .. } => {
            handle_claim_invite(state, req, verified_did).await
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
            inviter_relay_url,
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
            inviter_relay_url.as_deref(),
            verified_did,
        ),
        Request::MlsAckCommit {
            space_id,
            message_ids,
        } => {
            let did = verified_did.to_string();

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
            // `ucan_token` is now redundant on the wire — the gate
            // authenticated this request against the cached UCAN.
            ..
        } => {
            // Gate-wire-up regression guard: this arm has no downstream
            // consumer of `validated_ucan`, but we still assert the gate
            // produced one so a future refactor that loses the dispatcher's
            // gate call panics loudly here instead of silently leaking
            // GroupInfo to unauthenticated peers.
            let _ = gate_ucan
                .as_ref()
                .expect("non-bypass RequestRejoin must have ValidatedUcan from gate");

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
            // `ucan_token` is now redundant on the wire — the gate
            // authenticated this request against the cached UCAN.
            ..
        } => {
            // Gate-wire-up regression guard: this arm has no downstream
            // consumer of `validated_ucan`, but we still assert the gate
            // produced one so a future refactor that loses the dispatcher's
            // gate call panics loudly here instead of silently storing an
            // MLS commit attributed to an unauthenticated DID.
            let _ = gate_ucan
                .as_ref()
                .expect("non-bypass SubmitExternalCommit must have ValidatedUcan from gate");
            // `peer_did` is sourced from the connection-bound verified_did,
            // not from the UCAN audience. The gate guarantees they're equal.
            let peer_did = verified_did.to_string();

            let commit_blob = match base64_decode(&commit) {
                Ok(b) => b,
                Err(_) => {
                    return Response::Error {
                        message: "Invalid base64 in commit".to_string(),
                    };
                }
            };

            // Apply the External Commit to the leader's own MLS group so the
            // leader advances to the new epoch. Without this the leader stays
            // at the old epoch permanently and every subsequent RequestRejoin
            // hands out a GroupInfo for the stale epoch, causing the peer to
            // loop: rejoin → new epoch-N message stored → can't process → rejoin…
            if let Err(e) = crate::mls::blocking::process_message(
                state.db.0.clone(),
                space_id.clone(),
                commit_blob.clone(),
            )
            .await
            {
                eprintln!(
                    "[SpaceDelivery] External commit: leader MLS process failed for space {space_id}: {e}"
                );
                // Non-fatal: still store and distribute; the leader's local MLS
                // state may already be ahead (duplicate submit) or the commit may
                // be for a newer epoch the leader hasn't reached yet.
            }

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
                    // Return the stored message ID so the peer can advance its
                    // MLS cursor past the External Commit itself, preventing the
                    // next cycle from fetching and failing to process it.
                    Response::MessageStored { message_id: msg_id }
                }
                Err(e) => Response::Error {
                    message: format!("Failed to store external commit: {e}"),
                },
            }
        }

        Request::MlsKeyPackageCount { space_id } => {
            let did = verified_did.to_string();
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

#[cfg(test)]
mod audience_check_tests {
    //! Regression guards for UCAN audience verification.
    //!
    //! Post-T6 reality: the unified `auth_gate::authorize_request` is the
    //! central gate for every authenticated, non-bypass space-delivery
    //! request. Its audience binding (`require_audience` against the
    //! connection-bound DID) is covered by `auth_gate_tests`.
    //!
    //! What this module still pins is the **Announce bypass** path. Announce
    //! cannot rely on the gate (the gate returns `Ok(None)` for it, because
    //! Announce is what populates the cached UCAN the gate later reads), so
    //! `require_ucan_capability` runs inline there. Without the `aud ==
    //! announced peer DID` check inside the helper, a peer P could replay
    //! another member's UCAN through its own QUIC channel and have it
    //! cached — the gate would then trust the cached `validated_ucan` on
    //! subsequent requests and the replay would pass. So the helper's
    //! invariants matter exactly as much as before, just for one caller.
    //!
    //! These tests are static-source assertions because the dispatcher
    //! requires `&mut LeaderState`, a tokio runtime, an `iroh::Endpoint`, a
    //! populated `connected_peers` map, and a SQLite schema with HLC
    //! triggers; building all of that costs more than the linting checks
    //! buy us. Behavioural coverage is deferred to e2e in haex-e2e-tests.
    //!
    //! Unit coverage of the helper itself (`require_audience` accepts /
    //! rejects) lives in `ucan::verify::tests`.

    /// `require_ucan_capability` must take a `peer_did` parameter and call
    /// `require_audience` inside. Removing either would silently restore
    /// the replay window the audience check is meant to close.
    #[test]
    fn require_ucan_capability_takes_peer_did_and_calls_require_audience() {
        let source = include_str!("leader.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(p, _)| p)
            .unwrap_or(source);

        assert!(
            production.contains("peer_did: &str,"),
            "require_ucan_capability must declare a peer_did parameter"
        );
        assert!(
            production.contains("require_audience(validated, peer_did)"),
            "require_ucan_capability must invoke require_audience with the \
             announced peer DID — without this, a UCAN issued to any other \
             still-active member is accepted as a replay"
        );
    }

    /// Every UCAN-gated request handler must source `peer_did` from the
    /// connection-bound `verified_did` of the quic_did_auth handshake (Phase
    /// 2, see plan §4.1). Prior to Phase 2 the DID was looked up from the
    /// `connected_peers` map populated by Announce — that worked only when
    /// every handler ran after Announce and effectively meant "trust whatever
    /// Announce claimed", which is itself unsafe before the handshake binds
    /// the DID. After C7 every handler binds directly via
    /// `verified_did.to_string()`.
    ///
    /// **T6 update.** The pre-T6 invariant — "every UCAN-gated arm calls
    /// `require_ucan_capability(…, peer_did, …)`" — is gone. Sync arms now
    /// trust the unified `auth_gate` (which performs the same audience +
    /// capability + active-membership checks once per request). Only the
    /// Announce arm still calls the helper inline, because Announce is the
    /// bypass that *populates* the cached UCAN the gate later reads from.
    /// We keep the `verified_did.to_string()` guard below to pin that no
    /// regression brings back `require_peer_did(state, peer_endpoint_id)`.
    #[test]
    fn every_require_ucan_capability_call_passes_a_peer_did() {
        let source = include_str!("leader.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(p, _)| p)
            .unwrap_or(source);

        // Every handler that needs a DID for capability/buffer keys/audit
        // logs now derives it from the connection-bound verified_did. The
        // legacy `require_peer_did(state, peer_endpoint_id)` lookup against
        // `connected_peers` is gone — keeping it would defeat the Phase 2
        // promise that handlers no longer depend on Announce having run
        // first.
        let legacy_lookups = production
            .matches("require_peer_did(state, peer_endpoint_id)")
            .count();
        assert_eq!(
            legacy_lookups, 0,
            "no production handler should look up the peer DID via \
             require_peer_did any more — it must come from verified_did. \
             Found {legacy_lookups} legacy call sites."
        );

        let verified_bindings = production
            .matches("verified_did.to_string()")
            .count();
        assert!(
            verified_bindings >= 4,
            "expected at least 4 `verified_did.to_string()` bindings inside \
             request handlers (covering MLS request envelope DIDs + the \
             three UCAN-audience-checked handlers); found {verified_bindings}"
        );
    }
}

#[cfg(test)]
mod auth_gate_wireup_tests {
    //! Regression guards for the T5 wire-up: `handle_delivery_request` must
    //! invoke `auth_gate::authorize_request` **before** the `match request`
    //! dispatch, so every non-bypass request is authorised at one choke point.
    //!
    //! ## Deviation from the plan (Phase 4 Task 4.2)
    //!
    //! The plan prescribed a behavioural integration test
    //! (`unannounced_mls_upload_is_rejected_at_dispatcher`) built against a
    //! `build_test_leader_state("SPACE")` helper. We deviated and shipped the
    //! three source-text assertions below instead. Rationale:
    //!
    //! - **Fixture cost is real.** `LeaderState` carries an `AppHandle`, an
    //!   iroh `Endpoint`, an MLS provider, an HLC, a tokio runtime, plus a
    //!   SQLite schema with HLC triggers. The existing
    //!   `audience_check_tests` and `claim_invite_did_binding_tests` modules
    //!   hit the same wall and resolved it the same way — source-text only.
    //!   We follow that precedent.
    //! - **Behavioural coverage already exists at the gate level.**
    //!   `auth_gate_tests::rejects_request_without_prior_announce` (and
    //!   sibling rejection-path tests) drive the gate against an in-memory
    //!   DB. Those tests prove the gate works. The source-text assertions
    //!   here prove the *wire-up*: the dispatcher actually calls the gate,
    //!   and on `Err` it returns the response before reaching the match.
    //! - **End-to-end coverage lives in `haex-e2e-tests`.** Real-network
    //!   negative paths (un-announced peer, revoked member, etc.) run there.
    //!
    //! Net: gate behaviour is exercised against an in-memory DB;
    //! dispatcher-to-gate wiring is pinned via static-source assertions;
    //! the full path is covered e2e. The plan's `build_test_leader_state`
    //! helper was not worth its weight given that triangulation.
    //!
    //! T6 has landed: the gate outcome is now `gate_ucan` (no prefix
    //! underscore) and every non-bypass arm reads its `ValidatedUcan` from
    //! the gate via `gate_ucan.as_ref().expect(...)`. SyncPush passes the
    //! gate UCAN into `authorize_inbound_sync_push` for downstream origin
    //! attribution; SyncPull keeps it for the success-path audit log;
    //! RequestRejoin and SubmitExternalCommit bind it to `_gate_ucan`
    //! solely so a future wire-up regression would panic loudly.

    /// `handle_delivery_request` must call `auth_gate::authorize_request`
    /// before the `match request` dispatch. Without this single choke point
    /// the per-arm checks remain the only line of defence and the MLS-related
    /// arms (which had no inline UCAN check pre-T5) stay un-gated.
    #[test]
    fn handle_delivery_request_invokes_gate_before_match() {
        let source = include_str!("leader.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(p, _)| p)
            .unwrap_or(source);

        let fn_marker = "pub(super) async fn handle_delivery_request(";
        let fn_start = production
            .find(fn_marker)
            .expect("handle_delivery_request must exist");
        let body = &production[fn_start..];
        let gate_call_pos = body
            .find("auth_gate::authorize_request(")
            .expect(
                "handle_delivery_request must call auth_gate::authorize_request — \
                 without this every per-arm check stays the only line of defence \
                 and the MLS arms remain un-gated. See plan T5 §4.1.",
            );
        let match_pos = body
            .find("match request {")
            .expect("handle_delivery_request must contain `match request {`");

        assert!(
            gate_call_pos < match_pos,
            "auth_gate::authorize_request must be invoked BEFORE the `match \
             request` dispatch — gating after the match defeats the choke \
             point. See plan T5 §4.1."
        );
    }

    /// The gate-rejection arm in `handle_delivery_request` must `return` the
    /// `Response::Error` it receives, never fall through to the match. The
    /// `?` operator is impossible here because the fn returns `Response`,
    /// not `Result`, so the explicit `return response` pattern is the only
    /// safe shape.
    #[test]
    fn handle_delivery_request_returns_gate_rejection() {
        let source = include_str!("leader.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(p, _)| p)
            .unwrap_or(source);

        let fn_marker = "pub(super) async fn handle_delivery_request(";
        let fn_start = production
            .find(fn_marker)
            .expect("handle_delivery_request must exist");
        let body = &production[fn_start..];
        let gate_call = body
            .find("auth_gate::authorize_request(")
            .expect("gate call missing");
        let match_pos = body.find("match request {").expect("match missing");
        let between = &body[gate_call..match_pos];

        assert!(
            between.contains("Err(response) => return response"),
            "expected gate-Err arm to be exactly \
             `Err(response) => return response` so the dispatcher \
             short-circuits before the match. A loose `return …(response)` \
             could silently wrap, log, or mutate the rejection; we pin the \
             exact shape. Found gate→match slice:\n{}",
            &between[..between.len().min(200)]
        );
    }

    /// Paranoid guard, **not load-bearing**: the compiler already catches
    /// a rename of `LeaderState::connected_peers` or `LeaderState::db`
    /// because the gate call-site in `handle_delivery_request` reads
    /// `&state.connected_peers` / `&state.db` directly. This test only
    /// matters for the narrow case where a future refactor introduces a
    /// builder/getter that *re-exports the same identifier with different
    /// semantics* — e.g. swapping the field for an `Arc<Mutex<…>>` wrapper
    /// behind the same name. Three lines, zero runtime cost; kept for the
    /// signal value to future readers.
    #[test]
    fn leader_state_exposes_fields_the_gate_consumes() {
        let source = include_str!("leader.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(p, _)| p)
            .unwrap_or(source);

        assert!(
            production.contains("pub connected_peers: Arc<RwLock<HashMap<String, ConnectedPeer>>>"),
            "LeaderState.connected_peers must remain the typed handle the \
             gate reads from"
        );
        assert!(
            production.contains("pub db: DbConnection"),
            "LeaderState.db must remain the typed handle the gate reads from"
        );
    }
}

#[cfg(test)]
mod claim_invite_did_binding_tests {
    //! Red regression for the ClaimInvite DID-spoofing vector (Phase 2 of
    //! `docs/plans/2026-06-01-quic-did-auth-primitiv.md`).
    //!
    //! ## The bug these guards lock down
    //!
    //! Today `handle_claim_invite` lifts the claimant's DID directly from
    //! the request payload (`Request::ClaimInvite { did, .. }`) and passes
    //! it to `invite_tokens::validate_invite` as `claimer_did`. The
    //! iroh-QUIC connection only binds the remote `endpoint_id`; nothing
    //! ties the claimant's payload-`did` to the connection cryptographically.
    //! Per §1.2/§4.2 of the plan, this enables two distinct attacks:
    //!
    //! - **Targeted-Invite spoofing (§4.2 scenario 1):** a token has
    //!   `target_did = Alice`; any peer who knows the token can send
    //!   `ClaimInvite { did: "Alice", … }` from their own endpoint and
    //!   becomes "Alice" inside the MLS group.
    //! - **Public-Invite identity spoofing (§4.2 scenario 2):** a token
    //!   has `target_did = None`; any peer can pick a fresh DID (or borrow
    //!   a known one) and have a UCAN minted for that DID.
    //!
    //! `invite_tokens::validate_invite` itself is fine — it correctly
    //! rejects `claimer_did` ≠ `target_did`. The vulnerability is the
    //! *call site*: it has no way to know the connection-verified DID
    //! until the Phase 2 wiring lands.
    //!
    //! ## Why source-text assertions, not full behavioural tests
    //!
    //! Same reason `audience_check_tests` above is source-text-only:
    //! `handle_claim_invite` requires `&LeaderState`, which needs an
    //! `iroh::Endpoint`, an MLS provider, a tokio runtime, and a SQLite
    //! schema with HLC triggers — building all that for a unit test
    //! costs more than these assertions buy us. Real behavioural T5+T6
    //! cases (per §4.4) live in the `haex-e2e-tests` companion PR
    //! (`invitations/targeted-invite-did-mismatch`,
    //! `invitations/public-invite-foreign-did`).
    //!
    //! ## TDD discipline
    //!
    //! These tests are `#[ignore]`d while the rest of the Phase 2 commits
    //! land in sequence so the suite stays green on every commit — the
    //! `#[ignore]` attribute is removed in commit C5
    //! (`feat(space_delivery): bind ClaimInvite to verified DID`) which
    //! is also the commit that makes them pass.

    /// T5 (positive): the connection-verified DID flows into
    /// `handle_claim_invite`. Without a `verified_did` parameter the call
    /// site has nothing but the payload `did` to validate against — which
    /// is exactly the bug.
    #[test]
    fn handle_claim_invite_takes_verified_did_parameter() {
        let source = include_str!("leader.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(p, _)| p)
            .unwrap_or(source);

        assert!(
            production.contains("pub async fn handle_claim_invite(")
                && production.contains("verified_did: &str"),
            "handle_claim_invite must accept the connection-verified DID as \
             a parameter so the claim is gated by the cryptographically \
             bound peer identity rather than the client-supplied payload \
             `did` field. See plan §4.2 scenarios 1+2."
        );
    }

    /// T6 (negative): `validate_invite` must be invoked with the
    /// connection-verified DID, not with the payload `did` field. The
    /// guard pins the exact argument used at the call site — without it,
    /// any peer can spoof `Request::ClaimInvite::did` and pass a token
    /// validation gated only on a string match against `target_did`.
    #[test]
    fn handle_claim_invite_validates_against_verified_did_not_payload_did() {
        let source = include_str!("leader.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(p, _)| p)
            .unwrap_or(source);

        // The validate_invite call sits inside handle_claim_invite. After
        // the C5 fix the fourth positional argument (`claimer_did`) is
        // sourced from the connection-bound `verified_did`, never from
        // the request payload.
        let bytes = production.as_bytes();
        let call_marker = b"invite_tokens::validate_invite(";
        let mut found_correct = false;
        let mut idx = 0;
        while let Some(pos) = bytes
            .windows(call_marker.len())
            .skip(idx)
            .position(|w| w == call_marker)
        {
            let abs = idx + pos;
            idx = abs + call_marker.len();
            // Scan the next ~400 bytes — enough for the multiline call
            // expression to include the claimer argument.
            let end = (abs + 400).min(bytes.len());
            let window = std::str::from_utf8(&bytes[abs..end]).unwrap_or("");
            if window.contains("verified_did") && !window.contains("&did,\n") {
                found_correct = true;
                break;
            }
        }

        assert!(
            found_correct,
            "invite_tokens::validate_invite(…) inside handle_claim_invite must \
             pass `verified_did` (the connection-bound DID), not the payload \
             `did` field. Today the call site uses `&did` (payload-supplied), \
             which lets a peer claim any token by spoofing the `did` field. \
             See plan §4.2 scenarios 1+2 and §5.5 commit 5."
        );
    }
}
