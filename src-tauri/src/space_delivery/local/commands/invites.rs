//! Invite management: create / list / revoke / claim.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use tauri::State;

use crate::critical::CriticalFailureCode;
use crate::database::DbConnection;
use crate::ucan::{Cap, CapabilitySet};
use crate::AppState;

use super::super::invite_tokens;
use super::super::protocol::{Request, Response};
use super::super::types::{ClaimInviteResult, LocalInviteInfo};

/// Create a local invite token (admin-side, requires leader mode).
///
/// If `target_did` is provided, creates a contact invite (1:1, pre-created UCAN).
/// If `target_did` is None, creates a conference invite (anyone can claim, UCAN created at claim time).
/// Returns the token ID.
#[tauri::command]
pub async fn local_delivery_create_invite(
    state: State<'_, AppState>,
    space_id: String,
    target_did: Option<String>,
    capability: String,
    max_uses: u32,
    expires_in_seconds: u64,
    include_history: bool,
) -> Result<String, String> {
    let leader_state = super::peers::get_leader_state(&state, &space_id).await?;

    match target_did {
        Some(did) => {
            // Contact invite: pre-create UCAN since target DID is known
            let admin =
                super::super::ucan::load_admin_identity(&leader_state.db, &leader_state.space_id)
                    .map_err(|e| e.to_string())?;
            // Frontend still emits `"space/<cap>"` (Task 8 removes the
            // prefix); `cap_from_str` strips the bridge on the fly.
            let cap = crate::ucan::cap_from_str(&capability).map_err(|e| e.to_string())?;
            // Every session needs Read to Announce and establish its peer
            // storage connection. It is an explicit companion grant, not an
            // implication of the requested capability. D9 keeps Read/Write
            // terminal while Invite/Admin remain delegatable.
            let capability_set = match cap {
                Cap::Read => CapabilitySet::builder().read(false).build(),
                Cap::Write => CapabilitySet::builder().read(false).write(false).build(),
                Cap::Invite => CapabilitySet::builder().read(false).invite(true).build(),
                Cap::Admin => CapabilitySet::builder().read(false).admin(true).build(),
            };
            let ucan_token = super::super::ucan::create_delegated_ucan(
                &admin.did,
                &admin.private_key_base64,
                &did,
                &leader_state.space_id,
                capability_set,
                None,
                Some(&admin.root_ucan),
                super::super::ucan::MEMBER_UCAN_EXPIRES_IN_SECONDS,
            )
            .map_err(|e| e.to_string())?;

            invite_tokens::create_contact_invite_token(
                &leader_state.db,
                &leader_state.hlc,
                &state.column_sig_key_cache,
                &leader_state.invite_tokens,
                &space_id,
                &did,
                &capability,
                expires_in_seconds,
                include_history,
                ucan_token,
            )
            .map_err(|e| e.to_string())
        }
        None => invite_tokens::create_conference_invite_token(
            &leader_state.db,
            &leader_state.hlc,
            &state.column_sig_key_cache,
            &leader_state.invite_tokens,
            &space_id,
            &capability,
            max_uses,
            expires_in_seconds,
            include_history,
        )
        .await
        .map_err(|e| e.to_string()),
    }
}

/// List active invite tokens for a space (admin-side).
#[tauri::command]
pub async fn local_delivery_list_invites(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<Vec<LocalInviteInfo>, String> {
    let leader_state = super::peers::get_leader_state(&state, &space_id).await?;

    let tokens = leader_state.invite_tokens.read().await;
    let infos = tokens
        .iter()
        .filter(|t| t.space_id == space_id)
        .map(|t| LocalInviteInfo {
            id: t.id.clone(),
            target_did: t.target_did.clone(),
            capabilities: t.capabilities.clone(),
            max_uses: t.max_uses,
            current_uses: t.current_uses,
            expires_at: t
                .expires_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        })
        .collect();

    Ok(infos)
}

/// Revoke an invite token (admin-side).
#[tauri::command]
pub async fn local_delivery_revoke_invite(
    state: State<'_, AppState>,
    space_id: String,
    token_id: String,
) -> Result<(), String> {
    let leader_state = super::peers::get_leader_state(&state, &space_id).await?;

    let mut tokens = leader_state.invite_tokens.write().await;
    let len_before = tokens.len();
    tokens.retain(|t| t.id != token_id);

    if tokens.len() == len_before {
        return Err(format!("Invite token {token_id} not found"));
    }

    Ok(())
}

/// Parameters for persisting the UCAN rows on the claimant's side. Grouped
/// into a struct so callers can't accidentally swap `inviter_did` and
/// `claimant_did` at the call site — that mistake is exactly the bug this
/// helper was extracted to prevent.
pub(crate) struct PersistClaimedUcan<'a> {
    pub space_id: &'a str,
    pub inviter_did: &'a str,
    pub claimant_did: &'a str,
    /// (capability, token) pairs — one per capability the invite granted.
    /// Capabilities are orthogonal grants, not a rank, so every pair gets
    /// its own row rather than picking one to keep.
    pub granted: &'a [(String, String)],
}

