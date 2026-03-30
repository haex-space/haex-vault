//! Tauri commands for the local delivery service.

use std::collections::HashMap;
use std::sync::Arc;

use tauri::State;
use tokio::sync::RwLock;

use crate::database::DbConnection;
use crate::AppState;

use super::leader::{LeaderConnectionHandler, LeaderState};
use super::types::{DeliveryStatus, LeaderInfo};

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

/// Get the current leader for a local space (using CRDT priorities).
#[tauri::command]
pub async fn local_delivery_get_leader(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<Option<LeaderInfo>, String> {
    // Query haex_space_devices for this space, ordered by leader_priority ASC
    let sql = "SELECT device_endpoint_id, leader_priority FROM haex_space_devices \
               WHERE space_id = ?1 ORDER BY leader_priority ASC, device_endpoint_id ASC LIMIT 1"
        .to_string();
    let params = vec![serde_json::Value::String(space_id.clone())];

    let rows = crate::database::core::select_with_crdt(sql, params, &state.db)
        .map_err(|e| format!("Failed to query space devices: {e}"))?;

    if let Some(row) = rows.first() {
        let endpoint_id = row
            .get(0)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let priority = row.get(1).and_then(|v| v.as_i64()).unwrap_or(10) as i32;
        Ok(Some(LeaderInfo {
            endpoint_id,
            priority,
            space_id,
        }))
    } else {
        Ok(None)
    }
}
