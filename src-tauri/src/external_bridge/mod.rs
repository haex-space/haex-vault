//! External Bridge Module
//!
//! Provides WebSocket server for external applications (browser extensions,
//! CLI tools, servers, etc.) to communicate with haex-vault extensions.

mod authorization;
mod crypto;
mod error;
// Types are exercised by the in-module tests today and will be wired into
// the dispatcher in Task 1A.2; suppress the transient dead-code warnings
// until then.
#[allow(dead_code)]
mod mcp;
mod protocol;
mod server;
#[cfg(test)]
mod tests;

pub use authorization::{AuthorizedClient, BlockedClient, PendingAuthorization};
pub use server::{ExternalBridge, SessionAuthorization, SessionBlockedClient, DEFAULT_BRIDGE_PORT};

/// Sentinel `extension_public_key` (and `extension_id`) used by external clients
/// to address the haex-vault core itself instead of a specific extension.
/// A phantom row with this id exists in `haex_extensions` (see migration 0007).
pub const CORE_EXTENSION_ID: &str = "__core__";
/// Sentinel `extension_name` paired with `CORE_EXTENSION_ID` for core requests.
pub const CORE_EXTENSION_NAME: &str = "core";

use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, ExtensionApiAction, ExtensionPermission, PermissionStatus, Principal, ResourceType,
};
use crate::table_names::TABLE_EXTENSIONS;
use crate::AppState;
use authorization::{
    parse_authorized_client, parse_blocked_client, SQL_DELETE_BLOCKED_CLIENT, SQL_DELETE_CLIENT,
    SQL_GET_ALL_BLOCKED_CLIENTS, SQL_GET_ALL_CLIENTS, SQL_GET_CLIENT_EXTENSION_ROW_ID,
    SQL_INSERT_BLOCKED_CLIENT, SQL_INSERT_CLIENT, SQL_UPDATE_CLIENT_GRANT,
    SQL_UPDATE_CLIENT_REQUESTED_PERMISSIONS,
};
use protocol::canonical_requested_permissions;
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Manager, State};

