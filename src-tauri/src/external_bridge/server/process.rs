//! Request dispatcher — validates target, looks up the extension, and
//! routes the decrypted payload to the right window via Tauri events.

use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{PasswordsAction, Principal, RwAction};
use crate::extension::utils::emit_permission_prompt_if_needed;
use crate::AppState;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{oneshot, RwLock};

use super::auth::{
    check_client_authorized_for_core, check_client_authorized_for_extension,
    ensure_extension_loaded, get_extension_id_by_public_key_and_name,
};
use super::{ResponseSender, SessionAuthorization, DEFAULT_REQUEST_TIMEOUT_SECS};

/// How long the bridge waits for a permission-prompt decision before giving
/// up and reporting `PERMISSION_PROMPT_TIMEOUT` to the client. Matches the
/// frontend dialog's own patience — a user who wanted to decide would have
/// done so well before this.
const PERMISSION_PROMPT_TIMEOUT_SECS: u64 = 120;

/// Core (haex-vault built-in) actions mapped to the `PasswordsAction` they
/// require. Kept in sync with `CORE_METHODS` in
/// `useCoreExternalRequestHandlers/types.ts`. Unknown actions return `None`
/// and are rejected fail-closed by the caller — never prompted, since an
/// undeclared/unknown method has no legitimate reason to reach the vault.
fn map_core_action_to_passwords_action(action: &str) -> Option<PasswordsAction> {
    match action {
        "get-items"
        | "get-totp"
        | "get-password-config"
        | "get-password-presets"
        | "passkey-get"
        | "passkey-list" => Some(PasswordsAction::Read),
        "create-item" | "update-item" | "passkey-create" => Some(PasswordsAction::ReadWrite),
        _ => None,
    }
}

/// Core bookmarks actions mapped to the `RwAction` they require. Kept in sync
/// with `CORE_METHODS` in `useCoreExternalRequestHandlers/types.ts`. Unknown
/// actions return `None` and are rejected fail-closed by the caller.
/// Bookmarks access is authorized via its own `bookmarks` resource — a
/// bookmarks method is never satisfied by a `passwords` grant (see
/// `map_core_action_to_passwords_action`, which returns `None` for these
/// action strings).
fn map_core_action_to_bookmarks_action(action: &str) -> Option<RwAction> {
    match action {
        "bookmarks-collections-list" | "bookmarks-list" => Some(RwAction::Read),
        "bookmarks-collection-create"
        | "bookmarks-upsert"
        | "bookmarks-delete"
        | "bookmarks-device-upsert" => Some(RwAction::ReadWrite),
        _ => None,
    }
}

/// Builds the waiter key from a `PermissionPromptRequired` error. These are
/// the exact strings the frontend round-trips back through
/// `notify_extension_permission_decision` (including the ORIGINAL prompt
/// target), so the waiter key here must match them precisely.
fn prompt_err_key(
    err: &ExtensionError,
) -> Option<crate::extension::permissions::waiters::PromptKey> {
    match err {
        ExtensionError::PermissionPromptRequired {
            extension_id,
            resource_type,
            action,
            target,
            ..
        } => Some((
            extension_id.clone(),
            resource_type.clone(),
            action.clone(),
            target.clone(),
        )),
        _ => None,
    }
}

/// Waits for a `PermissionPromptRequired` error to resolve: registers a
/// server-side waiter BEFORE emitting the prompt event (so the decision can
/// never race ahead of the registration), waits up to
/// `PERMISSION_PROMPT_TIMEOUT_SECS`, and returns whether a decision arrived.
/// The caller is responsible for the authoritative re-check afterwards — a
/// `true` return here only means "a decision was made", not "it was granted".
async fn wait_for_permission_decision(
    app_handle: &AppHandle,
    prompt_error: &ExtensionError,
) -> bool {
    let Some(key) = prompt_err_key(prompt_error) else {
        return false;
    };
    let state = app_handle.state::<AppState>();
    let rx = state.permission_prompt_waiters.register(key).await;

    emit_permission_prompt_if_needed(app_handle, prompt_error);

    matches!(
        tokio::time::timeout(Duration::from_secs(PERMISSION_PROMPT_TIMEOUT_SECS), rx).await,
        Ok(Ok(()))
    )
}

