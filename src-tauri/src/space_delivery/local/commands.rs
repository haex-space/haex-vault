//! Tauri commands for the local delivery service.

use std::collections::HashMap;
use std::sync::Arc;

use tauri::State;
use tokio::sync::RwLock;

use crate::database::DbConnection;
use crate::AppState;

use super::leader::{LeaderConnectionHandler, LeaderState};
use super::types::{DeliveryStatus, ElectionResultInfo, LeaderInfo};

/// Start leader mode for a local space.
#[tauri::command]
pub async fn local_delivery_start(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String> {
    let leader_state = Arc::new(LeaderState {
        db: DbConnection(state.db.0.clone()),
        space_id: space_id.clone(),
        connected_peers: Arc::new(RwLock::new(HashMap::new())),
        notification_senders: Arc::new(RwLock::new(HashMap::new())),
    });

    let handler = Arc::new(LeaderConnectionHandler {
        state: leader_state,
    });

    let endpoint = state.peer_storage.lock().await;
    endpoint.set_delivery_handler(handler).await;

    eprintln!("[SpaceDelivery] Started leader mode for space {space_id}");
    Ok(())
}

/// Stop leader mode — clears buffers and unregisters handler.
#[tauri::command]
pub async fn local_delivery_stop(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<(), String> {
    // Clear buffer tables
    super::leader::clear_buffers(&DbConnection(state.db.0.clone()), &space_id)
        .map_err(|e| e.to_string())?;

    // Remove delivery handler
    let endpoint = state.peer_storage.lock().await;
    endpoint.state.write().await.delivery_handler = None;

    eprintln!("[SpaceDelivery] Stopped leader mode for space {space_id}");
    Ok(())
}

/// Get the current delivery status.
#[tauri::command]
pub async fn local_delivery_status(state: State<'_, AppState>) -> Result<DeliveryStatus, String> {
    let endpoint = state.peer_storage.lock().await;
    let peer_state = endpoint.state.read().await;
    let is_leader = peer_state.delivery_handler.is_some();

    Ok(DeliveryStatus {
        is_leader,
        space_id: None,
        connected_peers: vec![],
        buffered_messages: 0,
        buffered_welcomes: 0,
        buffered_key_packages: 0,
    })
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