/// Reverse-lookup of an installed extension's `(public_key, name)` by its
/// internal id — used to match a granted `extension_id` against the
/// client's declared `RequestedExtension` entries (which are keyed by
/// public_key + name, not the internal id).
async fn get_extension_public_key_and_name(
    app_handle: &AppHandle,
    extension_id: &str,
) -> Option<(String, String)> {
    let state = app_handle.state::<AppState>();
    let sql = format!("SELECT public_key, name FROM {TABLE_EXTENSIONS} WHERE id = ?1");
    let rows = select_with_crdt(
        sql,
        vec![JsonValue::String(extension_id.to_string())],
        &state.db,
    )
    .ok()?;
    let row = rows.first()?;
    Some((
        row.first()?.as_str()?.to_string(),
        row.get(1)?.as_str()?.to_string(),
    ))
}

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
pub fn external_bridge_get_authorized_clients(
    state: State<'_, AppState>,
) -> Result<Vec<AuthorizedClient>, String> {
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
pub async fn external_bridge_get_session_authorizations(
    state: State<'_, AppState>,
) -> Result<Vec<SessionAuthorization>, String> {
    let bridge = state.external_bridge.lock().await;
    let session_auths = bridge.get_session_authorizations();
    let auths = session_auths.read().await;
    Ok(auths.values().cloned().collect())
}

/// Revoke a session authorization (for "allow once")
#[tauri::command]
pub async fn external_bridge_revoke_session_authorization(
    client_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bridge = state.external_bridge.lock().await;
    let session_auths = bridge.get_session_authorizations();
    let mut auths = session_auths.write().await;
    // A client may hold one entry per granted target — drop them all.
    auths.retain(|_, sa| sa.client_id != client_id);
    println!(
        "[ExternalAuth] Session authorization revoked for client: {}",
        client_id
    );
    Ok(())
}

/// Get all session-blocked clients (for "deny once" - not stored in database)
#[tauri::command]
pub async fn external_bridge_get_session_blocked_clients(
    state: State<'_, AppState>,
) -> Result<Vec<SessionBlockedClient>, String> {
    let bridge = state.external_bridge.lock().await;
    Ok(bridge.get_session_blocked_clients().await)
}

/// Unblock a session-blocked client (for "deny once")
#[tauri::command]
pub async fn external_bridge_unblock_session_client(
    client_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bridge = state.external_bridge.lock().await;
    bridge.remove_session_blocked(&client_id).await;
    println!(
        "[ExternalAuth] Session block removed for client: {}",
        client_id
    );
    Ok(())
}

/// Revoke authorization for an external client (soft delete via CRDT)
///
/// `SQL_DELETE_CLIENT` deletes ALL of this client's authorized-extension rows
/// in one shot (it filters on `client_id` alone) — there is no notion of
/// revoking just one extension for a client, so every revoke also drops the
/// client's ENTIRE `haex_principal_permissions` set (core + every
/// extension's `ExtensionApi` rows) and its session permissions.
#[tauri::command]
pub async fn external_bridge_revoke_client(
    app_handle: AppHandle,
    client_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    {
        let hlc_guard = state
            .hlc
            .lock()
            .map_err(|e| format!("Failed to lock HLC: {}", e))?;

        let params = vec![JsonValue::String(client_id.clone())];

        execute_with_crdt(
            SQL_DELETE_CLIENT.to_string(),
            params,
            &state.db,
            &hlc_guard,
            &state.column_sig_key_cache,
        )
        .map_err(|e| e.to_string())?;
    }

    PermissionManager::delete_permissions(&state, &client_id)
        .await
        .map_err(|e| e.to_string())?;
    state.session_permissions.clear_extension(&client_id);

    // Emit event to notify frontend
    crate::crdt::notify_dirty_tables_changed(&app_handle);

    Ok(())
}

/// Deny a pending external client authorization request
#[tauri::command]
pub async fn external_bridge_deny_client(
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
pub async fn external_bridge_get_pending_authorizations(
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
pub async fn external_bridge_respond(
    request_id: String,
    success: bool,
    data: Option<JsonValue>,
    error: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bridge = state.external_bridge.lock().await;
    let pending_responses = bridge.get_pending_responses();

    // Build the response JSON
    let response = serde_json::json!({
        "requestId": request_id,
        "success": success,
        "data": data,
        "error": error,
    });

    // Get and remove the sender for this request
    let sender = {
        let mut pending = pending_responses.write().await;
        pending.remove(&request_id)
    };

    match sender {
        Some(tx) => {
            // Send response through the oneshot channel
            tx.send(response)
                .map_err(|_| "Failed to send response: receiver dropped".to_string())
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
///
/// The user may approve several targets at once (core + one or more
/// extensions); `external-auth.vue` sends the whole selection as
/// `extensionIds`. All targets are granted in THIS single call so the
/// pending authorization — which holds the client's declared manifest and is
/// cleared once authorization is granted — stays available while every
/// target's permission rows are built. Granting one target per call used to
/// drop the manifest after the first, leaving later targets with empty rows.
#[tauri::command]
pub async fn external_bridge_client_allow(
    app_handle: AppHandle,
    client_id: String,
    client_name: String,
    public_key: String,
    extension_ids: Vec<String>,
    remember: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let pending = {
        let bridge = state.external_bridge.lock().await;
        bridge.get_pending_authorization(&client_id).await
    };

    let declared_permissions = pending.as_ref().and_then(|p| p.permissions.clone());
    let empty_requested_extensions = Vec::new();
    let declared_extensions = pending
        .as_ref()
        .map(|p| &p.requested_extensions)
        .unwrap_or(&empty_requested_extensions);
    let canonical = canonical_requested_permissions(&declared_permissions, declared_extensions);

    // Grant every selected target while the pending manifest is still present.
    for extension_id in &extension_ids {
        // Declared permissions scoped to THIS target: core permissions for
        // the CORE_EXTENSION_ID target, or this one extension's declared
        // actions otherwise. Undeclared resources get no rows at all — they
        // fall through to a runtime prompt (Entscheidung 2 of the
        // permission-parity plan), they are never silently granted.
        let is_core = extension_id.as_str() == CORE_EXTENSION_ID;
        let mut internal_permissions: Vec<ExtensionPermission> = Vec::new();
        let mut cleared_extension_prefix: Option<String> = None;

        if let Some(pending) = &pending {
            if is_core {
                if let Some(client_permissions) = &pending.permissions {
                    let mut core = client_permissions.core.clone();
                    core.set_all_granted();
                    internal_permissions.extend(core.to_internal_permissions(&client_id));
                }
            } else if let Some((pk, name)) =
                get_extension_public_key_and_name(&app_handle, extension_id).await
            {
                let prefix = format!("{pk}::{name}::");
                if let Some(req_ext) = pending
                    .requested_extensions
                    .iter()
                    .find(|e| e.extension_public_key == pk && e.name == name)
                {
                    for action in &req_ext.actions {
                        internal_permissions.push(ExtensionPermission {
                            id: uuid::Uuid::new_v4().to_string(),
                            principal_id: client_id.clone(),
                            resource_type: ResourceType::ExtensionApi,
                            action: Action::ExtensionApi(ExtensionApiAction::Call),
                            target: format!("{prefix}{action}"),
                            constraints: None,
                            status: PermissionStatus::Granted,
                            raw_constraints: None,
                        });
                    }
                }
                cleared_extension_prefix = Some(prefix);
            }
        }

        if remember {
            // Upsert into database via CRDT for permanent authorization. A
            // re-grant (e.g. after a manifest change forced re-authorization)
            // already has a (client_id, extension_id) row — a plain INSERT
            // would violate the unique index and fail the whole grant.
            let existing_row_id = select_with_crdt(
                SQL_GET_CLIENT_EXTENSION_ROW_ID.to_string(),
                vec![
                    JsonValue::String(client_id.clone()),
                    JsonValue::String(extension_id.clone()),
                ],
                &state.db,
            )
            .map_err(|e| e.to_string())?
            .first()
            .and_then(|row| row.first())
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

            {
                let hlc_guard = state
                    .hlc
                    .lock()
                    .map_err(|e| format!("Failed to lock HLC: {}", e))?;

                if let Some(row_id) = existing_row_id {
                    let params = vec![
                        JsonValue::String(row_id),
                        JsonValue::String(client_name.clone()),
                        JsonValue::String(public_key.clone()),
                    ];
                    execute_with_crdt(
                        SQL_UPDATE_CLIENT_GRANT.to_string(),
                        params,
                        &state.db,
                        &hlc_guard,
                        &state.column_sig_key_cache,
                    )
                    .map_err(|e| e.to_string())?;
                } else {
                    let row_id = uuid::Uuid::new_v4().to_string();
                    let params = vec![
                        JsonValue::String(row_id),
                        JsonValue::String(client_id.clone()),
                        JsonValue::String(client_name.clone()),
                        JsonValue::String(public_key.clone()),
                        JsonValue::String(extension_id.clone()),
                        JsonValue::String(canonical.clone()),
                    ];
                    execute_with_crdt(
                        SQL_INSERT_CLIENT.to_string(),
                        params,
                        &state.db,
                        &hlc_guard,
                        &state.column_sig_key_cache,
                    )
                    .map_err(|e| e.to_string())?;
                }

                // Align the stored manifest on ALL of this client's rows with
                // the declaration the user just approved (see
                // SQL_UPDATE_CLIENT_REQUESTED_PERMISSIONS for why).
                execute_with_crdt(
                    SQL_UPDATE_CLIENT_REQUESTED_PERMISSIONS.to_string(),
                    vec![
                        JsonValue::String(canonical.clone()),
                        JsonValue::String(client_id.clone()),
                    ],
                    &state.db,
                    &hlc_guard,
                    &state.column_sig_key_cache,
                )
                .map_err(|e| e.to_string())?;
            }

            if !internal_permissions.is_empty() || is_core || cleared_extension_prefix.is_some() {
                let principal = Principal::ExternalClient(client_id.clone());
                let mut existing = PermissionManager::get_permissions(&state, &principal)
                    .await
                    .map_err(|e| e.to_string())?;

                // Drop this target's own stale rows (core, or this one
                // extension's ExtensionApi rows) before re-inserting —
                // otherwise a narrower re-grant (fewer declared actions/tags
                // than before) would leave the old, broader rows in place
                // alongside the new ones, and a same-target re-grant would
                // violate the unique index. Rows for OTHER extensions/core are
                // untouched.
                existing.retain(|p| {
                    if is_core {
                        // Core grant replaces the client's ENTIRE core
                        // permission set (whatever the current manifest
                        // declares) — every non-ExtensionApi row is core-domain
                        // and superseded. ExtensionApi rows belong to other,
                        // separately-granted extensions and are untouched.
                        p.resource_type == ResourceType::ExtensionApi
                    } else if let Some(prefix) = &cleared_extension_prefix {
                        !(p.resource_type == ResourceType::ExtensionApi
                            && p.target.starts_with(prefix.as_str()))
                    } else {
                        true
                    }
                });
                existing.extend(internal_permissions);

                PermissionManager::replace_permissions(&state, &client_id, &existing)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        } else {
            // Store session-based authorization (for "allow once")
            // This persists for the lifetime of the haex-vault session
            let bridge = state.external_bridge.lock().await;
            bridge
                .add_session_authorization(
                    &client_id,
                    &client_name,
                    &public_key,
                    extension_id,
                    &canonical,
                )
                .await;
            drop(bridge);

            for permission in internal_permissions {
                state.session_permissions.set_permission(permission);
            }
        }
    }

    // Notify the frontend only after ALL database mutations (client rows +
    // permission rows) for every target are complete, so a UI reload triggered
    // by the event can never observe a half-applied grant.
    if remember {
        crate::crdt::notify_dirty_tables_changed(&app_handle);
    }

    // Notify connected client that authorization was granted, and clear the
    // pending authorization now that every target has been granted.
    let bridge = state.external_bridge.lock().await;
    bridge
        .notify_authorization_granted(&client_id, &extension_ids)
        .await
        .map_err(|e| e.to_string())
}

/// Block an external client
/// If remember is true, the client is permanently blocked in the database.
/// If remember is false, the client is blocked for this session only (cleared when haex-vault restarts).
#[tauri::command]
pub async fn external_bridge_client_block(
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

            execute_with_crdt(
                SQL_INSERT_BLOCKED_CLIENT.to_string(),
                params,
                &state.db,
                &hlc_guard,
                &state.column_sig_key_cache,
            )
            .map_err(|e| e.to_string())?;
        }

        // A permanent block means zero residual access — drop any permission
        // rows the client may already hold (e.g. from a prior authorization
        // now being blocked instead of merely revoked).
        PermissionManager::delete_permissions(&state, &client_id)
            .await
            .map_err(|e| e.to_string())?;

        // Notify the frontend only after both the blocked-client row and the
        // permission-row cleanup are committed, so a UI reload triggered by
        // the event can never observe a half-applied block.
        crate::crdt::notify_dirty_tables_changed(&app_handle);
    }
    // Without `remember`, we only reject this specific request. A session-wide
    // block would silently swallow every subsequent reconnect — bad UX when
    // the user just wanted to dismiss the current prompt.
    state.session_permissions.clear_extension(&client_id);

    // Deny the pending request
    let bridge = state.external_bridge.lock().await;
    bridge
        .deny_pending_request(&client_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get all blocked external clients from database
#[tauri::command]
pub fn external_bridge_get_blocked_clients(
    state: State<'_, AppState>,
) -> Result<Vec<BlockedClient>, String> {
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
pub fn external_bridge_unblock_client(
    app_handle: AppHandle,
    client_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let hlc_guard = state
        .hlc
        .lock()
        .map_err(|e| format!("Failed to lock HLC: {}", e))?;

    let params = vec![JsonValue::String(client_id)];

    execute_with_crdt(
        SQL_DELETE_BLOCKED_CLIENT.to_string(),
        params,
        &state.db,
        &hlc_guard,
        &state.column_sig_key_cache,
    )
    .map_err(|e| e.to_string())?;

    // Emit event to notify frontend
    crate::crdt::notify_dirty_tables_changed(&app_handle);

    Ok(())
}

/// Signal that an extension has completed initialization and is ready to handle requests.
/// This is called by the frontend after an extension has finished its setup (migrations, etc.)
/// and unblocks any waiting `ensure_extension_loaded` calls in the ExternalBridge.
#[tauri::command]
pub async fn extension_signal_ready(
    extension_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bridge = state.external_bridge.lock().await;
    bridge.signal_extension_ready(&extension_id).await;
    Ok(())
}
