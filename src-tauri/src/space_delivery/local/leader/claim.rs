//! ClaimInvite handler and its persistence helpers.

use tauri::Manager;
use time::OffsetDateTime;

use super::super::buffer;
use super::super::invite_tokens;
use super::super::protocol::{Request, Response};
use super::super::types::ConnectedPeer;
use super::notify::notify_all_mls;
use super::util::{base64_decode, base64_encode};
use super::LeaderState;
use crate::critical::CriticalFailureCode;
use serde_json::Value as JsonValue;

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
    let (space_id, token, endpoint_id, key_packages, pops, label, public_key) = match request {
        Request::ClaimInvite {
            space_id,
            token,
            endpoint_id,
            key_packages,
            pops,
            label,
            public_key,
        } => (
            space_id,
            token,
            endpoint_id,
            key_packages,
            pops,
            label,
            public_key,
        ),
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

    debug_assert_eq!(
        space_id, state.space_id,
        "ClaimInvite routed to wrong leader"
    );

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
                let admin = match super::super::ucan::load_admin_identity(&state.db, &space_id) {
                    Ok(a) => a,
                    Err(e) => {
                        return Response::Error {
                            message: format!("Failed to load admin identity: {e}"),
                        }
                    }
                };
                match super::super::ucan::create_delegated_ucan(
                    &admin.did,
                    &admin.private_key_base64,
                    &did,
                    &space_id,
                    &capability,
                    Some(&admin.root_ucan),
                    super::super::ucan::MEMBER_UCAN_EXPIRES_IN_SECONDS,
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
    for (pkg_b64, pop_b64) in key_packages.iter().zip(pops.iter()) {
        if let (Ok(blob), Ok(pop_blob)) = (base64_decode(pkg_b64), base64_decode(pop_b64)) {
            let _ = buffer::store_key_package(&state.db, &space_id, &did, &blob, &pop_blob);
        }
    }

    // 5. Consume one key package for MLS add_member
    let (key_package_blob, pop_blob) = match buffer::consume_key_package(&state.db, &space_id, &did)
    {
        Ok(Some(pair)) => pair,
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
        did.clone(),
        pop_blob,
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
        let msg_id =
            match buffer::store_message(&state.db, &space_id, &did, "commit", &bundle.commit) {
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
        let resolved_label = member_label.unwrap_or_else(|| did.chars().take(16).collect());

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
        if let Err(e) =
            invite_tokens::consume_invite(&state.db, &state.hlc, &state.invite_tokens, &token).await
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

    let admin = match super::super::ucan::load_admin_identity(&state.db, space_id) {
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
            now_secs + super::super::ucan::MEMBER_UCAN_EXPIRES_IN_SECONDS as i64,
        )),
    ];
    if let Err(e) = crate::database::core::execute_with_crdt(sql, params, &state.db, &hlc_guard) {
        eprintln!("[SpaceDelivery] persist_admin_ucan: insert failed: {e}");
    }
}
