// src-tauri/src/extension/web/commands.rs
//!
//! Tauri commands for extension web operations
//!
//! These commands work for both WebView and iframe extensions:
//! - WebView: extension_id is resolved from the window context
//! - iframe: extension_id is resolved from public_key/name parameters
//!           (verified by frontend via origin check)

use crate::database::core::with_connection;
use crate::extension::error::ExtensionError;
use crate::extension::utils::{emit_permission_prompt_if_needed, resolve_extension_id};
use crate::extension::web::helpers::fetch_web_request;
use crate::extension::web::types::{WebFetchRequest, WebFetchResponse};
use crate::AppState;
use std::collections::HashMap;
use tauri::{AppHandle, State, WebviewWindow};

/// Check web limits (rate limit and concurrent requests) for an extension
fn check_web_limits(state: &AppState, extension_id: &str) -> Result<(), ExtensionError> {
    let limits = with_connection(&state.db, |conn| {
        state.limits.get_limits(conn, extension_id)
    })?;

    state
        .limits
        .web()
        .check_rate_limit(extension_id, &limits.web)
        .map_err(|e| ExtensionError::LimitExceeded {
            reason: e.to_string(),
        })
}

#[tauri::command]
pub async fn extension_web_open(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    url: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    // Resolve extension_id from window (WebView) or parameters (iframe)
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check web limits (rate limit)
    check_web_limits(&state, &extension_id)?;

    // Validate URL format
    let parsed_url = url::Url::parse(&url).map_err(|e| ExtensionError::WebError {
        reason: format!("Invalid URL: {}", e),
    })?;

    // Only allow http and https URLs
    let scheme = parsed_url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(ExtensionError::WebError {
            reason: format!(
                "Unsupported URL scheme: {}. Only http and https are allowed.",
                scheme
            ),
        });
    }

    // Check web permissions
    let permission_result =
        crate::extension::permissions::manager::PermissionManager::check_web_permission(
            &state,
            &extension_id,
            &url,
        )
        .await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    permission_result?;

    // Open URL in default browser using tauri-plugin-opener
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|e| ExtensionError::WebError {
        reason: format!("Failed to open URL in browser: {}", e),
    })?;

    Ok(())
}

#[tauri::command]
pub async fn extension_web_fetch(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    url: String,
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
    timeout: Option<u64>,
    allow_once: Option<bool>,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<WebFetchResponse, ExtensionError> {
    // Resolve extension_id from window (WebView) or parameters (iframe)
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    // Check web limits (rate limit)
    check_web_limits(&state, &extension_id)?;

    let method_str = method.as_deref().unwrap_or("GET");

    // Skip permission check if allowOnce is true (user clicked "Allow Once" in dialog)
    if !allow_once.unwrap_or(false) {
        // Check web permissions before making request
        let permission_result =
            crate::extension::permissions::manager::PermissionManager::check_web_permission(
                &state,
                &extension_id,
                &url,
            )
            .await;

        if let Err(ref e) = permission_result {
            emit_permission_prompt_if_needed(&app_handle, e);
        }
        permission_result?;
    }

    let request = WebFetchRequest {
        url,
        method: Some(method_str.to_string()),
        headers,
        body,
        timeout,
    };

    fetch_web_request(request).await
}
