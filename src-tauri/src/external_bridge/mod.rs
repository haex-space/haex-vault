//! External Bridge Module
//!
//! Provides WebSocket server for external applications (browser extensions,
//! CLI tools, servers, etc.) to communicate with haex-vault extensions.

mod authorization;
mod crypto;
mod error;
mod protocol;
mod server;
#[cfg(test)]
mod tests;

pub use authorization::{AuthorizedClient, BlockedClient, PendingAuthorization};
pub use server::{ExternalBridge, SessionAuthorization, DEFAULT_BRIDGE_PORT};

use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::AppState;
use authorization::{
    parse_authorized_client, parse_blocked_client,
    SQL_DELETE_CLIENT, SQL_GET_ALL_CLIENTS, SQL_INSERT_CLIENT,
    SQL_GET_ALL_BLOCKED_CLIENTS, SQL_INSERT_BLOCKED_CLIENT, SQL_DELETE_BLOCKED_CLIENT, SQL_IS_BLOCKED,
};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Emitter, State};

/// Start the external bridge server on a specific port
#[tauri::command]
pub async fn external_bridge_start(
    app: AppHandle,
    port: Option<u16>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bridge = state.external_bridge.lock().await;
    if bridge.is_running() {
        return Ok(());
    }
    drop(bridge);

    let mut bridge = state.external_bridge.lock().await;
    bridge.start(app, port).await.map_err(|e| e.to_string())
}

/// Stop the external bridge server
#[tauri::command]
pub async fn external_bridge_stop(state: State<'_, AppState>) -> Result<(), String> {
    let mut bridge = state.external_bridge.lock().await;
    bridge.stop().await.map_err(|e| e.to_string())
}

/// Get external bridge status
#[tauri::command]
pub async fn external_bridge_get_status(state: State<'_, AppState>) -> Result<bool, String> {
    let bridge = state.external_bridge.lock().await;
    Ok(bridge.is_running())
}

/// Get the current port of the external bridge server
#[tauri::command]
pub async fn external_bridge_get_port(state: State<'_, AppState>) -> Result<u16, String> {
    let bridge = state.external_bridge.lock().await;
    Ok(bridge.get_port())
}

/// Get the default external bridge port
#[tauri::command]
pub fn external_bridge_get_default_port() -> u16 {
    DEFAULT_BRIDGE_PORT
}

/// Get all authorized external clients from database
#[tauri::command]
pub fn external_get_authorized_clients(state: State<'_, AppState>) -> Result<Vec<AuthorizedClient>, String> {
    let rows = select_with_crdt(SQL_GET_ALL_CLIENTS.to_string(), vec![], &state.db)
        .map_err(|e| e.to_string())?;

    let clients: Vec<AuthorizedClient> = rows
        .iter()
        .filter_map(|row| parse_authorized_client(row))
        .collect();

    Ok(clients)
}

/// Get all session-based authorizations (for "allow once" - not stored in database)
#[tauri::command]
pub async fn external_get_session_authorizations(
    state: State<'_, AppState>,
) -> Result<Vec<SessionAuthorization>, String> {
    let bridge = state.external_bridge.lock().await;
    let session_auths = bridge.get_session_authorizations();
    let auths = session_auths.read().await;
    Ok(auths.values().cloned().collect())
}

/// Revoke a session authorization (for "allow once")
#[tauri::command]
pub async fn external_revoke_session_authorization(
    client_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bridge = state.external_bridge.lock().await;
    let session_auths = bridge.get_session_authorizations();
    let mut auths = session_auths.write().await;
    auths.remove(&client_id);
    println!("[ExternalAuth] Session authorization revoked for client: {}", client_id);
    Ok(())
}