/// Persist the UCAN rows that represent the delegation `inviter → claimant`
/// for a freshly-claimed local invite — one lookup row per granted capability.
/// `issuer` is the inviter because the ucan_token is signed by them; storing
/// the claimant there (as an earlier revision did) misrepresents the
/// delegation chain.
///
/// Any prior UCAN rows for `(space_id, audience_did = claimant)` are deleted
/// AFTER all of this claim's new rows land. A leave-then-rejoin cycle leaves
/// the previous UCAN rows behind (the LEAVING-state sync loop needs them to
/// push the membership delete), so without this cleanup a re-invite stacks
/// the new tokens on top of the old. The new invite may even carry
/// different capabilities, and a stale-token-wins resolution in
/// `getUcanForSpaceAsync` would silently use the wrong rights. We insert
/// first so a failure in the cleanup never strands the claimant without a
/// valid UCAN for the space. `haex_ucan_tokens` is not in
/// `SPACE_SCOPED_CRDT_TABLES`, so the cleanup is purely local — peers are
/// unaffected.
pub(crate) fn persist_claimed_ucan(
    db: &DbConnection,
    hlc_guard: &std::sync::MutexGuard<'_, crate::crdt::hlc::HlcService>,
    key_cache: &crate::crdt::column_sig::key_cache::SpaceKeyCache,
    p: PersistClaimedUcan<'_>,
) -> Result<(), String> {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Callers may fan one combined token out into one wire tuple per granted
    // capability (see `Response::InviteClaimed` / `load_existing_claim`) —
    // that shape predates Task 8b, when each row held a single decomposed
    // `space/<cap>` string. Post-8b the `capabilities` column stores the
    // FULL `CapabilitySet` carried by the token, so inserting one row per
    // wire tuple would just persist N identical rows for the same token
    // (bug caught by 06-data-consistency.ts "re-invite ... leaves one UCAN
    // capability set"). Dedupe on `token`: insert exactly one row per unique
    // token, but still verify every advertised capability is actually in
    // that token's set so a malformed wire response can't sneak in.
    let mut new_ids = Vec::new();
    let mut seen_tokens: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (capability, token) in p.granted {
        let claimed_cap = crate::ucan::cap_from_str(capability)
            .map_err(|e| format!("Unrecognized capability {capability}: {e}"))?;
        let parsed = crate::ucan::parse_ucan(token)
            .map_err(|e| format!("Failed to parse claimed UCAN: {e}"))?;
        let token_set = parsed.capabilities.get(p.space_id).ok_or_else(|| {
            format!(
                "Claimed UCAN contains no capability for space {}",
                p.space_id
            )
        })?;
        if !token_set.can(claimed_cap) {
            return Err(format!(
                "Claimed UCAN does not contain its advertised capability {capability}"
            ));
        }
        if !seen_tokens.insert(token.as_str()) {
            continue;
        }
        let ucan_id = uuid::Uuid::new_v4().to_string();
        let capability_set_json = serde_json::to_string(token_set)
            .map_err(|e| format!("Failed to serialize CapabilitySet: {e}"))?;
        crate::database::core::execute_with_crdt(
            "INSERT INTO haex_ucan_tokens (id, space_id, issuer_did, audience_did, capabilities, token, issued_at, expires_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
                .to_string(),
            vec![
                serde_json::Value::String(ucan_id.clone()),
                serde_json::Value::String(p.space_id.to_string()),
                serde_json::Value::String(p.inviter_did.to_string()),
                serde_json::Value::String(p.claimant_did.to_string()),
                serde_json::Value::String(capability_set_json),
                serde_json::Value::String(token.clone()),
                serde_json::Value::Number(serde_json::Number::from(now_secs)),
                serde_json::Value::Number(serde_json::Number::from(
                    now_secs + super::super::ucan::MEMBER_UCAN_EXPIRES_IN_SECONDS as i64,
                )),
            ],
            db,
            hlc_guard,
            key_cache,
        )
        .map_err(|e| format!("Failed to persist UCAN: {e}"))?;
        new_ids.push(ucan_id);
    }

    // Cleanup is best-effort. If the new rows landed but the cleanup query
    // fails (e.g. transient lock contention), the next consumer simply sees
    // this claim's fresh tokens plus stale leftovers instead of just the
    // new ones — `getUcanForSpaceAsync` is keyed by spaceId, so it returns
    // *some* valid UCAN either way. A failing DELETE must NOT roll back the
    // INSERTs — that would leave the claimant without authentication after
    // a successful ClaimInvite, which is the exact failure mode the
    // insert-first ordering exists to prevent.
    //
    // `NOT IN` covers every id inserted above — not just one — so deleting
    // stale rows from a *previous* claim never deletes a sibling capability
    // from *this* claim.
    if !new_ids.is_empty() {
        let placeholders: Vec<String> = (3..3 + new_ids.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "DELETE FROM haex_ucan_tokens WHERE space_id = ?1 AND audience_did = ?2 AND id NOT IN ({})",
            placeholders.join(", ")
        );
        let mut params = vec![
            serde_json::Value::String(p.space_id.to_string()),
            serde_json::Value::String(p.claimant_did.to_string()),
        ];
        params.extend(new_ids.into_iter().map(serde_json::Value::String));
        let _ = crate::database::core::execute_with_crdt(sql, params, db, hlc_guard, key_cache);
    }

    Ok(())
}

