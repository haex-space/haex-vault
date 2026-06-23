//! Authorization helpers — DB lookups, blocking checks, and extension
//! preflight (auto-start) used by `connection` and `process`.

use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::event_names::EVENT_EXTENSION_AUTO_START_REQUEST;
use crate::AppState;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::external_bridge::authorization::{
    SQL_GET_CLIENT_EXTENSION, SQL_GET_EXTENSION_ID_BY_PUBLIC_KEY_AND_NAME, SQL_IS_BLOCKED,
    SQL_IS_CLIENT_AUTHORIZED_FOR_EXTENSION, SQL_IS_CLIENT_KNOWN, SQL_UPDATE_LAST_SEEN,
};
use crate::external_bridge::error::BridgeError;

/// Check if a client is authorized (via CRDT database query)
pub(super) async fn check_client_authorized(app_handle: &AppHandle, client_id: &str) -> bool {
    let state = app_handle.state::<AppState>();
    let params = vec![JsonValue::String(client_id.to_string())];

    match select_with_crdt(SQL_IS_CLIENT_KNOWN.to_string(), params, &state.db) {
        Ok(rows) => {
            if let Some(row) = rows.first() {
                if let Some(count) = row.first() {
                    return count.as_i64().unwrap_or(0) > 0;
                }
            }
            false
        }
        Err(_) => false,
    }
}

/// Check if a client is blocked (via CRDT database query).
///
/// **Fail closed**: when the DB query errors out (e.g. the database is mid-
/// migration, the connection is locked, the disk is read-only), report the
/// client as *blocked* so the handshake is rejected. The symmetric
/// `check_client_authorized` is also conservative on error — denying — but
/// the blocked check has the opposite polarity: returning `false` on error
/// here would let a known-blocked client through during any transient DB
/// outage.
pub(super) async fn check_client_blocked(app_handle: &AppHandle, client_id: &str) -> bool {
    let state = app_handle.state::<AppState>();
    let params = vec![JsonValue::String(client_id.to_string())];

    match select_with_crdt(SQL_IS_BLOCKED.to_string(), params, &state.db) {
        Ok(rows) => {
            if let Some(row) = rows.first() {
                if let Some(count) = row.first() {
                    return count.as_i64().unwrap_or(0) > 0;
                }
            }
            false
        }
        Err(e) => {
            eprintln!(
                "[ExternalBridge] check_client_blocked DB error for client {}: {} \
                 — treating as blocked (fail-closed)",
                client_id, e
            );
            true
        }
    }
}

/// Get the extension_id for an authorized client
pub(super) async fn get_client_extension(
    app_handle: &AppHandle,
    client_id: &str,
) -> Option<String> {
    let state = app_handle.state::<AppState>();
    let params = vec![JsonValue::String(client_id.to_string())];

    match select_with_crdt(SQL_GET_CLIENT_EXTENSION.to_string(), params, &state.db) {
        Ok(rows) => {
            if let Some(row) = rows.first() {
                if let Some(ext_id) = row.first() {
                    return ext_id.as_str().map(|s| s.to_string());
                }
            }
            None
        }
        Err(_) => None,
    }
}

/// Update last_seen timestamp for a client
pub(super) async fn update_client_last_seen(
    app_handle: &AppHandle,
    client_id: &str,
) -> Result<(), BridgeError> {
    let state = app_handle.state::<AppState>();
    let hlc_guard = state
        .hlc
        .lock()
        .map_err(|e| BridgeError::Database(e.to_string()))?;

    let params = vec![JsonValue::String(client_id.to_string())];

    execute_with_crdt(
        SQL_UPDATE_LAST_SEEN.to_string(),
        params,
        &state.db,
        &hlc_guard,
    )
    .map_err(|e| BridgeError::Database(e.to_string()))?;

    Ok(())
}

