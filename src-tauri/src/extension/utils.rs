// src-tauri/src/extension/utils.rs
// Utility functions for extension management

use crate::extension::error::ExtensionError;
use crate::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

// ============================================================================
// Permission Prompt Utilities
// ============================================================================

/// Event name for permission prompt required
pub const EVENT_PERMISSION_PROMPT_REQUIRED: &str = "extension:permission-prompt-required";

/// Payload for permission prompt event
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionPromptPayload {
    pub extension_id: String,
    pub extension_name: String,
    pub resource_type: String,
    pub action: String,
    pub target: String,
}

/// Emits a permission prompt event if the error is PermissionPromptRequired
pub fn emit_permission_prompt_if_needed(app_handle: &AppHandle, error: &ExtensionError) {
    if let ExtensionError::PermissionPromptRequired {
        extension_id,
        extension_name,
        resource_type,
        action,
        target,
    } = error
    {
        let payload = PermissionPromptPayload {
            extension_id: extension_id.clone(),
            extension_name: extension_name.clone(),
            resource_type: resource_type.clone(),
            action: action.clone(),
            target: target.clone(),
        };
        let _ = app_handle.emit(EVENT_PERMISSION_PROMPT_REQUIRED, &payload);
    }
}

// ============================================================================
// Extension Identification
// ============================================================================

/// Get extension ID from public_key and name.
/// Used by extension commands to identify the calling extension.
pub fn get_extension_id_by_key_and_name(
    state: &State<'_, AppState>,
    public_key: &str,
    name: &str,
) -> Result<String, ExtensionError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(public_key, name)?
        .ok_or_else(|| ExtensionError::NotFound {
            public_key: public_key.to_string(),
            name: name.to_string(),
        })?;

    Ok(extension.id)
}

/// Resolves extension_id from either window context or parameters.
///
/// SECURITY: This function prioritizes window-based identification.
/// If the window is a registered extension window, we ALWAYS use that ID
/// (cannot be spoofed by the extension). Only if the window is NOT an
/// extension window (e.g., main window for iframe requests), we fall back
/// to parameters that were verified by the frontend via origin check.
///
/// This allows a single Tauri command to serve both:
/// - WebView extensions (extension_id from window)
/// - iframe extensions (extension_id from frontend-verified parameters)
pub fn resolve_extension_id(
    #[allow(unused_variables)] window: &WebviewWindow,
    state: &State<'_, AppState>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<String, ExtensionError> {
    // On Desktop: First try to get extension_id from window (WebView case)
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let window_id = window.label();
        let extension_id_from_window = {
            let windows = state
                .extension_webview_manager
                .windows
                .lock()
                .map_err(|e| ExtensionError::MutexPoisoned {
                    reason: e.to_string(),
                })?;
            windows.get(window_id).cloned()
        };

        if let Some(extension_id) = extension_id_from_window {
            // Window is a registered extension window - use this ID
            // (ignoring any parameters that might have been passed)
            eprintln!(
                "[resolve_extension_id] Using window-based ID: {}",
                extension_id
            );
            return Ok(extension_id);
        }
    }

    // Fallback: Use parameters provided by frontend (after origin verification)
    // This is used for iframe extensions on all platforms, and is the only
    // option on mobile where there are no WebView extensions.
    match (public_key, name) {
        (Some(pk), Some(n)) => {
            eprintln!(
                "[resolve_extension_id] Using parameter-based ID: {}::{}",
                pk, n
            );
            get_extension_id_by_key_and_name(state, &pk, &n)
        }
        _ => Err(ExtensionError::ValidationError {
            reason: "Cannot identify extension: not an extension window and no public_key/name provided".to_string(),
        }),
    }
}

// ============================================================================
// Table Prefix Utilities
// ============================================================================

/// Generates the table prefix for an extension
/// Format: {public_key}__{extension_name}__
///
/// This prefix ensures table isolation between extensions.
/// Each extension can only access tables that start with this prefix.
pub fn get_extension_table_prefix(public_key: &str, extension_name: &str) -> String {
    format!("{}__{}__", public_key, extension_name)
}

/// Checks if a table name belongs to a specific extension
pub fn is_extension_table(table_name: &str, public_key: &str, extension_name: &str) -> bool {
    let prefix = get_extension_table_prefix(public_key, extension_name);
    table_name.starts_with(&prefix)
}