/// Resolve the inviter's DID from a pending-invite row identified by
/// `(space_id, token_id)`. The pending-invite row is inserted by the UI the
/// moment an invite arrives and `inviter_did` is `NOT NULL` in the schema —
/// so this lookup is the single source of truth for who sent the invite.
/// Callers must not pass `inviter_did` in from the UI; that historically
/// caused the parameter to be forgotten in the invoke wire-up.
pub(crate) fn resolve_inviter_did_for_invite(
    space_id: &str,
    token_id: &str,
    db: &DbConnection,
) -> Result<String, String> {
    let rows = crate::database::core::select_with_crdt(
        "SELECT inviter_did FROM haex_pending_invites WHERE space_id = ?1 AND token_id = ?2 LIMIT 1"
            .to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(token_id.to_string()),
        ],
        db,
    )
    .map_err(|e| format!("Failed to look up pending invite: {e}"))?;

    rows.first()
        .and_then(|r| r.first())
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "Pending invite not found for space={} token={}",
                &space_id[..8.min(space_id.len())],
                &token_id[..8.min(token_id.len())]
            )
        })
}

/// Resolve the local `haex_identities.id` for the inviter's DID.
///
/// The claimant's UI must ensure a row for `inviter_did` exists before calling
/// `local_delivery_claim_invite` — the row represents the *other* party's
/// identity and therefore has no `private_key` on the claimant's device.
pub(crate) fn resolve_owner_identity_id(
    inviter_did: &str,
    db: &DbConnection,
) -> Result<String, String> {
    let rows = crate::database::core::select_with_crdt(
        "SELECT id FROM haex_identities WHERE did = ?1 LIMIT 1".to_string(),
        vec![serde_json::Value::String(inviter_did.to_string())],
        db,
    )
    .map_err(|e| format!("Failed to look up inviter identity: {e}"))?;

    rows.first()
        .and_then(|r| r.first())
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "Inviter identity for DID {} not present locally — UI must insert it before claiming",
                &inviter_did[..30.min(inviter_did.len())]
            )
        })
}