/// Revoke authorization for an external client (soft delete via CRDT)
#[tauri::command]
pub fn external_revoke_client(
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

/// Approve a pending external client authorization request
#[tauri::command]
pub async fn external_approve_client(
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
    let bridge = state.external_bridge.lock().await;
    bridge
        .notify_authorization_granted(&client_id, &extension_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Deny a pending external client authorization request
#[tauri::command]
pub async fn external_deny_client(
    client_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bridge = state.external_bridge.lock().await;
    bridge
        .deny_pending_request(&client_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get pending external client authorization requests
#[tauri::command]
pub async fn external_get_pending_authorizations(
    state: State<'_, AppState>,
) -> Result<Vec<PendingAuthorization>, String> {
    let bridge = state.external_bridge.lock().await;
    Ok(bridge.get_pending_authorizations().await)
}

/// Respond to an external request
///
/// Called by haex-vault extensions (via SDK) to send responses
/// back to external clients (browser extensions, CLI, servers, etc.)
#[tauri::command]
pub async fn external_respond(
    request_id: String,
    response: JsonValue,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bridge = state.external_bridge.lock().await;
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

/// Allow an external client access to an extension
/// If remember is true, the authorization is stored permanently in the database.
/// If remember is false, the authorization is stored for this session only (cleared when haex-vault restarts).
#[tauri::command]
pub async fn external_client_allow(
    app_handle: AppHandle,
    client_id: String,
    client_name: String,
    public_key: String,
    extension_id: String,
    remember: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if remember {
        // Insert into database via CRDT for permanent authorization
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
    } else {
        // Store session-based authorization (for "allow once")
        // This persists for the lifetime of the haex-vault session
        let bridge = state.external_bridge.lock().await;
        bridge
            .add_session_authorization(&client_id, &extension_id)
            .await;
    }

    // Notify connected client that authorization was granted
    let bridge = state.external_bridge.lock().await;
    bridge
        .notify_authorization_granted(&client_id, &extension_id)
        .await
        .map_err(|e| e.to_string())
}

/// Block an external client
/// If remember is true, the client is permanently blocked in the database.
/// If remember is false, only this request is denied.
#[tauri::command]
pub async fn external_client_block(
    app_handle: AppHandle,
    client_id: String,
    client_name: String,
    public_key: String,
    remember: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if remember {
        // Insert into blocked clients table via CRDT for permanent block
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
            ];

            execute_with_crdt(SQL_INSERT_BLOCKED_CLIENT.to_string(), params, &state.db, &hlc_guard)
                .map_err(|e| e.to_string())?;
        }

        // Emit event to notify frontend
        let _ = app_handle.emit("crdt:dirty-tables-changed", ());
    }

    // Deny the pending request
    let bridge = state.external_bridge.lock().await;
    bridge
        .deny_pending_request(&client_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get all blocked external clients from database
#[tauri::command]
pub fn external_get_blocked_clients(state: State<'_, AppState>) -> Result<Vec<BlockedClient>, String> {
    let rows = select_with_crdt(SQL_GET_ALL_BLOCKED_CLIENTS.to_string(), vec![], &state.db)
        .map_err(|e| e.to_string())?;

    let clients: Vec<BlockedClient> = rows
        .iter()
        .filter_map(|row| parse_blocked_client(row))
        .collect();

    Ok(clients)
}

/// Unblock an external client (remove from blocked list)
#[tauri::command]
pub fn external_unblock_client(
    app_handle: AppHandle,
    client_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let hlc_guard = state
        .hlc
        .lock()
        .map_err(|e| format!("Failed to lock HLC: {}", e))?;

    let params = vec![JsonValue::String(client_id)];

    execute_with_crdt(SQL_DELETE_BLOCKED_CLIENT.to_string(), params, &state.db, &hlc_guard)
        .map_err(|e| e.to_string())?;

    // Emit event to notify frontend
    let _ = app_handle.emit("crdt:dirty-tables-changed", ());

    Ok(())
}

/// Check if a client is blocked
#[tauri::command]
pub fn external_is_client_blocked(
    client_id: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let rows = select_with_crdt(
        SQL_IS_BLOCKED.to_string(),
        vec![JsonValue::String(client_id)],
        &state.db,
    )
    .map_err(|e| e.to_string())?;

    if let Some(row) = rows.first() {
        if let Some(count) = row.first() {
            return Ok(count.as_i64().unwrap_or(0) > 0);
        }
    }

    Ok(false)
}