/// Outcome of [`check_with_prompt_retry`].
enum PromptRetry<T> {
    /// A decision arrived in time (or none was needed) — `check` ran to completion.
    Resolved(Result<T, ExtensionError>),
    /// No decision arrived before the prompt timeout.
    TimedOut,
}

/// Runs `check`; if it reports `PermissionPromptRequired`, waits for the
/// user's decision and re-runs `check` exactly once more. Shared by every
/// core/extension permission gate in `process_request` below — they differ
/// only in which `PermissionManager::check_*` function `check` calls and its
/// `Ok` payload.
///
/// `check` returns a boxed future rather than being an async closure: a
/// generic `impl AsyncFnMut` here made the futures returned by
/// `process_request` non-`Send` in a way the compiler couldn't verify,
/// breaking the unrelated `tokio::spawn` call in `server/mod.rs` that awaits
/// this function's caller.
async fn check_with_prompt_retry<'a, T>(
    app_handle: &AppHandle,
    mut check: impl FnMut() -> std::pin::Pin<
        Box<dyn Future<Output = Result<T, ExtensionError>> + Send + 'a>,
    >,
) -> PromptRetry<T> {
    let mut result = check().await;
    if let Err(err @ ExtensionError::PermissionPromptRequired { .. }) = &result {
        if !wait_for_permission_decision(app_handle, err).await {
            return PromptRetry::TimedOut;
        }
        result = check().await;
    }
    PromptRetry::Resolved(result)
}

fn timeout_response(request_id: &str) -> serde_json::Value {
    serde_json::json!({
        "requestId": request_id,
        "success": false,
        "errorCode": "PERMISSION_PROMPT_TIMEOUT",
        "error": "Permission prompt timed out"
    })
}

fn denied_response(
    request_id: &str,
    error_code: &str,
    error: &ExtensionError,
) -> serde_json::Value {
    serde_json::json!({
        "requestId": request_id,
        "success": false,
        "errorCode": error_code,
        "error": error.to_string()
    })
}

#[cfg(test)]
mod core_action_mapping_tests {
    use super::*;

    #[test]
    fn read_only_core_methods_map_to_read() {
        for action in [
            "get-items",
            "get-totp",
            "get-password-config",
            "get-password-presets",
            "passkey-get",
            "passkey-list",
        ] {
            assert_eq!(
                map_core_action_to_passwords_action(action),
                Some(PasswordsAction::Read),
                "{action} should map to Read"
            );
        }
    }

    #[test]
    fn write_core_methods_map_to_read_write() {
        for action in ["create-item", "update-item", "passkey-create"] {
            assert_eq!(
                map_core_action_to_passwords_action(action),
                Some(PasswordsAction::ReadWrite),
                "{action} should map to ReadWrite"
            );
        }
    }

    #[test]
    fn unknown_action_fails_closed() {
        assert_eq!(
            map_core_action_to_passwords_action("delete-everything"),
            None
        );
        assert_eq!(map_core_action_to_passwords_action(""), None);
        assert_eq!(map_core_action_to_passwords_action("get-items "), None);
    }

    #[test]
    fn read_only_bookmarks_methods_map_to_read() {
        for action in ["bookmarks-collections-list", "bookmarks-list"] {
            assert_eq!(
                map_core_action_to_bookmarks_action(action),
                Some(RwAction::Read),
                "{action} should map to Read"
            );
        }
    }

    #[test]
    fn write_bookmarks_methods_map_to_read_write() {
        for action in [
            "bookmarks-collection-create",
            "bookmarks-upsert",
            "bookmarks-delete",
            "bookmarks-device-upsert",
        ] {
            assert_eq!(
                map_core_action_to_bookmarks_action(action),
                Some(RwAction::ReadWrite),
                "{action} should map to ReadWrite"
            );
        }
    }

    #[test]
    fn unknown_bookmarks_action_fails_closed() {
        assert_eq!(
            map_core_action_to_bookmarks_action("delete-everything"),
            None
        );
        assert_eq!(map_core_action_to_bookmarks_action(""), None);
    }

