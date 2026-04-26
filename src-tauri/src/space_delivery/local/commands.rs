//! Tauri commands for the local delivery service.

use std::collections::HashMap;
use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use tauri::State;
use tokio::sync::RwLock;

use crate::database::DbConnection;
use crate::AppState;

use super::invite_tokens;
use super::leader::LeaderState;
use super::multi_leader::MultiSpaceLeaderHandler;
use super::protocol::{Request, Response};
use super::types::{ClaimInviteResult, DeliveryStatus, ElectionResultInfo, LeaderInfo, LocalInviteInfo};

/// Start leader mode for a local space.
/// Inserts a new LeaderState into the shared map. On the first call,
/// registers the MultiSpaceLeaderHandler on the QUIC endpoint.
#[tauri::command]
pub async fn local_delivery_start(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String> {
    let db_conn = DbConnection(state.db.0.clone());
    let existing_tokens = invite_tokens::load_invite_tokens(&db_conn, &space_id)
        .unwrap_or_default();

    let hlc_clone = state.hlc.lock().map_err(|_| "HLC lock poisoned".to_string())?.clone();

    let leader_state = Arc::new(LeaderState {
        db: db_conn,
        hlc: Arc::new(std::sync::Mutex::new(hlc_clone)),
        app_handle: app.clone(),
        space_id: space_id.clone(),
        connected_peers: Arc::new(RwLock::new(HashMap::new())),
        notification_senders: Arc::new(RwLock::new(HashMap::new())),
        invite_tokens: Arc::new(RwLock::new(existing_tokens)),
    });

    let mut leaders = state.leader_state.write().await;
    let is_first = leaders.is_empty();
    leaders.insert(space_id.clone(), leader_state);
    drop(leaders);

    // Register handler only once — it holds the same Arc as leader_state map
    if is_first {
        let hlc_clone = state.hlc.lock().map_err(|_| "HLC lock poisoned".to_string())?.clone();
        let handler = Arc::new(MultiSpaceLeaderHandler {
            leaders: state.leader_state.clone(),
            db: DbConnection(state.db.0.clone()),
            hlc: Arc::new(std::sync::Mutex::new(hlc_clone)),
            app_handle: app,
        });
        let endpoint = state.peer_storage.lock().await;
        endpoint.set_delivery_handler(handler).await;
    }

    eprintln!("[SpaceDelivery] Started leader mode for space {space_id}");
    Ok(())
}

/// Broadcast an MLS commit via the local leader buffer.
/// Called by frontend after mls_remove_member (or other commit-producing operations).
#[tauri::command]
pub async fn local_delivery_broadcast_commit(
    state: State<'_, AppState>,
    space_id: String,
    commit: Vec<u8>,
) -> Result<(), String> {
    let leader_state = get_leader_state(&state, &space_id).await?;

    // Store commit in buffer
    let msg_id = super::buffer::store_message(
        &leader_state.db, &space_id, "leader", "commit", &commit,
    )
    .map_err(|e| format!("Failed to store commit: {e}"))?;

    // Track pending ACKs from all space members (not just connected peers)
    let expected_dids: Vec<String> = super::buffer::get_space_member_dids(&leader_state.db, &space_id)
        .unwrap_or_default();

    if !expected_dids.is_empty() {
        let _ = super::buffer::store_pending_commit(&leader_state.db, &space_id, msg_id, &expected_dids);
    }

    // Broadcast notification to all connected peers
    let senders = leader_state.notification_senders.read().await;
    for (_, sender) in senders.iter() {
        let _ = sender.try_send(super::protocol::Notification::Mls {
            space_id: space_id.clone(),
            message_type: "commit".to_string(),
        });
    }

    eprintln!("[SpaceDelivery] Broadcast commit for space {space_id} (msg_id={msg_id}, expected_acks={})", expected_dids.len());
    Ok(())
}

/// Stop leader mode for a space — clears buffers and removes from leader map.
/// The MultiSpaceLeaderHandler stays registered (handles PushInvite even with empty map).
#[tauri::command]
pub async fn local_delivery_stop(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String> {
    super::buffer::clear_buffers(&DbConnection(state.db.0.clone()), &space_id)
        .map_err(|e| e.to_string())?;

    state.leader_state.write().await.remove(&space_id);

    eprintln!("[SpaceDelivery] Stopped leader mode for space {space_id}");
    Ok(())
}

/// Get the current delivery status.
#[tauri::command]
pub async fn local_delivery_status(state: State<'_, AppState>) -> Result<DeliveryStatus, String> {
    let leaders = state.leader_state.read().await;

    Ok(DeliveryStatus {
        is_leader: !leaders.is_empty(),
        active_spaces: leaders.keys().cloned().collect(),
        connected_peers: vec![],
        buffered_messages: 0,
        buffered_welcomes: 0,
        buffered_key_packages: 0,
    })
}

/// Connect to a local space leader and start autonomous sync.
///
/// The UCAN token is resolved from the local DB (`haex_ucan_tokens` filtered
/// by `(space_id, identity_did)` and non-expired) at connect and again on
/// every reconnect. A freshly delegated token after a previous expiry takes
/// effect without any explicit refresh call from the frontend.
#[tauri::command]
pub async fn local_delivery_connect(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    space_id: String,
    leader_endpoint_id: String,
    leader_relay_url: Option<String>,
    identity_did: String,
) -> Result<(), String> {
    let log = |level: &str, msg: &str| {
        let _ = crate::logging::insert_log(&state, level, "LocalDeliveryConnect", None, msg, None, "rust");
    };
    log("info", &format!(
        "ENTER: space={} leader={} did={}",
        &space_id[..8.min(space_id.len())],
        &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
        &identity_did[..24.min(identity_did.len())],
    ));

    // 1. Check if already connected
    let mut loops = state.local_sync_loops.lock().await;
    if loops.contains_key(&space_id) {
        log("warn", &format!("already connected: space={}", &space_id[..8.min(space_id.len())]));
        return Err(format!("Already connected to space {space_id}"));
    }

    // 2. Get our endpoint info
    let endpoint = state.peer_storage.lock().await;
    if !endpoint.is_running() {
        log("error", "peer endpoint not running");
        return Err("Peer storage endpoint not running".to_string());
    }
    let our_endpoint_id = endpoint.endpoint_id().to_string();
    let iroh_endpoint = endpoint
        .endpoint_ref()
        .ok_or("Endpoint not running")?
        .clone();
    drop(endpoint); // Release lock before starting async work

    // 3. Use our endpoint ID as device_id
    let device_id = our_endpoint_id.clone();

    // 4. Start sync loop
    let db = DbConnection(state.db.0.clone());
    let handle = match super::sync_loop::start_peer_sync_loop(
        db,
        iroh_endpoint,
        leader_endpoint_id.clone(),
        leader_relay_url,
        space_id.clone(),
        identity_did,
        our_endpoint_id,
        device_id,
        app,
    )
    .await
    {
        Ok(h) => h,
        Err(e) => {
            log("error", &format!(
                "start_peer_sync_loop failed: space={} err={}",
                &space_id[..8.min(space_id.len())], e,
            ));
            return Err(e.to_string());
        }
    };

    loops.insert(space_id.clone(), handle);
    log("info", &format!("loop started: space={}", &space_id[..8.min(space_id.len())]));
    eprintln!("[SpaceDelivery] Started sync loop for space {space_id}");
    Ok(())
}

/// Disconnect from a local space leader and stop sync.
#[tauri::command]
pub async fn local_delivery_disconnect(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String> {
    let mut loops = state.local_sync_loops.lock().await;
    if let Some(handle) = loops.remove(&space_id) {
        handle.stop();
        eprintln!("[SpaceDelivery] Stopped sync loop for space {space_id}");
        Ok(())
    } else {
        Err(format!("No active sync loop for space {space_id}"))
    }
}

/// Get the current leader for a local space.
/// When the endpoint is running, probes all devices in parallel.
/// When not running, falls back to DB-only query (no reachability check).
#[tauri::command]
pub async fn local_delivery_get_leader(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<Option<LeaderInfo>, String> {
    let db = DbConnection(state.db.0.clone());
    let endpoint = state.peer_storage.lock().await;

    if !endpoint.is_running() {
        // Endpoint not running — fall back to DB-only (first by priority)
        let candidates = super::discovery::get_space_device_candidates(&db, &space_id)
            .map_err(|e| e.to_string())?;
        return Ok(candidates.first().map(|c| LeaderInfo {
            endpoint_id: c.endpoint_id.clone(),
            priority: c.priority,
            space_id,
        }));
    }

    // Endpoint running — full election with parallel probing
    let own_endpoint_id = endpoint.endpoint_id().to_string();
    let result = super::election::elect_leader(&db, &endpoint, &space_id, &own_endpoint_id)
        .await
        .map_err(|e| e.to_string())?;

    match result {
        super::election::ElectionResult::SelfIsLeader => {
            Ok(Some(LeaderInfo {
                endpoint_id: own_endpoint_id,
                priority: 0,
                space_id,
            }))
        }
        super::election::ElectionResult::RemoteLeader { endpoint_id, priority, .. } => {
            Ok(Some(LeaderInfo { endpoint_id, priority, space_id }))
        }
        super::election::ElectionResult::NoLeaderFound => Ok(None),
    }
}

/// Run leader election for a local space.
/// Probes all devices in parallel, returns who should be leader.
#[tauri::command]
pub async fn local_delivery_elect(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<ElectionResultInfo, String> {
    let db = DbConnection(state.db.0.clone());
    let endpoint = state.peer_storage.lock().await;
    let own_endpoint_id = endpoint.endpoint_id().to_string();

    let result = super::election::elect_leader(&db, &endpoint, &space_id, &own_endpoint_id)
        .await
        .map_err(|e| e.to_string())?;

    match result {
        super::election::ElectionResult::SelfIsLeader => {
            Ok(ElectionResultInfo {
                role: "leader".to_string(),
                leader_endpoint_id: Some(own_endpoint_id),
                leader_priority: None,
                leader_relay_url: None,
            })
        }
        super::election::ElectionResult::RemoteLeader { endpoint_id, relay_url, priority } => {
            Ok(ElectionResultInfo {
                role: "peer".to_string(),
                leader_endpoint_id: Some(endpoint_id),
                leader_priority: Some(priority),
                leader_relay_url: relay_url,
            })
        }
        super::election::ElectionResult::NoLeaderFound => {
            Ok(ElectionResultInfo {
                role: "none".to_string(),
                leader_endpoint_id: None,
                leader_priority: None,
                leader_relay_url: None,
            })
        }
    }
}

// ============================================================================
// Invite management commands
// ============================================================================

/// Helper to get the LeaderState for a specific space.
async fn get_leader_state(state: &AppState, space_id: &str) -> Result<Arc<LeaderState>, String> {
    state
        .leader_state
        .read()
        .await
        .get(space_id)
        .cloned()
        .ok_or_else(|| format!("Leader mode not active for space {space_id}"))
}

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
    let leader_state = get_leader_state(&state, &space_id).await?;

    match target_did {
        Some(did) => {
            // Contact invite: pre-create UCAN since target DID is known
            let admin = super::ucan::load_admin_identity(&leader_state.db, &leader_state.space_id)
                .map_err(|e| e.to_string())?;
            let ucan_token = super::ucan::create_delegated_ucan(
                &admin.did,
                &admin.private_key_base64,
                &did,
                &leader_state.space_id,
                &capability,
                Some(&admin.root_ucan),
                super::ucan::MEMBER_UCAN_EXPIRES_IN_SECONDS,
            )
            .map_err(|e| e.to_string())?;

            invite_tokens::create_contact_invite_token(
                &leader_state.db,
                &leader_state.hlc,
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
    let leader_state = get_leader_state(&state, &space_id).await?;

    let tokens = leader_state.invite_tokens.read().await;
    let infos = tokens
        .iter()
        .filter(|t| t.space_id == space_id)
        .map(|t| LocalInviteInfo {
            id: t.id.clone(),
            target_did: t.target_did.clone(),
            capability: t.capability.clone(),
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
    let leader_state = get_leader_state(&state, &space_id).await?;

    let mut tokens = leader_state.invite_tokens.write().await;
    let len_before = tokens.len();
    tokens.retain(|t| t.id != token_id);

    if tokens.len() == len_before {
        return Err(format!("Invite token {token_id} not found"));
    }

    Ok(())
}

/// Parameters for persisting the UCAN row on the claimant's side. Grouped
/// into a struct so callers can't accidentally swap `inviter_did` and
/// `claimant_did` at the call site — that mistake is exactly the bug this
/// helper was extracted to prevent.
pub(crate) struct PersistClaimedUcan<'a> {
    pub space_id: &'a str,
    pub inviter_did: &'a str,
    pub claimant_did: &'a str,
    pub capability: &'a str,
    pub token: &'a str,
}

/// Persist the UCAN row that represents the delegation `inviter → claimant`
/// for a freshly-claimed local invite. `issuer` is the inviter because the
/// ucan_token is signed by them; storing the claimant there (as an earlier
/// revision did) misrepresents the delegation chain.
pub(crate) fn persist_claimed_ucan(
    db: &DbConnection,
    hlc_guard: &std::sync::MutexGuard<'_, crate::crdt::hlc::HlcService>,
    p: PersistClaimedUcan<'_>,
) -> Result<(), String> {
    let ucan_id = uuid::Uuid::new_v4().to_string();
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    crate::database::core::execute_with_crdt(
        "INSERT INTO haex_ucan_tokens (id, space_id, issuer_did, audience_did, capability, token, issued_at, expires_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
            .to_string(),
        vec![
            serde_json::Value::String(ucan_id),
            serde_json::Value::String(p.space_id.to_string()),
            serde_json::Value::String(p.inviter_did.to_string()),
            serde_json::Value::String(p.claimant_did.to_string()),
            serde_json::Value::String(p.capability.to_string()),
            serde_json::Value::String(p.token.to_string()),
            serde_json::Value::Number(serde_json::Number::from(now_secs)),
            serde_json::Value::Number(serde_json::Number::from(
                now_secs + super::ucan::MEMBER_UCAN_EXPIRES_IN_SECONDS as i64,
            )),
        ],
        db,
        hlc_guard,
    )
    .map_err(|e| format!("Failed to persist UCAN: {e}"))?;
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

    log("info", &format!("ENTER local_delivery_claim_invite space={} token={} inviter_did={}", &space_id[..8.min(space_id.len())], &token_id[..8.min(token_id.len())], &inviter_did[..20.min(inviter_did.len())]));
    log("info", &format!("Starting claim: leader={} space={} token={}", &leader_endpoint_id[..16.min(leader_endpoint_id.len())], &space_id[..8.min(space_id.len())], &token_id[..8.min(token_id.len())]));

    // 1. Get iroh endpoint
    let endpoint = state.peer_storage.lock().await;
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

    // 2. Generate MLS KeyPackages
    let key_packages_raw = crate::mls::blocking::generate_key_packages(state.db.0.clone(), 10)
        .await
        .map_err(|e| {
            log("error", &format!("MLS KeyPackage generation failed: {e}"));
            format!("Failed to generate key packages: {e}")
        })?;
    let key_packages_b64: Vec<String> = key_packages_raw.iter().map(|p| BASE64.encode(p)).collect();
    log("info", &format!("Generated {} MLS KeyPackages", key_packages_b64.len()));

    // 3. Connect to leader via QUIC and send ClaimInvite
    let (addr, relay) = super::quic_retry::build_endpoint_addr_with_relay(
        &iroh_endpoint,
        &leader_endpoint_id,
        leader_relay_url.as_deref(),
        configured_relay.as_ref(),
    )
    .map_err(|e| format!("Invalid leader endpoint ID: {e}"))?;

    log("info", &format!("Connecting to {} via relay {:?}", &leader_endpoint_id[..16.min(leader_endpoint_id.len())], relay.as_ref().map(|u| u.to_string())));

    // Encode once outside the retry loop — the request bytes are identical
    // across attempts, including the (expensively-generated) KeyPackages.
    let req = Request::ClaimInvite {
        space_id: space_id.clone(),
        token: token_id.clone(),
        did: identity_did.clone(),
        endpoint_id: our_endpoint_id,
        key_packages: key_packages_b64,
        label,
        public_key: identity_public_key,
    };
    let bytes = super::protocol::encode(&req)
        .map_err(|e| format!("Failed to encode request: {e}"))?;

    // QUIC connect + send + read with automatic retry on transient failures.
    let response = super::quic_retry::send_request_with_retry(
        "ClaimInvite",
        &iroh_endpoint,
        addr,
        &bytes,
    )
    .await
    .map_err(|e| {
        log("error", &format!("QUIC send failed: {e}"));
        format!("{e}")
    })?;

    // 4. Process response
    let (welcome_b64, ucan_token, capability) = match response {
        Response::InviteClaimed {
            welcome,
            ucan,
            capability,
        } => {
            log("info", &format!("Invite claimed successfully, capability={capability}"));
            (welcome, ucan, capability)
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
    let hlc_guard = state.hlc.lock().map_err(|_| "HLC lock poisoned".to_string())?;
    eprintln!("[ClaimInvite] [trace] AFTER hlc.lock() — guard acquired");

    // owner_identity_id must reference the *inviter's* identity row — the
    // space was created by them, not by us. The UI ensures a row for
    // `inviter_did` exists on the claimant's device before invoking this
    // command (see stores/spaces/invites.ts: ensureIdentityForDidAsync).
    eprintln!("[ClaimInvite] [trace] BEFORE resolve_owner_identity_id");
    let owner_identity_id = resolve_owner_identity_id(&inviter_did, &db)?;
    eprintln!("[ClaimInvite] [trace] AFTER resolve_owner_identity_id → owner_id={}", &owner_identity_id[..8.min(owner_identity_id.len())]);

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
    )
    .map_err(|e| format!("Failed to persist space: {e}"))?;
    eprintln!("[ClaimInvite] [trace] AFTER execute_with_crdt INSERT haex_spaces");

    // 7. Persist UCAN token
    eprintln!("[ClaimInvite] [trace] BEFORE persist_claimed_ucan");
    persist_claimed_ucan(
        &db,
        &hlc_guard,
        PersistClaimedUcan {
            space_id: &space_id,
            inviter_did: &inviter_did,
            claimant_did: &identity_did,
            capability: &capability,
            token: &ucan_token,
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
    )
    .map_err(|e| format!("Failed to mark invite as accepted: {e}"))?;
    eprintln!("[ClaimInvite] [trace] AFTER mark accepted — returning Ok");

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
    ) {
        eprintln!(
            "[ClaimInvite] [warn] sibling pending-invite cleanup failed for space={} token={}: {e}",
            &space_id[..8.min(space_id.len())],
            &token_id[..8.min(token_id.len())],
        );
    }

    Ok(ClaimInviteResult {
        space_id,
        capability,
    })
}

/// Push an invite directly to a peer's device via QUIC.
/// The peer creates a dummy space + pending invite locally.
#[tauri::command]
pub async fn local_delivery_push_invite(
    state: State<'_, AppState>,
    target_endpoint_id: String,
    space_id: String,
    space_name: String,
    space_type: String,
    token_id: String,
    capabilities: Vec<String>,
    include_history: bool,
    inviter_did: String,
    inviter_label: Option<String>,
    inviter_avatar: Option<String>,
    inviter_avatar_options: Option<String>,
    space_endpoints: Vec<String>,
    origin_url: Option<String>,
    expires_at: String,
) -> Result<bool, String> {
    let log = |level: &str, msg: &str| {
        let _ = crate::logging::insert_log(&state, level, "PushInvite-Send", None, msg, None, "rust");
    };

    log("info", &format!("Sending → target={} space={} token={}", &target_endpoint_id[..16.min(target_endpoint_id.len())], &space_id[..8.min(space_id.len())], &token_id[..8.min(token_id.len())]));

    let endpoint = state.peer_storage.lock().await;
    if !endpoint.is_running() {
        log("error", "ABORT: peer endpoint not running");
        return Err("Peer endpoint not running".to_string());
    }
    let iroh_endpoint = endpoint
        .endpoint_ref()
        .ok_or("Endpoint not running")?
        .clone();
    let configured_relay = endpoint.configured_relay_url().cloned();
    drop(endpoint);

    // PushInvite has no per-request relay payload — fall back through
    // configured → live relay only.
    let (addr, relay) = super::quic_retry::build_endpoint_addr_with_relay(
        &iroh_endpoint,
        &target_endpoint_id,
        None,
        configured_relay.as_ref(),
    )
    .map_err(|e| format!("Invalid endpoint ID: {e}"))?;
    match &relay {
        Some(url) => log("info", &format!("Connecting via relay: {url}")),
        None => log("warn", "Connecting without relay (mDNS only)"),
    }
    log("info", &format!("Connecting to {target_endpoint_id} (relay={})", relay.is_some()));

    let request = super::protocol::Request::PushInvite {
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
        expires_at,
    };

    let bytes = super::protocol::encode(&request)
        .map_err(|e| format!("Encode error: {e}"))?;

    // QUIC connect + send + read with automatic retry on transient failures.
    let response = super::quic_retry::send_request_with_retry(
        "PushInvite-Send",
        &iroh_endpoint,
        addr,
        &bytes,
    )
    .await
    .map_err(|e| {
        log("error", &format!("QUIC send failed: {e}"));
        format!("{e}")
    })?;

    match &response {
        super::protocol::Response::PushInviteAck { accepted } => {
            log("info", &format!("Response: accepted={accepted}"));
            Ok(*accepted)
        }
        super::protocol::Response::Error { message } => {
            log("error", &format!("Response: remote error={message}"));
            Err(format!("Remote error: {message}"))
        }
        _ => {
            log("error", "Response: unexpected variant");
            Err("Unexpected response".to_string())
        }
    }
}
