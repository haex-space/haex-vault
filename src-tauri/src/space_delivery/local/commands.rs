//! Tauri commands for the local delivery service.

use std::collections::HashMap;
use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use tauri::State;
use tokio::sync::RwLock;

use crate::database::DbConnection;
use crate::AppState;

use super::invite_tokens;
use super::leader::{LeaderConnectionHandler, LeaderState};
use super::protocol::{Request, Response};
use super::types::{ClaimInviteResult, DeliveryStatus, ElectionResultInfo, LeaderInfo, LocalInviteInfo};

/// Start leader mode for a local space.
#[tauri::command]
pub async fn local_delivery_start(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String> {
    // Load existing invite tokens from DB (persisted across restarts)
    let db_conn = DbConnection(state.db.0.clone());
    let existing_tokens = invite_tokens::load_invite_tokens(&db_conn, &space_id)
        .unwrap_or_default();

    // Share the HLC service from AppState — clone is cheap (inner Arc)
    let hlc_clone = state.hlc.lock().map_err(|_| "HLC lock poisoned".to_string())?.clone();

    let leader_state = Arc::new(LeaderState {
        db: db_conn,
        hlc: Arc::new(std::sync::Mutex::new(hlc_clone)),
        app_handle: app,
        space_id: space_id.clone(),
        connected_peers: Arc::new(RwLock::new(HashMap::new())),
        notification_senders: Arc::new(RwLock::new(HashMap::new())),
        invite_tokens: Arc::new(RwLock::new(existing_tokens)),
    });

    // Store leader_state in AppState for invite management commands
    *state.leader_state.lock().await = Some(leader_state.clone());

    let handler = Arc::new(LeaderConnectionHandler {
        state: leader_state,
    });

    let endpoint = state.peer_storage.lock().await;
    endpoint.set_delivery_handler(handler).await;

    eprintln!("[SpaceDelivery] Started leader mode for space {space_id}");
    Ok(())
}

/// Stop leader mode — clears buffers and restores the invite-only handler.
#[tauri::command]
pub async fn local_delivery_stop(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String> {
    // Clear buffer tables
    super::buffer::clear_buffers(&DbConnection(state.db.0.clone()), &space_id)
        .map_err(|e| e.to_string())?;

    // Clear leader_state from AppState
    *state.leader_state.lock().await = None;

    // Restore the lightweight invite receiver handler
    let endpoint = state.peer_storage.lock().await;
    let db_conn = DbConnection(state.db.0.clone());
    let hlc_clone = state.hlc.lock().map_err(|_| "HLC lock poisoned".to_string())?.clone();
    let receiver_state = Arc::new(super::invite_receiver::InviteReceiverState {
        db: db_conn,
        hlc: Arc::new(std::sync::Mutex::new(hlc_clone)),
        app_handle: app,
    });
    let handler = Arc::new(super::invite_receiver::InviteReceiverHandler {
        state: receiver_state,
    });
    endpoint.set_delivery_handler(handler).await;

    eprintln!("[SpaceDelivery] Stopped leader mode for space {space_id}, invite receiver restored");
    Ok(())
}

/// Get the current delivery status.
#[tauri::command]
pub async fn local_delivery_status(state: State<'_, AppState>) -> Result<DeliveryStatus, String> {
    let endpoint = state.peer_storage.lock().await;
    let peer_state = endpoint.state.read().await;
    let is_leader = peer_state.delivery_handler.is_some();
    drop(peer_state);
    drop(endpoint);

    let loops = state.local_sync_loops.lock().await;
    let active_space = loops.keys().next().cloned();

    Ok(DeliveryStatus {
        is_leader,
        space_id: active_space,
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

/// Helper to get the LeaderState from AppState or return an error.
async fn get_leader_state(state: &AppState) -> Result<Arc<LeaderState>, String> {
    state
        .leader_state
        .lock()
        .await
        .clone()
        .ok_or_else(|| "Leader mode not active".to_string())
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
    let leader_state = get_leader_state(&state).await?;

    if leader_state.space_id != space_id {
        return Err(format!(
            "Leader is serving space {}, not {space_id}",
            leader_state.space_id
        ));
    }

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
    let leader_state = get_leader_state(&state).await?;

    if leader_state.space_id != space_id {
        return Err(format!(
            "Leader is serving space {}, not {space_id}",
            leader_state.space_id
        ));
    }

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
    token_id: String,
) -> Result<(), String> {
    let leader_state = get_leader_state(&state).await?;

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
    token_id: String,
    identity_did: String,
    label: Option<String>,
    identity_public_key: Option<String>,
) -> Result<ClaimInviteResult, String> {
    // 1. Get iroh endpoint
    let endpoint = state.peer_storage.lock().await;
    if !endpoint.is_running() {
        return Err("Peer storage endpoint not running".to_string());
    }
    let our_endpoint_id = endpoint.endpoint_id().to_string();
    let iroh_endpoint = endpoint
        .endpoint_ref()
        .ok_or("Endpoint not running")?
        .clone();
    drop(endpoint);

    // 2. Generate MLS KeyPackages
    let mls_manager = crate::mls::manager::MlsManager::new(state.db.0.clone());
    let key_packages_raw = mls_manager
        .generate_key_packages(10)
        .map_err(|e| format!("Failed to generate key packages: {e}"))?;
    let key_packages_b64: Vec<String> = key_packages_raw.iter().map(|p| BASE64.encode(p)).collect();

    // 3. Connect to leader via QUIC and send ClaimInvite
    let remote_id: iroh::EndpointId = leader_endpoint_id
        .parse()
        .map_err(|e| format!("Invalid leader endpoint ID: {e}"))?;

    let relay = leader_relay_url
        .as_deref()
        .and_then(|s| s.parse::<iroh::RelayUrl>().ok());

    let addr = match relay {
        Some(url) => iroh::EndpointAddr::new(remote_id).with_relay_url(url),
        None => iroh::EndpointAddr::new(remote_id),
    };

    let conn = iroh_endpoint
        .connect(addr, super::protocol::ALPN)
        .await
        .map_err(|e| format!("Failed to connect to leader: {e}"))?;

    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("Failed to open stream: {e}"))?;

    let req = Request::ClaimInvite {
        space_id: space_id.clone(),
        token: token_id,
        did: identity_did.clone(),
        endpoint_id: our_endpoint_id,
        key_packages: key_packages_b64,
        label,
        public_key: identity_public_key,
    };

    let bytes = super::protocol::encode(&req).map_err(|e| format!("Failed to encode request: {e}"))?;
    send.write_all(&bytes)
        .await
        .map_err(|e| format!("Failed to send request: {e}"))?;
    send.finish()
        .map_err(|e| format!("Failed to finish send: {e}"))?;

    let response = super::protocol::read_response(&mut recv)
        .await
        .map_err(|e| format!("Failed to read response: {e}"))?;

    conn.close(0u32.into(), b"done");

    // 4. Process response
    let (welcome_b64, ucan_token, capability) = match response {
        Response::InviteClaimed {
            welcome,
            ucan,
            capability,
        } => (welcome, ucan, capability),
        Response::Error { message } => return Err(format!("Leader rejected invite: {message}")),
        _ => return Err("Unexpected response from leader".to_string()),
    };

    // 5. Process MLS welcome
    let welcome_bytes = BASE64
        .decode(&welcome_b64)
        .map_err(|e| format!("Failed to decode welcome: {e}"))?;
    mls_manager
        .process_message(&space_id, &welcome_bytes)
        .map_err(|e| format!("Failed to process MLS welcome: {e}"))?;

    // 6. Persist space locally (type = 'local', status = 'active')
    // Capabilities are derived at runtime from UCAN tokens, not stored on the space
    let db = DbConnection(state.db.0.clone());
    let hlc_guard = state.hlc.lock().map_err(|_| "HLC lock poisoned".to_string())?;

    crate::database::core::execute_with_crdt(
        "INSERT OR IGNORE INTO haex_spaces (id, type, status, name) VALUES (?1, 'local', 'active', ?2)".to_string(),
        vec![
            serde_json::Value::String(space_id.clone()),
            serde_json::Value::String(format!("Local Space {}", &space_id[..8.min(space_id.len())])),
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
    let endpoint = state.peer_storage.lock().await;
    if !endpoint.is_running() {
        return Err("Peer endpoint not running".to_string());
    }
    let iroh_endpoint = endpoint
        .endpoint_ref()
        .ok_or("Endpoint not running")?
        .clone();
    drop(endpoint);

    let remote_id: iroh::EndpointId = target_endpoint_id
        .parse()
        .map_err(|e| format!("Invalid endpoint ID: {e}"))?;

    // iroh discovers relay transparently from EndpointId
    let addr = iroh::EndpointAddr::new(remote_id);

    let conn = iroh_endpoint
        .connect(addr, super::protocol::ALPN)
        .await
        .map_err(|e| format!("Failed to connect: {e}"))?;

    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("Failed to open stream: {e}"))?;

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
    send.write_all(&bytes)
        .await
        .map_err(|e| format!("Send error: {e}"))?;
    send.finish()
        .map_err(|e| format!("Finish error: {e}"))?;

    let response = super::protocol::read_response(&mut recv)
        .await
        .map_err(|e| format!("Read response error: {e}"))?;

    conn.close(0u32.into(), b"done");

    match response {
        super::protocol::Response::PushInviteAck { accepted } => Ok(accepted),
        super::protocol::Response::Error { message } => Err(format!("Remote error: {message}")),
        _ => Err("Unexpected response".to_string()),
    }
}
