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
#[tauri::command]
pub async fn local_delivery_connect(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    space_id: String,
    leader_endpoint_id: String,
    leader_relay_url: Option<String>,
    identity_did: String,
) -> Result<(), String> {
    // 1. Check if already connected
    let mut loops = state.local_sync_loops.lock().await;
    if loops.contains_key(&space_id) {
        return Err(format!("Already connected to space {space_id}"));
    }

    // 2. Get our endpoint info
    let endpoint = state.peer_storage.lock().await;
    if !endpoint.is_running() {
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
    let handle = super::sync_loop::start_peer_sync_loop(
        db,
        iroh_endpoint,
        leader_endpoint_id,
        leader_relay_url,
        space_id.clone(),
        identity_did,
        our_endpoint_id,
        device_id,
        app,
    )
    .await
    .map_err(|e| e.to_string())?;

    loops.insert(space_id.clone(), handle);
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
                86400 * 365,
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
    let remote_id: iroh::EndpointId = leader_endpoint_id
        .parse()
        .map_err(|e| format!("Invalid leader endpoint ID: {e}"))?;

    // Use explicit relay URL if provided, fall back to configured relay, then live relay
    let relay = leader_relay_url
        .as_deref()
        .and_then(|s| s.parse::<iroh::RelayUrl>().ok())
        .or(configured_relay)
        .or_else(|| iroh_endpoint.addr().relay_urls().next().cloned());

    log("info", &format!("Connecting to {} via relay {:?}", &leader_endpoint_id[..16.min(leader_endpoint_id.len())], relay.as_ref().map(|u| u.to_string())));

    let addr = match relay {
        Some(url) => iroh::EndpointAddr::new(remote_id).with_relay_url(url),
        None => iroh::EndpointAddr::new(remote_id),
    };

    // Encode once outside the retry loop — the request bytes are identical
    // across attempts, including the (expensively-generated) KeyPackages.
    let req = Request::ClaimInvite {
        space_id: space_id.clone(),
        token: token_id,
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
    let hlc_guard = state.hlc.lock().map_err(|_| "HLC lock poisoned".to_string())?;

    crate::database::core::execute_with_crdt(
        "INSERT OR IGNORE INTO haex_spaces (id, type, status, name) VALUES (?1, 'local', 'active', ?2)".to_string(),
        vec![
            serde_json::Value::String(space_id.clone()),
            serde_json::Value::String(space_name),
        ],
        &db,
        &hlc_guard,
    )
    .map_err(|e| format!("Failed to persist space: {e}"))?;

    // 7. Persist UCAN token
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
            serde_json::Value::String(space_id.clone()),
            serde_json::Value::String(identity_did.clone()), // self-issued for local claims
            serde_json::Value::String(identity_did),
            serde_json::Value::String(capability.clone()),
            serde_json::Value::String(ucan_token),
            serde_json::Value::Number(serde_json::Number::from(now_secs)),
            serde_json::Value::Number(serde_json::Number::from(now_secs + 86400 * 365)), // 1 year
        ],
        &db,
        &hlc_guard,
    )
    .map_err(|e| format!("Failed to persist UCAN: {e}"))?;

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

    let remote_id: iroh::EndpointId = target_endpoint_id
        .parse()
        .map_err(|e| format!("Invalid endpoint ID: {e}"))?;

    // Use the configured relay URL (from DB settings / env / default).
    // Falls back to the live relay from endpoint.addr() if available.
    let relay = configured_relay
        .or_else(|| iroh_endpoint.addr().relay_urls().next().cloned());
    let has_relay = relay.is_some();
    let addr = match relay {
        Some(url) => {
            log("info", &format!("Connecting via relay: {url}"));
            iroh::EndpointAddr::new(remote_id).with_relay_url(url)
        }
        None => {
            log("warn", "Connecting without relay (mDNS only)");
            iroh::EndpointAddr::new(remote_id)
        }
    };

    log("info", &format!("Connecting to {remote_id} (relay={has_relay})"));

    let request = super::protocol::Request::PushInvite {
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
