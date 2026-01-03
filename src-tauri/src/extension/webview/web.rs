//! WebView-specific extension commands
//!
//! Commands that are only needed for WebView extensions (desktop native windows).
//! Most extension commands are unified in extension::web, extension::database, etc.
//! and work for both WebView and iframe extensions.

use crate::extension::core::protocol::ExtensionInfo;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::ResourceType;
use crate::extension::utils::resolve_extension_id;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{Emitter, State, WebviewWindow};

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

// ============================================================================
// Filtered Sync Event Emission
// ============================================================================

// Event names matching @haex-space/vault-sdk HAEXTENSION_EVENTS
// Source of truth: haex-vault-sdk/src/events.ts

/// Event for sync tables updated - sent to extensions after CRDT pull
/// Matches HAEXTENSION_EVENTS.SYNC_TABLES_UPDATED in vault-sdk
pub const SYNC_TABLES_EVENT: &str = "haextension:sync:tables-updated";

/// Internal event for sync tables updated - used by main window stores
pub const SYNC_TABLES_INTERNAL_EVENT: &str = "sync:tables-updated";

/// Payload for sync tables updated event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTablesPayload {
    pub tables: Vec<String>,
}

/// Result containing filtered tables per extension (for iframe forwarding)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilteredSyncTablesResult {
    /// Map of extension_id -> list of tables they are allowed to see
    pub extensions: HashMap<String, Vec<String>>,
}

/// Emits filtered sync:tables-updated events to WebView extensions.
///
/// Each extension only receives the table names they have database permissions for.
/// This prevents extensions from seeing activity in tables they don't have access to.
///
/// For WebView extensions: Emits directly to the first window of each extension.
/// For iframe extensions: Returns filtered table lists so frontend can forward via postMessage.
///
/// Deduplication: Only emits to the first WebView window per extension to prevent
/// duplicate event processing when multiple windows are open.
#[tauri::command]
pub async fn extension_emit_filtered_sync_tables(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    tables: Vec<String>,
) -> Result<FilteredSyncTablesResult, ExtensionError> {
    eprintln!(
        "[SyncEvent] Emitting filtered sync:tables-updated for {} tables",
        tables.len()
    );

    // Load extensions if not already loaded (same as get_all_extensions command)
    state
        .extension_manager
        .load_installed_extensions(&app_handle, &state)
        .await?;

    // Get all installed extensions
    let all_extensions = state.extension_manager.get_all_extensions()?;
    eprintln!(
        "[SyncEvent] Found {} installed extensions in ExtensionManager",
        all_extensions.len()
    );
    for ext in &all_extensions {
        eprintln!(
            "[SyncEvent]   - Extension: {}:{} (v{})",
            ext.manifest.public_key, ext.manifest.name, ext.manifest.version
        );
    }

    // Track which extensions we've already emitted to (for WebView deduplication)
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let mut emitted_to_webviews: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Result for iframe forwarding
    let mut result = FilteredSyncTablesResult {
        extensions: HashMap::new(),
    };

    for extension in all_extensions {
        let extension_id = format!("{}:{}", extension.manifest.public_key, extension.manifest.name);

        // Get permissions for this extension
        let permissions = PermissionManager::get_permissions(&state, &extension_id).await?;

        // Filter tables based on database permissions
        let allowed_tables: Vec<String> = tables
            .iter()
            .filter(|table_name| {
                // Check if extension has any DB permission for this table
                permissions.iter().any(|perm| {
                    if perm.resource_type != ResourceType::Db {
                        return false;
                    }

                    // Check if target matches (wildcard or exact/prefix match)
                    let target = &perm.target;
                    if target == "*" {
                        return true;
                    }

                    // Check prefix match (e.g., "publickey__extname__*" matches "publickey__extname__table")
                    if target.ends_with('*') {
                        let prefix = &target[..target.len() - 1];
                        return table_name.starts_with(prefix);
                    }

                    // Exact match
                    target == *table_name
                })
            })
            .cloned()
            .collect();

        // Skip if no tables are allowed for this extension
        if allowed_tables.is_empty() {
            continue;
        }

        eprintln!(
            "[SyncEvent] Extension {} can see {} of {} tables",
            extension_id,
            allowed_tables.len(),
            tables.len()
        );

        // Store for iframe forwarding
        result.extensions.insert(extension_id.clone(), allowed_tables.clone());

        // Emit to WebView if this extension has one (desktop only)
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            // Skip if we already emitted to a WebView for this extension
            if emitted_to_webviews.contains(&extension_id) {
                continue;
            }

            let payload = SyncTablesPayload {
                tables: allowed_tables,
            };

            // Try to emit to this extension's WebView
            match state.extension_webview_manager.emit_to_extension(
                &app_handle,
                &extension_id,
                SYNC_TABLES_EVENT,
                &payload,
            ) {
                Ok(true) => {
                    emitted_to_webviews.insert(extension_id.clone());
                    eprintln!(
                        "[SyncEvent] Emitted to WebView for extension: {}",
                        extension_id
                    );
                }
                Ok(false) => {
                    // No WebView for this extension - iframe mode, will be handled by frontend
                    eprintln!(
                        "[SyncEvent] No WebView for extension: {} (will use iframe)",
                        extension_id
                    );
                }
                Err(e) => {
                    eprintln!(
                        "[SyncEvent] Error emitting to WebView for {}: {}",
                        extension_id, e
                    );
                }
            }
        }
    }

    // Emit internal event for main window stores (unfiltered, internal use only)
    let _ = app_handle.emit(SYNC_TABLES_INTERNAL_EVENT, SyncTablesPayload { tables });

    Ok(result)
}
