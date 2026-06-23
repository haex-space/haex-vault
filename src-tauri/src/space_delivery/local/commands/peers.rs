//! Peer connection commands: connect, disconnect, force-sync, get-leader, elect.

use std::sync::Arc;

use tauri::State;

use crate::database::DbConnection;
use crate::AppState;

use super::super::leader::LeaderState;
use super::super::types::{ElectionResultInfo, LeaderInfo};

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
        let _ = crate::logging::insert_log(
            &state,
            level,
            "LocalDeliveryConnect",
            None,
            msg,
            None,
            "rust",
        );
    };
    log(
        "info",
        &format!(
            "ENTER: space={} leader={} did={}",
            &space_id[..8.min(space_id.len())],
            &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
            &identity_did[..24.min(identity_did.len())],
        ),
    );

    // 1. Check if already connected
    let mut loops = state.local_sync_loops.lock().await;
    if loops.contains_key(&space_id) {
        log(
            "warn",
            &format!(
                "already connected: space={}",
                &space_id[..8.min(space_id.len())]
            ),
        );
        return Err(format!("Already connected to space {space_id}"));
    }

    // 2. Get our endpoint info
    let endpoint = state.peer_storage.read().await;
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

    // 3. Resolve the HLC device UUID. This is the same UUID embedded in
    // every row's HLC timestamp (via HlcService::get_or_create_device_id),
    // so the push-scanner origin filter can correctly distinguish locally-
    // authored rows from pulled rows. The iroh endpoint ID is a 256-bit hex
    // key (not a UUID) and cannot serve this role.
    let device_id = crate::crdt::hlc::HlcService::get_or_create_device_id(&app)
        .map_err(|e| format!("Failed to read device UUID: {e}"))?;

    // 4. Start sync loop
    let db = DbConnection(state.db.0.clone());
    let handle = match super::super::sync_loop::start_peer_sync_loop(
        db,
        iroh_endpoint,
        // Existing shared-space sync: keep the space-scoped behaviour unchanged.
        super::super::sync_loop::SyncMode::SpaceScoped,
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
            log(
                "error",
                &format!(
                    "start_peer_sync_loop failed: space={} err={}",
                    &space_id[..8.min(space_id.len())],
                    e,
                ),
            );
            return Err(e.to_string());
        }
    };

    loops.insert(space_id.clone(), handle);
    log(
        "info",
        &format!("loop started: space={}", &space_id[..8.min(space_id.len())]),
    );
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

/// Cut the current `POLL_INTERVAL` sleep short for a running sync loop so
/// the next sync cycle starts immediately. Used by the e2e tests to make
/// post-accept device-row propagation deterministic instead of waiting up
/// to 5s for the next poll tick.
///
/// No-op (returns Ok) when no loop is running for the space — the caller
/// has no useful action to take in that case, and the natural retry path
/// (start a loop, or wait for one to be started elsewhere) is unchanged.
///
/// Cannot be abused for DDoS: the cycle itself is serial, so multiple
/// wake-ups coalesce — worst case the effective poll interval drops to
/// "as fast as one cycle completes", which is the same load shape as a
/// shorter `POLL_INTERVAL` constant.
#[tauri::command]
pub async fn local_delivery_force_sync(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String> {
    let loops = state.local_sync_loops.lock().await;
    if let Some(handle) = loops.get(&space_id) {
        handle.wakeup();
    }
    Ok(())
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
    // Extract the iroh endpoint handle under a brief read lock so the
    // parallel peer probing inside elect_leader runs without holding it.
    let (own_endpoint_id, iroh_endpoint) = {
        let endpoint = state.peer_storage.read().await;
        if !endpoint.is_running() {
            // Endpoint not running — fall back to DB-only (first by priority)
            let candidates = super::super::discovery::get_space_device_candidates(&db, &space_id)
                .map_err(|e| e.to_string())?;
            return Ok(candidates.first().map(|c| LeaderInfo {
                endpoint_id: c.endpoint_id.clone(),
                priority: c.priority,
                space_id,
            }));
        }
        (
            endpoint.endpoint_id().to_string(),
            endpoint.endpoint_ref().cloned(),
        )
    };

    let result =
        super::super::election::elect_leader(&db, iroh_endpoint, &space_id, &own_endpoint_id)
            .await
            .map_err(|e| e.to_string())?;

    match result {
        super::super::election::ElectionResult::SelfIsLeader => Ok(Some(LeaderInfo {
            endpoint_id: own_endpoint_id,
            priority: 0,
            space_id,
        })),
        super::super::election::ElectionResult::RemoteLeader {
            endpoint_id,
            priority,
            ..
        } => Ok(Some(LeaderInfo {
            endpoint_id,
            priority,
            space_id,
        })),
        super::super::election::ElectionResult::NoLeaderFound => Ok(None),
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
    let (own_endpoint_id, iroh_endpoint) = {
        let endpoint = state.peer_storage.read().await;
        (
            endpoint.endpoint_id().to_string(),
            endpoint.endpoint_ref().cloned(),
        )
    };

    let result =
        super::super::election::elect_leader(&db, iroh_endpoint, &space_id, &own_endpoint_id)
            .await
            .map_err(|e| e.to_string())?;

    match result {
        super::super::election::ElectionResult::SelfIsLeader => Ok(ElectionResultInfo {
            role: "leader".to_string(),
            leader_endpoint_id: Some(own_endpoint_id),
            leader_priority: None,
            leader_relay_url: None,
        }),
        super::super::election::ElectionResult::RemoteLeader {
            endpoint_id,
            relay_url,
            priority,
        } => Ok(ElectionResultInfo {
            role: "peer".to_string(),
            leader_endpoint_id: Some(endpoint_id),
            leader_priority: Some(priority),
            leader_relay_url: relay_url,
        }),
        super::super::election::ElectionResult::NoLeaderFound => Ok(ElectionResultInfo {
            role: "none".to_string(),
            leader_endpoint_id: None,
            leader_priority: None,
            leader_relay_url: None,
        }),
    }
}

/// Helper to get the LeaderState for a specific space.
pub(super) async fn get_leader_state(
    state: &AppState,
    space_id: &str,
) -> Result<Arc<LeaderState>, String> {
    state
        .leader_state
        .read()
        .await
        .get(space_id)
        .cloned()
        .ok_or_else(|| format!("Leader mode not active for space {space_id}"))
}
