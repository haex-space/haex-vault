//! Browser Bridge Module
//!
//! Provides WebSocket server for external applications (browser extensions)
//! to communicate with haex-vault extensions.

mod authorization;
mod crypto;
mod error;
mod protocol;
mod server;
#[cfg(test)]
mod tests;

pub use authorization::{AuthorizedClient, PendingAuthorization};
pub use error::BridgeError;
pub use protocol::{BridgeRequest, BridgeResponse, ClientInfo};
pub use server::BrowserBridge;

use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::AppState;
use authorization::{parse_authorized_client, SQL_DELETE_CLIENT, SQL_GET_ALL_CLIENTS, SQL_INSERT_CLIENT};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Emitter, Manager, State};

/// Start the browser bridge WebSocket server
#[tauri::command]
pub async fn start_browser_bridge(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bridge = state.browser_bridge.lock().await;
    if bridge.is_running() {
        return Ok(());
    }
    drop(bridge);

    let mut bridge = state.browser_bridge.lock().await;
    bridge.start(app).await.map_err(|e| e.to_string())
}

/// Stop the browser bridge WebSocket server
#[tauri::command]
pub async fn stop_browser_bridge(state: State<'_, AppState>) -> Result<(), String> {
    let mut bridge = state.browser_bridge.lock().await;
    bridge.stop().await.map_err(|e| e.to_string())
}

/// Get browser bridge status
#[tauri::command]
pub async fn get_browser_bridge_status(state: State<'_, AppState>) -> Result<bool, String> {
    let bridge = state.browser_bridge.lock().await;
    Ok(bridge.is_running())
}

/// Get all authorized clients from database
#[tauri::command]
pub fn get_authorized_clients(state: State<'_, AppState>) -> Result<Vec<AuthorizedClient>, String> {
    let rows = select_with_crdt(SQL_GET_ALL_CLIENTS.to_string(), vec![], &state.db)
        .map_err(|e| e.to_string())?;

    let clients: Vec<AuthorizedClient> = rows
        .iter()
        .filter_map(|row| parse_authorized_client(row))
        .collect();

    Ok(clients)
}

/// Revoke authorization for a client (soft delete via CRDT)
#[tauri::command]
pub fn revoke_client_authorization(
    app_handle: AppHandle,
    client_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let hlc_guard = state
        .hlc
        .lock()
        .map_err(|e| format!("Failed to lock HLC: {}", e))?;

    let params = vec![JsonValue::String(client_id)];

    execute_with_crdt(SQL_DELETE_CLIENT.to_string(), params, &state.db, &hlc_guard)
        .map_err(|e| e.to_string())?;

    // Emit event to notify frontend
    let _ = app_handle.emit("crdt:dirty-tables-changed", ());

    Ok(())
}

/// Approve a pending authorization request
#[tauri::command]
pub async fn approve_client_authorization(
    app_handle: AppHandle,
    client_id: String,
    client_name: String,
    public_key: String,
    extension_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Insert into database via CRDT (in a block to drop hlc_guard before await)
    {
        let hlc_guard = state
            .hlc
            .lock()
            .map_err(|e| format!("Failed to lock HLC: {}", e))?;

        let row_id = uuid::Uuid::new_v4().to_string();
        let params = vec![
            JsonValue::String(row_id),
            JsonValue::String(client_id.clone()),
            JsonValue::String(client_name),
            JsonValue::String(public_key),
            JsonValue::String(extension_id.clone()),
        ];

        execute_with_crdt(SQL_INSERT_CLIENT.to_string(), params, &state.db, &hlc_guard)
            .map_err(|e| e.to_string())?;
    }

    // Emit event to notify frontend
    let _ = app_handle.emit("crdt:dirty-tables-changed", ());

    // Notify connected client that authorization was granted
    let bridge = state.browser_bridge.lock().await;
    bridge
        .notify_authorization_granted(&client_id, &extension_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Deny a pending authorization request
#[tauri::command]
pub async fn deny_client_authorization(
    client_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bridge = state.browser_bridge.lock().await;
    bridge
        .deny_pending_request(&client_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get pending authorization requests
#[tauri::command]
pub async fn get_pending_authorizations(
    state: State<'_, AppState>,
) -> Result<Vec<PendingAuthorization>, String> {
    let bridge = state.browser_bridge.lock().await;
    Ok(bridge.get_pending_authorizations().await)
}

/// Respond to an external request
///
/// Called by haex-vault extensions (via SDK) to send responses
/// back to external clients (browser extensions, CLI, servers, etc.)
#[tauri::command]
pub async fn respond_to_external_request(
    request_id: String,
    response: JsonValue,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bridge = state.browser_bridge.lock().await;
    let pending_responses = bridge.get_pending_responses();

    // Get and remove the sender for this request
    let sender = {
        let mut pending = pending_responses.write().await;
        pending.remove(&request_id)
    };

    match sender {
        Some(tx) => {
            // Send response through the oneshot channel
            tx.send(response).map_err(|_| "Failed to send response: receiver dropped".to_string())
        }
        None => {
            // No pending request with this ID (may have timed out)
            Err(format!("No pending request found with ID: {}", request_id))
        }
    }
}
