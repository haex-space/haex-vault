// src-tauri/src/extension/web/commands.rs
//!
//! Tauri commands for extension web operations
//!

use crate::extension::error::ExtensionError;
use crate::extension::web::helpers::fetch_web_request;
use crate::extension::web::types::{WebFetchRequest, WebFetchResponse};
use crate::AppState;
use std::collections::HashMap;
use tauri::State;

#[tauri::command]
pub async fn extension_web_open(
    url: String,
    public_key: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    // Get extension to validate it exists
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)?
        .ok_or_else(|| ExtensionError::NotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

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
    crate::extension::permissions::manager::PermissionManager::check_web_permission(
        &state,
        &extension.id,
        &url,
    )
    .await?;

    // Open URL in default browser using tauri-plugin-opener
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|e| ExtensionError::WebError {
        reason: format!("Failed to open URL in browser: {}", e),
    })?;

    Ok(())
}

#[tauri::command]
pub async fn extension_web_fetch(
    url: String,
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
    timeout: Option<u64>,
    public_key: String,
    name: String,
    allow_once: Option<bool>,
    state: State<'_, AppState>,
) -> Result<WebFetchResponse, ExtensionError> {
    // Get extension to validate it exists
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)?
        .ok_or_else(|| ExtensionError::NotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    let method_str = method.as_deref().unwrap_or("GET");

    // Skip permission check if allowOnce is true (user clicked "Allow Once" in dialog)
    if !allow_once.unwrap_or(false) {
        // Check web permissions before making request
        crate::extension::permissions::manager::PermissionManager::check_web_permission(
            &state,
            &extension.id,
            &url,
        )
        .await?;
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
