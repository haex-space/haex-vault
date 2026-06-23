//! Filtered Sync Event Emission (Cross-platform)

use crate::{
    extension::{
        error::ExtensionError,
        permissions::{
            manager::PermissionManager,
            types::{Principal, ResourceType},
        },
    },
    AppState,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, State};

/// Event for sync tables updated - sent to extensions after CRDT pull
/// Matches HAEXTENSION_EVENTS.SYNC_TABLES_UPDATED in vault-sdk
#[cfg(desktop)]
pub const SYNC_TABLES_EVENT: &str = "haextension:sync:tables-updated";

/// Payload for sync tables updated event (used by desktop webview extensions)
#[cfg_attr(any(target_os = "android", target_os = "ios"), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTablesPayload {
    pub tables: Vec<String>,
}

/// Result containing filtered tables per extension
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilteredSyncTablesResult {
    /// Map of extension_id -> list of tables they are allowed to see
    pub extensions: HashMap<String, Vec<String>>,
}

/// Filter sync tables by extension permissions.
///
/// Each extension only receives the table names they have database permissions for.
/// This prevents extensions from seeing activity in tables they don't have access to.
///
/// Returns a map of extension_id -> allowed table names.
/// This function does NOT emit any events - use extension_emit_sync_tables for webviews.
#[tauri::command]
pub async fn extension_filter_sync_tables(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    tables: Vec<String>,
) -> Result<FilteredSyncTablesResult, ExtensionError> {
    eprintln!("[SyncEvent] ========== FILTERING SYNC TABLES ==========");
    eprintln!("[SyncEvent] Tables to filter: {:?}", tables);

    // Load extensions if not already loaded
    state
        .extension_manager
        .load_installed_extensions(&app_handle, &state)
        .await?;

    // Get all installed extensions
    let all_extensions = state.extension_manager.get_all_extensions()?;
    eprintln!(
        "[SyncEvent] Found {} installed extensions",
        all_extensions.len()
    );

    let mut result = FilteredSyncTablesResult {
        extensions: HashMap::new(),
    };

    for extension in all_extensions {
        let extension_id = extension.id.clone();

        // Get permissions for this extension
        let permissions =
            PermissionManager::get_permissions(&state, &Principal::Extension(extension_id.clone()))
                .await?;

        // Filter tables based on:
        // 1. Extension's own tables (prefix match) - always allowed without explicit permissions
        // 2. Explicit database permissions for other tables
        let allowed_tables: Vec<String> = tables
            .iter()
            .filter(|table_name| {
                // Extensions always have implicit access to their own tables
                if crate::extension::utils::is_extension_table(
                    table_name,
                    &extension.manifest.public_key,
                    &extension.manifest.name,
                ) {
                    return true;
                }

                // Check if extension has explicit DB permission for this table
                permissions.iter().any(|perm| {
                    if perm.resource_type != ResourceType::Db {
                        return false;
                    }

                    let target = &perm.target;
                    if target == "*" {
                        return true;
                    }

                    if target.ends_with('*') {
                        let prefix = &target[..target.len() - 1];
                        return table_name.starts_with(prefix);
                    }

                    target == *table_name
                })
            })
            .cloned()
            .collect();

        if !allowed_tables.is_empty() {
            eprintln!(
                "[SyncEvent] Extension {} can see {} of {} tables",
                extension_id,
                allowed_tables.len(),
                tables.len()
            );
            result.extensions.insert(extension_id, allowed_tables);
        }
    }

    Ok(result)
}

/// Emit sync:tables-updated events to webview extensions.
///
/// Takes a pre-filtered map of extension_id -> tables and emits to each extension's webviews.
/// Desktop only - on mobile, use postMessage for iframes from the frontend.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn extension_emit_sync_tables(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    filtered_extensions: FilteredSyncTablesResult,
) -> Result<(), ExtensionError> {
    eprintln!("[SyncEvent] ========== EMITTING SYNC TABLES TO WEBVIEWS ==========");
    eprintln!(
        "[SyncEvent] Extensions to emit to: {}",
        filtered_extensions.extensions.len()
    );

    for (extension_id, allowed_tables) in filtered_extensions.extensions {
        if allowed_tables.is_empty() {
            continue;
        }

        let payload = SyncTablesPayload {
            tables: allowed_tables.clone(),
        };

        match state
            .extension_webview_manager
            .emit_to_all_extension_windows(&app_handle, &extension_id, SYNC_TABLES_EVENT, &payload)
        {
            Ok(true) => {
                eprintln!(
                    "[SyncEvent] Emitted to WebView(s) for extension: {}",
                    extension_id
                );
            }
            Ok(false) => {
                eprintln!(
                    "[SyncEvent] No WebView for extension: {} (iframe mode)",
                    extension_id
                );
            }
            Err(e) => {
                eprintln!(
                    "[SyncEvent] Error emitting to WebView(s) for {}: {}",
                    extension_id, e
                );
            }
        }
    }

    Ok(())
}