    /// A bookmarks core method must never be satisfied by a passwords grant
    /// (and vice versa) — the two resources are gated independently.
    #[test]
    fn bookmarks_methods_are_not_authorized_via_passwords_mapping() {
        for action in [
            "bookmarks-collections-list",
            "bookmarks-collection-create",
            "bookmarks-list",
            "bookmarks-upsert",
            "bookmarks-delete",
            "bookmarks-device-upsert",
        ] {
            assert_eq!(
                map_core_action_to_passwords_action(action),
                None,
                "{action} must not map to a passwords action"
            );
        }
        for action in [
            "get-items",
            "get-totp",
            "create-item",
            "update-item",
            "get-password-config",
            "get-password-presets",
            "passkey-create",
            "passkey-get",
            "passkey-list",
        ] {
            assert_eq!(
                map_core_action_to_bookmarks_action(action),
                None,
                "{action} must not map to a bookmarks action"
            );
        }
    }
}

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
            .values()
            .any(|sa| sa.client_id == client_id && sa.extension_id == extension_id)
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

    // Fine-grained permission gate — the connection-level authorization check
    // above only establishes that this client may talk to this extension (or
    // core) AT ALL. This gate enforces the actual declared-action scope,
    // exactly like the extension permission system. Runs BEFORE
    // ensure_extension_loaded/event-dispatch so the 30s response timeout only
    // starts once permission is actually granted.
    let principal = Principal::ExternalClient(client_id.to_string());
    let mut core_passwords_scope = None;
    if is_core {
        if let Some(passwords_action) = map_core_action_to_passwords_action(action) {
            match check_with_prompt_retry(app_handle, || {
                let state = app_handle.state::<AppState>();
                let principal = &principal;
                let passwords_action = passwords_action.clone();
                Box::pin(async move {
                    PermissionManager::check_passwords_permission(
                        &state,
                        principal,
                        passwords_action,
                    )
                    .await
                })
            })
            .await
            {
                PromptRetry::TimedOut => return timeout_response(&request_id),
                PromptRetry::Resolved(Ok(scope)) => core_passwords_scope = Some(scope),
                PromptRetry::Resolved(Err(e)) => {
                    return denied_response(&request_id, "PERMISSION_DENIED", &e)
                }
            }
        } else if let Some(bookmarks_action) = map_core_action_to_bookmarks_action(action) {
            match check_with_prompt_retry(app_handle, || {
                let state = app_handle.state::<AppState>();
                let principal = &principal;
                Box::pin(async move {
                    PermissionManager::check_bookmarks_permission(
                        &state,
                        principal,
                        bookmarks_action,
                    )
                    .await
                })
            })
            .await
            {
                PromptRetry::TimedOut => return timeout_response(&request_id),
                PromptRetry::Resolved(Ok(())) => {}
                PromptRetry::Resolved(Err(e)) => {
                    return denied_response(&request_id, "BOOKMARKS_SCOPE_DENIED", &e)
                }
            }
        } else {
            // Unknown core action: fail closed, never prompt.
            return serde_json::json!({
                "requestId": request_id,
                "success": false,
                "errorCode": "PERMISSION_DENIED",
                "error": format!("Unknown core action: {action}")
            });
        }
    } else {
        match check_with_prompt_retry(app_handle, || {
            let state = app_handle.state::<AppState>();
            let principal = &principal;
            Box::pin(async move {
                PermissionManager::check_extension_api_permission(
                    &state,
                    principal,
                    ext_public_key,
                    ext_name,
                    action,
                )
                .await
            })
        })
        .await
        {
            PromptRetry::TimedOut => return timeout_response(&request_id),
            PromptRetry::Resolved(Ok(())) => {}
            PromptRetry::Resolved(Err(e)) => {
                return denied_response(&request_id, "PERMISSION_DENIED", &e)
            }
        }
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

    // Build the external request payload to send to the extension. For core
    // requests, `scope` carries the resolved passwords tag-scope so the
    // frontend core handler (`useCoreExternalRequestHandlers/passwords.ts`)
    // can filter queries to the allowed tags — the Read/Write boundary is
    // already enforced above, tag-scoping is refinement within that boundary.
    let external_request = serde_json::json!({
        "requestId": request_id,
        "publicKey": client_public_key,
        "action": action,
        "payload": payload,
        "extensionPublicKey": ext_public_key,
        "extensionName": ext_name,
        "scope": core_passwords_scope
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
