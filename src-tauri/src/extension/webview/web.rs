//! WebView-specific extension commands
//!
//! Commands that are only needed for WebView extensions (desktop native windows).
//! Most extension commands are unified in extension::web, extension::database, etc.
//! and work for both WebView and iframe extensions.

use crate::extension::core::protocol::ExtensionInfo;
use crate::extension::error::ExtensionError;
use crate::extension::utils::resolve_extension_id;
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::{State, WebviewWindow};

use super::helpers::get_extension_info_from_window;

// ============================================================================
// Types for SDK communication
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationContext {
    pub theme: String,
    pub locale: String,
    #[serde(default)]
    pub platform: String,
    pub device_id: String,
}

// ============================================================================
// Extension Info Command
// ============================================================================

/// Get extension info for the calling extension.
/// Works for both WebView (from window) and iframe (from parameters).
#[tauri::command]
pub fn extension_get_info(
    window: WebviewWindow,
    state: State<'_, AppState>,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<ExtensionInfo, ExtensionError> {
    // Try to resolve extension_id first
    let extension_id = resolve_extension_id(&window, &state, public_key.clone(), name.clone())?;

    // If we have public_key and name, we can construct ExtensionInfo from the manager
    if let Some(pk) = public_key {
        if let Some(n) = name {
            // Get extension from manager
            if let Some(extension) = state
                .extension_manager
                .get_extension_by_public_key_and_name(&pk, &n)?
            {
                return Ok(ExtensionInfo {
                    public_key: extension.manifest.public_key,
                    name: extension.manifest.name,
                    version: extension.manifest.version,
                });
            }
        }
    }

    // Fallback: Get from window (for native WebView extensions)
    get_extension_info_from_window(&window, &state)
}

// ============================================================================
// Context API Commands
// ============================================================================

/// Get application context (theme, locale, platform, device_id)
#[tauri::command]
pub fn extension_context_get(
    state: State<'_, AppState>,
) -> Result<ApplicationContext, ExtensionError> {
    let context = state
        .context
        .lock()
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("Failed to lock context: {}", e),
        })?;
    Ok(context.clone())
}

/// Set application context (used by host application)
#[tauri::command]
pub fn extension_context_set(
    state: State<'_, AppState>,
    context: ApplicationContext,
) -> Result<(), ExtensionError> {
    let mut current_context =
        state
            .context
            .lock()
            .map_err(|e| ExtensionError::ValidationError {
                reason: format!("Failed to lock context: {}", e),
            })?;
    *current_context = context;
    Ok(())
}

// ============================================================================
// Event Broadcasting
// ============================================================================

/// Broadcasts an event to all extension webview windows
#[tauri::command]
pub async fn extension_emit_to_all(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    event: String,
    payload: serde_json::Value,
) -> Result<(), ExtensionError> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        state
            .extension_webview_manager
            .emit_to_all_extensions(&app_handle, &event, payload)?;
    }

    // Suppress unused variable warning on mobile
    let _ = app_handle;
    let _ = event;
    let _ = payload;

    Ok(())
}