/// Check if a client is authorized for a specific extension (by extension public_key + name)
pub(super) async fn check_client_authorized_for_extension(
    app_handle: &AppHandle,
    client_id: &str,
    extension_public_key: &str,
    extension_name: &str,
) -> bool {
    let state = app_handle.state::<AppState>();
    let params = vec![
        JsonValue::String(client_id.to_string()),
        JsonValue::String(extension_public_key.to_string()),
        JsonValue::String(extension_name.to_string()),
    ];

    match select_with_crdt(
        SQL_IS_CLIENT_AUTHORIZED_FOR_EXTENSION.to_string(),
        params,
        &state.db,
    ) {
        Ok(rows) => {
            if let Some(row) = rows.first() {
                if let Some(count) = row.first() {
                    return count.as_i64().unwrap_or(0) > 0;
                }
            }
            false
        }
        Err(e) => {
            eprintln!(
                "[ExternalBridge] Failed to check client authorization: {}",
                e
            );
            false
        }
    }
}

/// Check if a client is authorized for the core target.
/// Uses the simpler (client_id, extension_id) lookup since core has no
/// public_key/name pair to JOIN against.
pub(super) async fn check_client_authorized_for_core(
    app_handle: &AppHandle,
    client_id: &str,
) -> bool {
    use crate::external_bridge::authorization::SQL_IS_AUTHORIZED;

    let state = app_handle.state::<AppState>();
    let params = vec![
        JsonValue::String(client_id.to_string()),
        JsonValue::String(crate::external_bridge::CORE_EXTENSION_ID.to_string()),
    ];

    match select_with_crdt(SQL_IS_AUTHORIZED.to_string(), params, &state.db) {
        Ok(rows) => rows
            .first()
            .and_then(|row| row.first())
            .and_then(|count| count.as_i64())
            .map(|c| c > 0)
            .unwrap_or(false),
        Err(e) => {
            eprintln!("[ExternalBridge] Failed to check core authorization: {}", e);
            false
        }
    }
}

/// Get extension ID by public_key and name
pub(super) async fn get_extension_id_by_public_key_and_name(
    app_handle: &AppHandle,
    extension_public_key: &str,
    extension_name: &str,
) -> Option<String> {
    let state = app_handle.state::<AppState>();

    let params = vec![
        JsonValue::String(extension_public_key.to_string()),
        JsonValue::String(extension_name.to_string()),
    ];

    match select_with_crdt(
        SQL_GET_EXTENSION_ID_BY_PUBLIC_KEY_AND_NAME.to_string(),
        params,
        &state.db,
    ) {
        Ok(rows) => {
            if let Some(row) = rows.first() {
                if let Some(id) = row.first() {
                    return id.as_str().map(|s| s.to_string());
                }
            }
            None
        }
        Err(e) => {
            eprintln!("[ExternalBridge] Failed to get extension ID: {}", e);
            None
        }
    }
}

/// Maximum time to wait for an extension to signal ready (in milliseconds)
const EXTENSION_READY_TIMEOUT_MS: u64 = 30000;