/// Claim a local invite (invitee-side). Connects to leader via QUIC,
/// sends KeyPackages and token, receives MLS welcome + UCAN.
#[tauri::command]
pub async fn local_delivery_claim_invite(
    state: State<'_, AppState>,
    leader_endpoint_id: String,
    leader_relay_url: Option<String>,
    space_id: String,
    space_name: String,
    token_id: String,
    identity_did: String,
    label: Option<String>,
    identity_public_key: Option<String>,
) -> Result<ClaimInviteResult, String> {
    let log = |level: &str, msg: &str| {
        let _ = crate::logging::insert_log(&state, level, "ClaimInvite", None, msg, None, "rust");
    };

    // Fail fast if the pending invite is missing — avoids an expensive QUIC
    // round-trip and surfaces the error before we generate MLS KeyPackages.
    let lookup_db = DbConnection(state.db.0.clone());
    let inviter_did = resolve_inviter_did_for_invite(&space_id, &token_id, &lookup_db)?;

    log(
        "info",
        &format!(
            "ENTER local_delivery_claim_invite space={} token={} inviter_did={}",
            &space_id[..8.min(space_id.len())],
            &token_id[..8.min(token_id.len())],
            &inviter_did[..20.min(inviter_did.len())]
        ),
    );
    log(
        "info",
        &format!(
            "Starting claim: leader={} space={} token={}",
            &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
            &space_id[..8.min(space_id.len())],
            &token_id[..8.min(token_id.len())]
        ),
    );

    // 1. Get iroh endpoint
    let endpoint = state.peer_storage.read().await;
    if !endpoint.is_running() {
        log("error", "ABORT: peer endpoint not running");
        return Err("Peer storage endpoint not running".to_string());
    }
    let our_endpoint_id = endpoint.endpoint_id().to_string();
    let iroh_endpoint = endpoint
        .endpoint_ref()
        .ok_or("Endpoint not running")?
        .clone();
    let configured_relay = endpoint.configured_relay_url().cloned();
    drop(endpoint);

    // 2. Load the claimant's signing key. Needed both for the PoP attached to
    //    each freshly-generated KeyPackage below and for the server-initiated
    //    quic_did_auth handshake — ClaimInvite is the first time this DID ever
    //    connects to the leader, so the handshake is what cryptographically
    //    binds the claim to this DID (plan §4.2 scenarios 1+2).
    let db_for_identity = DbConnection(state.db.0.clone());
    let our_identity =
        super::super::quic_retry::load_signing_identity_for_did(&db_for_identity, &identity_did)
            .map_err(|e| {
                log("error", &format!("identity load failed: {e}"));
                e.to_string()
            })?;

    // 3. Generate MLS KeyPackages
    let key_packages_raw = crate::mls::blocking::generate_key_packages(
        state.db.0.clone(),
        10,
        our_identity.signing_key.clone(),
    )
    .await
    .map_err(|e| {
        log("error", &format!("MLS KeyPackage generation failed: {e}"));
        format!("Failed to generate key packages: {e}")
    })?;
    let key_packages_b64: Vec<String> = key_packages_raw
        .iter()
        .map(|(kp, _)| BASE64.encode(kp))
        .collect();
    let pops_b64: Vec<String> = key_packages_raw
        .iter()
        .map(|(_, pop)| BASE64.encode(pop))
        .collect();
    log(
        "info",
        &format!("Generated {} MLS KeyPackages", key_packages_b64.len()),
    );

    // 3. Connect to leader via QUIC and send ClaimInvite
    let (addr, relay) = super::super::quic_retry::build_endpoint_addr_with_relay(
        &iroh_endpoint,
        &leader_endpoint_id,
        leader_relay_url.as_deref(),
        configured_relay.as_ref(),
    )
    .map_err(|e| format!("Invalid leader endpoint ID: {e}"))?;

    log(
        "info",
        &format!(
            "Connecting to {} via relay {:?}",
            &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
            relay.as_ref().map(|u| u.to_string())
        ),
    );

    // Encode once outside the retry loop — the request bytes are identical
    // across attempts, including the (expensively-generated) KeyPackages.
    //
    // The claimant DID is no longer carried on the wire — the leader reads it
    // from the quic_did_auth handshake state for this connection (the same
    // signing key used by `complete_client_did_auth` below).
    let req = Request::ClaimInvite {
        space_id: space_id.clone(),
        token: token_id.clone(),
        endpoint_id: our_endpoint_id,
        key_packages: key_packages_b64,
        pops: pops_b64,
        label,
        public_key: identity_public_key,
    };
    let bytes = super::super::protocol::encode(&req)
        .map_err(|e| format!("Failed to encode request: {e}"))?;

    // QUIC connect + send + read with automatic retry on transient failures.
    let response = super::super::quic_retry::send_request_with_retry(
        "ClaimInvite",
        &iroh_endpoint,
        addr,
        &identity_did,
        &our_identity.signing_key,
        &bytes,
    )
    .await
    .map_err(|e| {
        log("error", &format!("QUIC send failed: {e}"));
        format!("{e}")
    })?;

    // 4. Process response
    let (welcome_b64, granted) = match response {
        Response::InviteClaimed { welcome, granted } => {
            let caps = granted
                .iter()
                .map(|g| g.capability.as_str())
                .collect::<Vec<_>>()
                .join(",");
            log(
                "info",
                &format!("Invite claimed successfully, capabilities={caps}"),
            );
            let granted: Vec<(String, String)> = granted
                .into_iter()
                .map(|g| (g.capability, g.token))
                .collect();
            (welcome, granted)
        }
        Response::Error { message } => {
            log("error", &format!("Leader rejected: {message}"));
            return Err(format!("Leader rejected invite: {message}"));
        }
        _ => {
            log("error", "Unexpected response variant from leader");
            return Err("Unexpected response from leader".to_string());
        }
    };

    // 5. Process MLS welcome (crash-safe: stage → process → delete)
    let welcome_bytes = BASE64
        .decode(&welcome_b64)
        .map_err(|e| format!("Failed to decode welcome: {e}"))?;

    let staging_id = uuid::Uuid::new_v4().to_string();
    let staging_db = DbConnection(state.db.0.clone());
    crate::database::core::execute(
        "INSERT INTO haex_mls_pending_welcomes_no_sync (id, space_id, welcome_payload, source, created_at) \
         VALUES (?1, ?2, ?3, 'quic', datetime('now'))".to_string(),
        vec![
            serde_json::Value::String(staging_id.clone()),
            serde_json::Value::String(space_id.clone()),
            serde_json::Value::String(BASE64.encode(&welcome_bytes)),
        ],
        &staging_db,
    )
    .map_err(|e| format!("Failed to stage welcome: {e}"))?;

    crate::mls::blocking::process_welcome(state.db.0.clone(), space_id.clone(), welcome_bytes)
        .await
        .map_err(|e| format!("Failed to process MLS welcome: {e}"))?;

    let _ = crate::database::core::execute(
        "DELETE FROM haex_mls_pending_welcomes_no_sync WHERE id = ?1".to_string(),
        vec![serde_json::Value::String(staging_id)],
        &staging_db,
    );

    // 6. Persist space locally (type = 'local', status = 'active')
    // Capabilities are derived at runtime from UCAN tokens, not stored on the space
    let db = DbConnection(state.db.0.clone());

    // eprintln! directly (not log()) because log() itself locks HLC — if the
    // mutex is contended, a log() call here would deadlock silently.
    eprintln!("[ClaimInvite] [trace] BEFORE hlc.lock()");
    let hlc_guard = state
        .lock_or_fail(
            &state.hlc,
            CriticalFailureCode::HlcMutexPoisoned,
            "space_delivery::local::commands::claim_invite",
            serde_json::json!({}),
        )
        .map_err(|e| e.to_string())?;
    eprintln!("[ClaimInvite] [trace] AFTER hlc.lock() — guard acquired");

    // owner_identity_id must reference the *inviter's* identity row — the
    // space was created by them, not by us. The UI ensures a row for
    // `inviter_did` exists on the claimant's device before invoking this
    // command (see stores/spaces/invites.ts: ensureIdentityForDidAsync).
    eprintln!("[ClaimInvite] [trace] BEFORE resolve_owner_identity_id");
    let owner_identity_id = resolve_owner_identity_id(&inviter_did, &db)?;
    eprintln!(
        "[ClaimInvite] [trace] AFTER resolve_owner_identity_id → owner_id={}",
        &owner_identity_id[..8.min(owner_identity_id.len())]
    );

    eprintln!("[ClaimInvite] [trace] BEFORE execute_with_crdt INSERT haex_spaces");
    crate::database::core::execute_with_crdt(
        "INSERT OR IGNORE INTO haex_spaces (id, type, status, name, owner_identity_id) VALUES (?1, 'local', 'active', ?2, ?3)".to_string(),
        vec![
            serde_json::Value::String(space_id.clone()),
            serde_json::Value::String(space_name),
            serde_json::Value::String(owner_identity_id),
        ],
        &db,
        &hlc_guard,
        &state.column_sig_key_cache,
    )
    .map_err(|e| format!("Failed to persist space: {e}"))?;
    eprintln!("[ClaimInvite] [trace] AFTER execute_with_crdt INSERT haex_spaces");

    // 7. Persist UCAN tokens — one per granted capability
    eprintln!("[ClaimInvite] [trace] BEFORE persist_claimed_ucan");
    persist_claimed_ucan(
        &db,
        &hlc_guard,
        &state.column_sig_key_cache,
        PersistClaimedUcan {
            space_id: &space_id,
            inviter_did: &inviter_did,
            claimant_did: &identity_did,
            granted: &granted,
        },
    )?;
    eprintln!("[ClaimInvite] [trace] AFTER persist_claimed_ucan");

    // 8. Mark the pending-invite row as accepted. Doing this here keeps the
    // accept flow atomic inside the Tauri command — the UI previously did it
    // after further async steps (persistSpace, loadSpaces, addSelfAsMember),
    // which left the invite stuck on "pending" whenever any of those hung or
    // threw.
    crate::database::core::execute_with_crdt(
        "UPDATE haex_pending_invites SET status = 'accepted', responded_at = datetime('now') WHERE space_id = ?1 AND token_id = ?2".to_string(),
        vec![
            serde_json::Value::String(space_id.clone()),
            serde_json::Value::String(token_id.clone()),
        ],
        &db,
        &hlc_guard,
        &state.column_sig_key_cache,
    )
    .map_err(|e| format!("Failed to mark invite as accepted: {e}"))?;
    eprintln!("[ClaimInvite] [trace] AFTER mark accepted — returning Ok");

    // 8a. Task C4: hydrate the column-sig SpaceKeyCache for this newly-joined
    //     space. On the claimant side the local `haex_space_members` row is
    //     seeded via CRDT sync from the leader (may not have arrived yet at
    //     this point), so this reload is often a miss — `get_or_reload` will
    //     retry via JIT on the next signing call.
    super::super::column_sig_hook::warm_column_sig_cache(
        &state.column_sig_key_cache,
        &db,
        &space_id,
    );

    // 8b. Clean up other pending invites for the same space — once we've
    //     joined, leftover invites (from the same inviter via duplicate
    //     retries that slipped past idempotency, or from other inviters
    //     who also offered access to this space) are no longer actionable
    //     and would otherwise sit in the UI until the 7-day cleanup tick.
    //     CRDT delete is safe — pending-invite rows have unique UUIDs that
    //     don't collide with any row on the sender's device.
    //
    //     Best-effort: a cleanup failure must not unwind the successful
    //     accept, but silently swallowing it would make stale rows in the
    //     UI undiagnosable. eprintln! only — log_to_db would deadlock on
    //     the still-held HLC guard.
    if let Err(e) = crate::database::core::execute_with_crdt(
        "DELETE FROM haex_pending_invites WHERE space_id = ?1 AND token_id != ?2 AND status = 'pending'".to_string(),
        vec![
            serde_json::Value::String(space_id.clone()),
            serde_json::Value::String(token_id.clone()),
        ],
        &db,
        &hlc_guard,
        &state.column_sig_key_cache,
    ) {
        eprintln!(
            "[ClaimInvite] [warn] sibling pending-invite cleanup failed for space={} token={}: {e}",
            &space_id[..8.min(space_id.len())],
            &token_id[..8.min(token_id.len())],
        );
    }

    Ok(ClaimInviteResult {
        space_id,
        capabilities: granted.into_iter().map(|(cap, _)| cap).collect(),
    })
}
