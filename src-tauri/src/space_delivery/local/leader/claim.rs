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
use crate::ucan::{cap_from_str, Cap, CapabilitySet};
use serde_json::Value as JsonValue;

/// Lift a single [`Cap`] + `delegatable` bit into a [`CapabilitySet`] for
/// [`create_delegated_ucan`]. Claim-invite mints one UCAN per stored
/// capability (each row in `haex_ucan_tokens` grants a single cap), so
/// every issued token embeds a set of exactly one entry.
fn build_singleton_capset(cap: Cap, delegatable: bool) -> CapabilitySet {
    let builder = CapabilitySet::builder();
    match cap {
        Cap::Read => builder.read(delegatable),
        Cap::Write => builder.write(delegatable),
        Cap::Invite => builder.invite(delegatable),
        Cap::Admin => builder.admin(delegatable),
    }
    .build()
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
    //
    //    A row in `haex_ucan_tokens` alone is NOT sufficient evidence of an
    //    in-flight retry: `leaveSpaceAsync`'s local-leave branch (see
    //    `src/stores/spaces/index.ts`) deletes UCANs on the *leaving* device
    //    only and deliberately never notifies this leader, so a departed
    //    DID's row here can be a stale leftover from a membership that has
    //    since ended rather than an unfinished attempt at the current one.
    //    Requiring the DID to still be an *active* member (tombstoned by the
    //    same background sync that eventually carries the leave's
    //    `haex_space_members` delete to us) tells the two cases apart: a
    //    true retry always finds the member still active (step 12 below
    //    inserted it before this function could return), while a re-invite
    //    after a leave finds it gone and is correctly treated as a fresh
    //    claim instead of resurrecting the old grant.
    let existing = load_existing_claim(&state.db, &space_id, &did);
    let is_retry = match &existing {
        None => false,
        Some(_) => match super::super::ucan::is_active_space_member(&state.db, &space_id, &did) {
            Ok(is_member) => is_member,
            Err(e) => {
                // Fail closed: an unknown membership state must not be
                // silently treated as "not a member" (which would route a
                // true retry through the fresh-claim path — re-validating an
                // already-consumed, single-use invite token and rejecting a
                // legitimate retry outright).
                return Response::Error {
                    message: format!("Failed to check space membership: {e}"),
                };
            }
        },
    };
    if is_retry {
        eprintln!(
            "[SpaceDelivery] ClaimInvite: retry for {} in space {} — regenerating welcome with fresh KeyPackage",
            &did[..20.min(did.len())],
            &space_id[..12.min(space_id.len())],
        );
    }

    // 2. Resolve capabilities + UCANs — one UCAN per granted capability.
    //    Capabilities are orthogonal grants (write/invite/admin do not
    //    imply or rank above one another — see ADR 0002 Phase C), so every
    //    entry in the invite's capabilities array gets its own
    //    independently-issued, independently-verifiable UCAN.
    //    - Retry: reuse the previously-issued UCANs (tokens already
    //      consumed in the first attempt, no re-validation needed).
    //    - First attempt: read-only validate the token; consume happens at
    //      step 13 only after the rest of the flow succeeds.
    let granted: Vec<(String, String)> = if is_retry {
        existing.expect("is_retry implies load_existing_claim returned Some")
    } else {
        let (capabilities, pre_ucan) = match invite_tokens::validate_invite(
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

        // 3. Determine UCANs: use the pre-created one (contact invites,
        //    always exactly one capability) or create one per capability
        //    now (conference invites — UCANs are created at claim time).
        match (pre_ucan, capabilities.as_slice()) {
            (Some(ucan), [single_capability]) => vec![(single_capability.clone(), ucan)],
            (_, capabilities) => {
                let admin = match super::super::ucan::load_admin_identity(&state.db, &space_id) {
                    Ok(a) => a,
                    Err(e) => {
                        return Response::Error {
                            message: format!("Failed to load admin identity: {e}"),
                        }
                    }
                };
                let mut issued = Vec::with_capacity(capabilities.len());
                for capability in capabilities {
                    // Frontend + invite-token wire still emits `"space/<cap>"`
                    // strings (Task 8 removes the prefix); `cap_from_str`
                    // strips the bridge on the fly.
                    let cap = match cap_from_str(capability) {
                        Ok(c) => c,
                        Err(e) => {
                            return Response::Error {
                                message: format!("Unrecognized capability {capability}: {e}"),
                            }
                        }
                    };
                    // D9: admin-tier grants (Admin, Invite) stay delegatable
                    // so the claimant can further delegate their own peer
                    // set; Write/Read are terminal. Mirrors the choice in
                    // `space_delivery::local::commands::invites` — matching
                    // heuristics on both create sites keeps
                    // contact-invite and conference-invite tokens
                    // observationally identical.
                    let delegatable = matches!(cap, Cap::Admin | Cap::Invite);
                    let capability_set = build_singleton_capset(cap, delegatable);
                    match super::super::ucan::create_delegated_ucan(
                        &admin.did,
                        &admin.private_key_base64,
                        &did,
                        &space_id,
                        capability_set,
                        None,
                        Some(&admin.root_ucan),
                        super::super::ucan::MEMBER_UCAN_EXPIRES_IN_SECONDS,
                    ) {
                        Ok(t) => issued.push((capability.clone(), t)),
                        Err(e) => {
                            return Response::Error {
                                message: format!("Failed to create UCAN: {e}"),
                            }
                        }
                    }
                }
                issued
            }
        }
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
        // Plan §5.0: Add commits do not carry a receive-side committer-
        // capability proof — the KeyPackage PoP + Phase-1 addee-membership
        // check bound them upstream (this is the ClaimInvite leader-relay
        // path documented in `authorize_local_removal`'s "NOT applied to
        // add_member" note).
        let msg_id = match buffer::store_message(
            &state.db,
            &space_id,
            &did,
            "commit",
            &bundle.commit,
            None,
            None,
        ) {
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
    persist_admin_ucan(state, &space_id, &did, &granted, is_retry);

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
        let app_state = state.app_handle.state::<crate::AppState>();
        if let Err(e) = crate::database::core::execute_with_crdt(
            ensure_identity_sql,
            ensure_identity_params,
            &state.db,
            &hlc_guard,
            &app_state.column_sig_key_cache,
        ) {
            return Response::Error {
                message: format!("Failed to persist member identity: {e}"),
            };
        }

        let insert_member_sql = "INSERT OR IGNORE INTO haex_space_members \
            (id, space_id, identity_id, role, joined_at) \
            SELECT ?1, ?2, id, ?3, ?4 FROM haex_identities WHERE did = ?5"
            .to_string();
        // `haex_space_members.role` is a single legacy display column, not
        // an authorization source (that's `haex_ucan_tokens`, checked
        // per-capability by the AuthGate) — it holds whichever capability
        // happens to be first in `granted`, not the full orthogonal set.
        let role = granted
            .first()
            .map(|(cap, _)| cap.clone())
            .unwrap_or_else(|| "space/read".to_string());
        let member_params = vec![
            JsonValue::String(uuid::Uuid::new_v4().to_string()),
            JsonValue::String(space_id.clone()),
            JsonValue::String(role),
            JsonValue::String(now),
            JsonValue::String(did.clone()),
        ];
        if let Err(e) = crate::database::core::execute_with_crdt(
            insert_member_sql,
            member_params,
            &state.db,
            &hlc_guard,
            &app_state.column_sig_key_cache,
        ) {
            return Response::Error {
                message: format!("Failed to persist space member: {e}"),
            };
        }
    }

    // 12b. Task C4: keep the SpaceKeyCache warm for this space. The leader
    //      already loaded its own signing key at vault-open, so this is a
    //      defensive JIT reload — a no-op cache hit in the common path and
    //      a self-repair if the entry was evicted mid-session (e.g. by a
    //      concurrent `local_delivery_stop` + restart cycle).
    super::super::column_sig_hook::warm_column_sig_cache(
        &state
            .app_handle
            .state::<crate::AppState>()
            .column_sig_key_cache,
        &state.db,
        &space_id,
    );

    // 13. Consume the token — **only now**, after the claim has fully
    //     succeeded. If anything above failed, the token is still unspent
    //     and the invitee can retry without a manually re-issued invite.
    //
    //     Skip on retry: the token was already consumed by the first
    //     attempt and incrementing again would (a) overshoot `max_uses` for
    //     single-use contact invites and (b) double-count for multi-use
    //     conference invites.
    if !is_retry {
        let app_state = state.app_handle.state::<crate::AppState>();
        if let Err(e) = invite_tokens::consume_invite(
            &state.db,
            &state.hlc,
            &app_state.column_sig_key_cache,
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

    // 14. Return welcome + one UCAN per granted capability
    Response::InviteClaimed {
        welcome: base64_encode(&welcome_blob),
        granted: granted
            .into_iter()
            .map(
                |(capability, token)| super::super::protocol::ClaimedCapabilityUcan {
                    capability,
                    token,
                },
            )
            .collect(),
    }
}

/// Look up all already-granted UCANs for this DID in this space, if any.
/// Returns (capability, ucan_token) pairs — one per previously-issued
/// capability — so the idempotency path can re-serve exactly the same
/// values a previous successful claim produced. Capabilities are
/// orthogonal grants, so a prior claim may have issued several.
///
/// Under the Task-8b column layout each row's `capabilities` is a JSON
/// [`CapabilitySet`]. This flattens back to one `("space/<cap>", token)`
/// entry per set-entry so the retry path reconstructs the pre-8b response
/// shape (`Vec<ClaimedCapabilityUcan>`) verbatim — one output pair per cap
/// held by each stored UCAN.
fn load_existing_claim(
    db: &crate::database::DbConnection,
    space_id: &str,
    claimer_did: &str,
) -> Option<Vec<(String, String)>> {
    let rows = crate::database::core::select_with_crdt(
        "SELECT capabilities, token FROM haex_ucan_tokens \
         WHERE space_id = ?1 AND audience_did = ?2"
            .to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(claimer_did.to_string()),
        ],
        db,
    )
    .ok()?;

    let granted: Vec<(String, String)> = rows
        .iter()
        .flat_map(|row| {
            let capabilities = row.first().and_then(|v| v.as_str()).unwrap_or("");
            let ucan = row.get(1).and_then(|v| v.as_str()).unwrap_or("");
            let set: CapabilitySet = match serde_json::from_str(capabilities) {
                Ok(s) => s,
                Err(_) => return Vec::new().into_iter(),
            };
            set.entries()
                .map(|e| (format!("space/{}", cap_wire_name(e.cap)), ucan.to_string()))
                .collect::<Vec<_>>()
                .into_iter()
        })
        .collect();

    if granted.is_empty() {
        None
    } else {
        Some(granted)
    }
}

/// Wire-name for a [`Cap`] — matches [`cap_from_str`]'s accepted lowercase
/// tokens so a `("space/<name>", token)` pair round-trips through
/// [`load_existing_claim`] into the same [`Cap`] the leader originally
/// minted.
fn cap_wire_name(cap: Cap) -> &'static str {
    match cap {
        Cap::Read => "read",
        Cap::Write => "write",
        Cap::Invite => "invite",
        Cap::Admin => "admin",
    }
}

/// Returns `true` if `haex_ucan_tokens` already has a row whose parsed
/// [`CapabilitySet`] contains `capability` for this `(space_id,
/// audience_did)` pair — under the Task-8b column layout, the check is a
/// set-membership predicate on the parsed JSON rather than a bare-string
/// equality on the pre-8b `capability` column.
fn admin_ucan_row_exists(
    db: &crate::database::DbConnection,
    space_id: &str,
    audience_did: &str,
    capability: &str,
) -> bool {
    let Ok(needed) = cap_from_str(capability) else {
        return false;
    };
    crate::database::core::select_with_crdt(
        "SELECT capabilities FROM haex_ucan_tokens \
         WHERE space_id = ?1 AND audience_did = ?2"
            .to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(audience_did.to_string()),
        ],
        db,
    )
    .map(|rows| {
        rows.iter().any(|row| {
            row.first()
                .and_then(|v| v.as_str())
                .and_then(|s| serde_json::from_str::<CapabilitySet>(s).ok())
                .map(|set| set.can(needed))
                .unwrap_or(false)
        })
    })
    .unwrap_or(false)
}

/// Persist the granted UCANs on the admin's side so subsequent claim
/// retries for the same DID can be detected and routed through the
/// regenerate path. Errors are logged and swallowed: the UCANs were
/// successfully delivered to the invitee regardless, and losing a row only
/// means the next retry will be treated as a first attempt (still safe —
/// the duplicate-leaf handling in `add_member` covers it).
///
/// On a retry (`is_retry`), skips a given `(capability, token)` pair if a
/// row for that exact `(space_id, audience_did, capability)` triple already
/// exists — avoids inserting a duplicate on every retry of the same claim,
/// since `granted` on a retry is reloaded verbatim from that same row.
/// Checked per capability, not per `(space_id, audience_did)`: capabilities
/// are independent grants, so an existing `space/read` row must not block a
/// fresh `space/write` one.
///
/// On a fresh (non-retry) claim, any pre-existing rows for
/// `(space_id, audience_did)` are purged first instead. These can only be
/// leftovers from a past membership that has since ended — `is_retry` is
/// false here specifically because the DID is no longer an active member
/// (see step 1) — most commonly a local self-leave, which never notifies
/// this leader (`leaveSpaceAsync`'s local branch). Without the purge, a
/// rejoin's fresh grant would sit alongside the stale one (or be skipped
/// outright by the exists-check for a capability the DID held before), and
/// a later true retry of *this* claim would reload the stale row instead.
fn persist_admin_ucan(
    state: &LeaderState,
    space_id: &str,
    audience_did: &str,
    granted: &[(String, String)],
    is_retry: bool,
) {
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

    if !is_retry {
        let purge_sql =
            "DELETE FROM haex_ucan_tokens WHERE space_id = ?1 AND audience_did = ?2".to_string();
        let purge_params = vec![
            JsonValue::String(space_id.to_string()),
            JsonValue::String(audience_did.to_string()),
        ];
        if let Err(e) = crate::database::core::execute_with_crdt(
            purge_sql,
            purge_params,
            &state.db,
            &hlc_guard,
            &app_state.column_sig_key_cache,
        ) {
            eprintln!("[SpaceDelivery] persist_admin_ucan: purge of stale rows failed: {e}");
        }
    }

    for (capability, ucan_token) in granted {
        if is_retry && admin_ucan_row_exists(&state.db, space_id, audience_did, capability) {
            continue;
        }

        let ucan_id = uuid::Uuid::new_v4().to_string();
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        // Task 8b: the `capabilities` column stores a JSON [`CapabilitySet`],
        // not a bare cap string. Each row is one delegation UCAN — the
        // claim-loop above still mints one UCAN per cap, so the persisted
        // set is always a singleton here. Mirror the leader-side
        // `delegatable` policy (Admin/Invite delegatable, Write/Read
        // terminal) so the stored set matches what the token carries.
        let cap = match cap_from_str(capability) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "[SpaceDelivery] persist_admin_ucan: unrecognized capability {capability}: {e}"
                );
                continue;
            }
        };
        let delegatable = matches!(cap, Cap::Admin | Cap::Invite);
        let capability_set_json = serde_json::to_string(&build_singleton_capset(cap, delegatable))
            .expect("CapabilitySet serialization is infallible");
        let sql = "INSERT OR IGNORE INTO haex_ucan_tokens \
            (id, space_id, issuer_did, audience_did, capabilities, token, issued_at, expires_at) \
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
            .to_string();
        let params = vec![
            JsonValue::String(ucan_id),
            JsonValue::String(space_id.to_string()),
            JsonValue::String(admin.did.clone()),
            JsonValue::String(audience_did.to_string()),
            JsonValue::String(capability_set_json),
            JsonValue::String(ucan_token.to_string()),
            JsonValue::Number(serde_json::Number::from(now_secs)),
            JsonValue::Number(serde_json::Number::from(
                now_secs + super::super::ucan::MEMBER_UCAN_EXPIRES_IN_SECONDS as i64,
            )),
        ];
        if let Err(e) = crate::database::core::execute_with_crdt(
            sql,
            params,
            &state.db,
            &hlc_guard,
            &app_state.column_sig_key_cache,
        ) {
            eprintln!("[SpaceDelivery] persist_admin_ucan: insert failed: {e}");
        }
    }
}
