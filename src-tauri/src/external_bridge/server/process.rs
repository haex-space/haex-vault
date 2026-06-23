//! Request dispatcher — validates target, looks up the extension, and
//! routes the decrypted payload to the right window via Tauri events.

use crate::AppState;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{oneshot, RwLock};

use super::auth::{
    check_client_authorized_for_core, check_client_authorized_for_extension,
    ensure_extension_loaded, get_extension_id_by_public_key_and_name,
};
use super::{ResponseSender, SessionAuthorization, DEFAULT_REQUEST_TIMEOUT_SECS};

/// Process a decrypted request and route it to the appropriate extension
///
/// # Arguments
/// * `action` - The action/method name to perform
/// * `payload` - The decrypted request payload (must contain requestId)
/// * `client_public_key` - Client's public key (Base64 SPKI format, used as identifier)
/// * `extension_public_key` - Target extension's public key (from manifest)
/// * `extension_name` - Target extension's name (from manifest)
/// * `client_id` - Client's unique identifier
/// * `app_handle` - Tauri app handle for emitting events
/// * `pending_responses` - Map to store response channel for correlation
pub(super) async fn process_request(
    action: &str,
    payload: &serde_json::Value,
    client_public_key: &str,
    extension_public_key: Option<&str>,
    extension_name: Option<&str>,
    client_id: &str,
    app_handle: &AppHandle,
    pending_responses: Arc<RwLock<HashMap<String, ResponseSender>>>,
    session_authorizations: Arc<RwLock<HashMap<String, SessionAuthorization>>>,
) -> serde_json::Value {
    // Extract requestId - required for response correlation
    let request_id = match payload.get("requestId").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            return serde_json::json!({
                "success": false,
                "error": "Missing required field: requestId"
            });
        }
    };

    // Validate that extension target is specified
    let (ext_public_key, ext_name) = match (extension_public_key, extension_name) {
        (Some(pk), Some(name)) if !pk.is_empty() && !name.is_empty() => (pk, name),
        _ => {
            return serde_json::json!({
                "requestId": request_id,
                "success": false,
                "error": "Missing required fields: extensionPublicKey and extensionName"
            });
        }
    };

    // Core-target detection: requests addressed to the haex-vault core itself
    // (not a specific extension) carry the CORE sentinel as extensionPublicKey/name.
    let is_core = ext_public_key == crate::external_bridge::CORE_EXTENSION_ID
        && ext_name == crate::external_bridge::CORE_EXTENSION_NAME;

    // Lookup the extension's internal ID first (needed for session auth check)
    let extension_id = if is_core {
        crate::external_bridge::CORE_EXTENSION_ID.to_string()
    } else {
        match get_extension_id_by_public_key_and_name(app_handle, ext_public_key, ext_name).await {
            Some(id) => id,
            None => {
                return serde_json::json!({
                    "requestId": request_id,
                    "success": false,
                    "error": "Extension not found"
                });
            }
        }
    };

    // Verify client is authorized for this extension (or core)
    // Check both database authorization AND session authorization ("allow once")
    let db_authorized = if is_core {
        check_client_authorized_for_core(app_handle, client_id).await
    } else {
        check_client_authorized_for_extension(app_handle, client_id, ext_public_key, ext_name).await
    };
    let session_authorized = {
        let auths = session_authorizations.read().await;
        auths
            .get(client_id)
            .map(|sa| sa.extension_id == extension_id)
            .unwrap_or(false)
    };

    if !db_authorized && !session_authorized {
        return serde_json::json!({
            "requestId": request_id,
            "success": false,
            "error": if is_core {
                "Client not authorized for core access".to_string()
            } else {
                "Client not authorized for this extension".to_string()
            }
        });
    }

    // Ensure the extension is loaded (auto-start if needed).
    // Core requests are handled by the main window — no extension to load.
    if !is_core {
        if let Err(e) = ensure_extension_loaded(app_handle, &extension_id).await {
            eprintln!(
                "[ExternalBridge] Failed to ensure extension is loaded: {}",
                e
            );
            return serde_json::json!({
                "requestId": request_id,
                "success": false,
                "error": format!("Failed to load extension: {}", e)
            });
        }
    }

    // Create oneshot channel for response
    let (tx, rx) = oneshot::channel::<serde_json::Value>();

    // Store the sender in pending_responses
    {
        let mut pending = pending_responses.write().await;
        pending.insert(request_id.clone(), tx);
    }

    // Build the external request payload to send to the extension
    let external_request = serde_json::json!({
        "requestId": request_id,
        "publicKey": client_public_key,
        "action": action,
        "payload": payload,
        "extensionPublicKey": ext_public_key,
        "extensionName": ext_name
    });

    // Emit the request to the extension via Tauri event.
    // - Core target: emit "haextension:external:core-request" to main window.
    // - WebView extension: emit_to_all_extension_windows() targets ONLY that
    //   extension's webview windows by label.
    // - Iframe extension (or no native webview): emit_to("main", …) so the
    //   frontend can forward via postMessage to the iframe of THIS extension.
    //   .emit() would broadcast to every webview, leaking the request payload
    //   (incl. publicKey, action, payload) to unrelated extensions.
    let emit_result = if is_core {
        let ok = app_handle
            .emit_to(
                "main",
                "haextension:external:core-request",
                &external_request,
            )
            .is_ok();
        if ok {
            eprintln!("[ExternalBridge] Emitted core request to main window");
        }
        ok
    } else {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let state = app_handle.state::<AppState>();
            let manager = &state.extension_webview_manager;

            // Try to emit to all webviews of this extension first
            match manager.emit_to_all_extension_windows(
                app_handle,
                &extension_id,
                "haextension:external:request",
                external_request.clone(),
            ) {
                Ok(true) => {
                    eprintln!(
                        "[ExternalBridge] Emitted request to extension webview(s): {}",
                        extension_id
                    );
                    true
                }
                Ok(false) => {
                    eprintln!(
                        "[ExternalBridge] No webview for extension {}, emitting to main window",
                        extension_id
                    );
                    app_handle
                        .emit_to("main", "haextension:external:request", &external_request)
                        .is_ok()
                }
                Err(e) => {
                    eprintln!(
                        "[ExternalBridge] Error emitting to webview(s): {}, trying main window",
                        e
                    );
                    app_handle
                        .emit_to("main", "haextension:external:request", &external_request)
                        .is_ok()
                }
            }
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            // Mobile: always emit to main window (iframe mode)
            app_handle
                .emit_to("main", "haextension:external:request", &external_request)
                .is_ok()
        }
    };

    if !emit_result {
        eprintln!("[ExternalBridge] Failed to emit external request to any window");
        // Clean up pending response
        let mut pending = pending_responses.write().await;
        pending.remove(&request_id);
        return serde_json::json!({
            "requestId": request_id,
            "success": false,
            "error": "Failed to route request to extension"
        });
    }

    // Wait for response with timeout
    // TODO: Make timeout configurable per extension
    match tokio::time::timeout(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS), rx).await {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => {
            // Channel was dropped (sender was dropped without sending)
            serde_json::json!({
                "requestId": request_id,
                "success": false,
                "error": "Extension did not respond"
            })
        }
        Err(_) => {
            // Timeout
            // Clean up pending response
            let mut pending = pending_responses.write().await;
            pending.remove(&request_id);
            serde_json::json!({
                "requestId": request_id,
                "success": false,
                "error": "Request timeout"
            })
        }
    }
}
