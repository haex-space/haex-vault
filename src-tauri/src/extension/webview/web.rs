//! WebView-specific extension types and commands
//!
//! Types used for extension communication (context, etc.).
//! Most commands are now in extension::mod.rs for better organization.

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
    let _extension_id = resolve_extension_id(&window, &state, public_key.clone(), name.clone())?;

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