/// Drops all tables belonging to an extension
///
/// This function finds all tables with the extension's prefix and drops them.
/// Used when uninstalling an extension to clean up its data.
///
/// The cleanup process:
/// 1. Find all tables with the extension's prefix
/// 2. Drop CRDT triggers for each table (to prevent trigger errors)
/// 3. Remove entries from haex_crdt_dirty_tables (to prevent sync errors)
/// 4. Drop the tables themselves
///
/// # Arguments
/// * `tx` - Database transaction
/// * `public_key` - Extension's public key
/// * `extension_name` - Extension's name
///
/// # Returns
/// * `Ok(Vec<String>)` - List of dropped table names
/// * `Err` - If any DROP TABLE fails
pub fn drop_extension_tables(
    tx: &rusqlite::Transaction,
    public_key: &str,
    extension_name: &str,
) -> Result<Vec<String>, crate::database::error::DatabaseError> {
    let prefix = get_extension_table_prefix(public_key, extension_name);

    // Note: Foreign key constraints must be disabled BEFORE the transaction starts
    // (PRAGMA changes don't take effect within an active transaction)
    // The caller is responsible for setting PRAGMA foreign_keys = OFF before calling this function

    // First, clean up haex_crdt_dirty_tables for ANY tables with this prefix
    // This must happen BEFORE we query sqlite_master, because dirty_tables might
    // reference tables that were never created or have been partially created
    let dirty_pattern = format!("{}%", prefix);
    tx.execute(
        "DELETE FROM haex_crdt_dirty_tables WHERE table_name LIKE ?1",
        [&dirty_pattern],
    )?;
    println!(
        "[EXTENSION_CLEANUP] Cleaned up dirty_tables entries for prefix: {}",
        prefix
    );

    // Find all tables with this extension's prefix
    let mut stmt =
        tx.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE ?1")?;

    let pattern = format!("{}%", prefix);
    let table_names: Vec<String> = stmt
        .query_map([&pattern], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    println!(
        "[EXTENSION_CLEANUP] Found {} tables to clean up: {:?}",
        table_names.len(),
        table_names
    );

    // For each table, we need to:
    // 1. Drop CRDT triggers first
    // 2. Drop the table
    for table_name in &table_names {
        println!("[EXTENSION_CLEANUP] Cleaning up table: {}", table_name);

        // Drop CRDT triggers for this table (prevents trigger errors on table drop)
        // Trigger naming pattern: z_dirty_{TABLE_NAME}_{insert|update|delete}
        let trigger_suffixes = ["insert", "update", "delete"];
        for suffix in &trigger_suffixes {
            let trigger_name = format!("z_dirty_{}_{}", table_name, suffix);
            let drop_sql = format!("DROP TRIGGER IF EXISTS \"{}\"", trigger_name);
            println!("[EXTENSION_CLEANUP] Executing: {}", drop_sql);
            tx.execute(&drop_sql, [])?;
        }

        // Drop the table
        let drop_table_sql = format!("DROP TABLE IF EXISTS \"{}\"", table_name);
        println!("[EXTENSION_CLEANUP] Executing: {}", drop_table_sql);
        tx.execute(&drop_table_sql, []).map_err(|e| {
            eprintln!(
                "[EXTENSION_CLEANUP] ERROR dropping table {}: {}",
                table_name, e
            );
            e
        })?;
        println!(
            "[EXTENSION_CLEANUP] Successfully dropped table: {}",
            table_name
        );
    }

    if !table_names.is_empty() {
        println!(
            "[EXTENSION_CLEANUP] Dropped {} tables for extension {}::{}",
            table_names.len(),
            public_key,
            extension_name
        );
    }

    Ok(table_names)
}

/// Ed25519 public key length in bytes
const ED25519_PUBLIC_KEY_LENGTH: usize = 32;

/// Validates that a public key is a valid Ed25519 public key in hex format.
///
/// A valid public key must:
/// - Be exactly 64 hex characters (32 bytes)
/// - Contain only valid hex characters (0-9, a-f, A-F)
pub fn validate_public_key(public_key: &str) -> Result<(), ExtensionError> {
    // Check length: Ed25519 public key is 32 bytes = 64 hex characters
    if public_key.len() != ED25519_PUBLIC_KEY_LENGTH * 2 {
        return Err(ExtensionError::ValidationError {
            reason: format!(
                "Invalid public key length: expected {} hex characters, got {}",
                ED25519_PUBLIC_KEY_LENGTH * 2,
                public_key.len()
            ),
        });
    }

    // Check that it's valid hex
    if !public_key.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ExtensionError::ValidationError {
            reason: "Invalid public key: must contain only hex characters (0-9, a-f, A-F)"
                .to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_extension_table_prefix() {
        let prefix = get_extension_table_prefix("test_key", "test_extension");
        assert_eq!(prefix, "test_key__test_extension__");
    }

    #[test]
    fn test_get_extension_table_prefix_with_special_chars() {
        let prefix = get_extension_table_prefix("key-123", "my_ext.v1");
        assert_eq!(prefix, "key-123__my_ext.v1__");
    }

    #[test]
    fn test_is_extension_table_own_table() {
        assert!(is_extension_table(
            "test_key__test_ext__users",
            "test_key",
            "test_ext"
        ));
    }

    #[test]
    fn test_is_extension_table_other_extension() {
        assert!(!is_extension_table(
            "other_key__other_ext__users",
            "test_key",
            "test_ext"
        ));
    }

    #[test]
    fn test_is_extension_table_system_table() {
        assert!(!is_extension_table(
            "haex_extensions",
            "test_key",
            "test_ext"
        ));
    }

    #[test]
    fn test_is_extension_table_similar_prefix() {
        // Should not match if prefix is similar but not exact
        assert!(!is_extension_table(
            "test_key__test_ext_fake__users",
            "test_key",
            "test_ext"
        ));
    }

    #[test]
    fn test_validate_public_key_valid() {
        // Valid Ed25519 public key (64 hex chars)
        assert!(validate_public_key(
            "b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca"
        )
        .is_ok());
    }

    #[test]
    fn test_validate_public_key_valid_uppercase() {
        // Valid with uppercase hex chars
        assert!(validate_public_key(
            "B4401F13F65E576B8A30FF9FD83DF82A8BB707E1994D40C99996FE88603CEFCA"
        )
        .is_ok());
    }

    #[test]
    fn test_validate_public_key_too_short() {
        assert!(validate_public_key("abc123").is_err());
    }

    #[test]
    fn test_validate_public_key_too_long() {
        assert!(validate_public_key(
            "b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca00"
        )
        .is_err());
    }

    #[test]
    fn test_validate_public_key_invalid_chars() {
        // Contains underscores and other non-hex chars
        assert!(validate_public_key("demo_test_key_12345").is_err());
    }

    #[test]
    fn test_validate_public_key_empty() {
        assert!(validate_public_key("").is_err());
    }
}