/// Ensure an extension is loaded (auto-start if needed)
/// Returns Ok(()) if extension is loaded or was successfully started
///
/// This function:
/// 1. Checks if extension already has an open window (Desktop only)
/// 2. If not, emits an event to the frontend to request extension loading
/// 3. Waits for the extension to signal it's ready (via extension_signal_ready)
pub(super) async fn ensure_extension_loaded(
    app_handle: &AppHandle,
    extension_id: &str,
) -> Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let state = app_handle.state::<AppState>();

        // Check if extension already has an open window
        if state
            .extension_webview_manager
            .has_window_for_extension(extension_id)
        {
            eprintln!(
                "[ExternalBridge] Extension {} already has an open window",
                extension_id
            );
            return Ok(());
        }
    }

    // Extension not loaded - request frontend to start it
    eprintln!(
        "[ExternalBridge] Extension {} not loaded, requesting frontend to start it...",
        extension_id
    );

    // Get the ExternalBridge to set up the ready signal BEFORE emitting the auto-start event
    // This ensures we don't miss the signal if the extension starts very quickly
    let state = app_handle.state::<AppState>();
    let bridge = state.external_bridge.lock().await;

    // Pre-create the notify for this extension so we can wait on it
    {
        let mut signals = bridge.extension_ready_signals.write().await;
        signals
            .entry(extension_id.to_string())
            .or_insert_with(|| Arc::new(Notify::new()));
    }

    // Get the notify handle before dropping the lock
    let notify = {
        let signals = bridge.extension_ready_signals.read().await;
        signals.get(extension_id).cloned()
    };

    // Drop the bridge lock so the signal can be sent
    drop(bridge);

    // Emit event to frontend to start the extension
    // The frontend will handle this based on the extension's display_mode
    let payload = serde_json::json!({
        "extensionId": extension_id,
    });

    // Nur Main-Window — die Extension läuft noch nicht. Das Frontend startet
    // sie basierend auf dem display_mode (WebviewWindow oder Iframe).
    if let Err(e) = app_handle.emit_to("main", EVENT_EXTENSION_AUTO_START_REQUEST, &payload) {
        return Err(format!("Failed to emit auto-start request: {}", e));
    }

    // Wait for extension to signal ready (with timeout)
    // The extension will call extension_signal_ready after completing its initialization
    // (migrations, setup hook, etc.)
    if let Some(notify) = notify {
        eprintln!(
            "[ExternalBridge] Waiting for extension {} to signal ready (timeout: {}ms)...",
            extension_id, EXTENSION_READY_TIMEOUT_MS
        );

        let result = tokio::time::timeout(
            Duration::from_millis(EXTENSION_READY_TIMEOUT_MS),
            notify.notified(),
        )
        .await;

        // Cleanup the signal entry
        {
            let bridge = state.external_bridge.lock().await;
            let mut signals = bridge.extension_ready_signals.write().await;
            signals.remove(extension_id);
        }

        if result.is_err() {
            eprintln!(
                "[ExternalBridge] Timeout waiting for extension {} to signal ready",
                extension_id
            );
            // Don't fail - the extension might still be usable, just slow to initialize
            // The request will timeout naturally if the extension truly isn't working
        } else {
            eprintln!("[ExternalBridge] Extension {} signaled ready", extension_id);
        }
    }

    // Verify extension is now loaded (Desktop only)
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let state = app_handle.state::<AppState>();
        if !state
            .extension_webview_manager
            .has_window_for_extension(extension_id)
        {
            // Extension might be running in iframe mode, which we can't detect from backend
            // We'll proceed and let the request timeout if the extension doesn't respond
            eprintln!(
                "[ExternalBridge] Extension {} may be running in iframe mode or failed to start",
                extension_id
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod fail_closed_tests {
    #[test]
    fn check_client_blocked_fails_closed() {
        let source = include_str!("auth.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(p, _)| p)
            .unwrap_or(source);

        // Locate the function body.
        let fn_marker = "async fn check_client_blocked(";
        let fn_start = production
            .find(fn_marker)
            .expect("check_client_blocked must exist in auth.rs");
        // Scan forward to the next top-level fn (anchored at column 0).
        // Functions in this module are declared `pub(super) async fn`, so
        // anchor the next boundary on that prefix to keep the assertion
        // scoped to this function's body.
        let body_end = production[fn_start..]
            .find("\npub(super) async fn ")
            .or_else(|| production[fn_start..].find("\npub(super) fn "))
            .map(|off| fn_start + off)
            .unwrap_or(production.len());
        let body = &production[fn_start..body_end];

        assert!(
            !body.contains("Err(_) => false"),
            "check_client_blocked must not fail open. An `Err(_) => false` \
             arm lets known-blocked clients through during any transient \
             DB outage (migration, lock, read-only disk). Return `true` \
             on Err instead."
        );
        assert!(
            body.contains("true"),
            "check_client_blocked's Err arm must return true (fail closed)"
        );
    }
}
