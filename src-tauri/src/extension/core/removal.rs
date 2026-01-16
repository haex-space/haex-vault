// src-tauri/src/extension/core/removal.rs
//
// Extension removal logic.

use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::extension::database::executor::SqlExecutor;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::utils::drop_extension_tables;
use crate::table_names::TABLE_EXTENSIONS;
use crate::AppState;
use tauri::{AppHandle, State};

use super::manager::ExtensionManager;

impl ExtensionManager {
    /// Removes an extension from the system
    ///
    /// # Arguments
    /// * `app_handle` - Tauri app handle
    /// * `public_key` - Extension's public key
    /// * `extension_name` - Extension name
    /// * `extension_version` - Extension version
    /// * `delete_data` - If true, deletes all extension tables and data. If false, only removes the extension entry (data persists for sync).
    /// * `state` - App state
    pub async fn remove_extension_internal(
        &self,
        app_handle: &AppHandle,
        public_key: &str,
        extension_name: &str,
        extension_version: &str,
        delete_data: bool,
        state: &State<'_, AppState>,
    ) -> Result<(), ExtensionError> {
        // Get the extension from memory to get its ID
        let extension = self
            .get_extension_by_public_key_and_name(public_key, extension_name)?
            .ok_or_else(|| ExtensionError::NotFound {
                public_key: public_key.to_string(),
                name: extension_name.to_string(),
            })?;

        eprintln!("DEBUG: Removing extension with ID: {}", extension.id);
        eprintln!(
            "DEBUG: Extension name: {extension_name}, version: {extension_version}, delete_data: {delete_data}"
        );

        // Only delete DB entries if delete_data is true (complete removal)
        // For updates (delete_data=false), we keep the DB entry and permissions
        if delete_data {
            // Delete permissions and extension entry in a transaction
            with_connection(&state.db, |conn| {
                // Disable foreign key constraints BEFORE starting the transaction
                // (PRAGMA changes don't take effect within an active transaction)
                conn.execute("PRAGMA foreign_keys = OFF", [])
                    .map_err(DatabaseError::from)?;

                let tx = conn.transaction().map_err(DatabaseError::from)?;

                let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

                // Delete all permissions for this extension
                eprintln!(
                    "DEBUG: Deleting permissions for extension_id: {}",
                    extension.id
                );
                PermissionManager::delete_permissions_in_transaction(
                    &tx,
                    &hlc_service,
                    &extension.id,
                )?;

                // Drop all tables belonging to this extension
                eprintln!(
                    "DEBUG: Dropping tables for extension {}::{}",
                    public_key, extension_name
                );
                let dropped_tables = drop_extension_tables(&tx, public_key, extension_name)?;
                if !dropped_tables.is_empty() {
                    eprintln!("DEBUG: Dropped tables: {:?}", dropped_tables);
                }

                // First disable the extension before deleting (tombstoning)
                // This ensures the extension won't be loaded even if tombstone filter fails
                let disable_sql = format!("UPDATE {TABLE_EXTENSIONS} SET enabled = 0 WHERE id = ?");
                eprintln!(
                    "DEBUG: Disabling extension before delete: {} with id = {}",
                    disable_sql, extension.id
                );
                SqlExecutor::execute_internal_typed(
                    &tx,
                    &hlc_service,
                    &disable_sql,
                    rusqlite::params![&extension.id],
                )?;

                // Delete extension entry (will be transformed to tombstone)
                let sql = format!("DELETE FROM {TABLE_EXTENSIONS} WHERE id = ?");
                eprintln!("DEBUG: Executing SQL: {} with id = {}", sql, extension.id);
                SqlExecutor::execute_internal_typed(
                    &tx,
                    &hlc_service,
                    &sql,
                    rusqlite::params![&extension.id],
                )?;

                eprintln!("DEBUG: Committing transaction");
                let commit_result = tx.commit().map_err(DatabaseError::from);

                // Re-enable foreign key constraints after transaction
                conn.execute("PRAGMA foreign_keys = ON", [])
                    .map_err(DatabaseError::from)?;

                commit_result
            })?;

            eprintln!("DEBUG: Transaction committed successfully");
        } else {
            eprintln!(
                "DEBUG: Keeping DB entry and permissions (delete_data=false, update mode)"
            );
        }

        // Remove from in-memory manager
        self.remove_extension(public_key, extension_name)?;

        // Delete only the specific version folder: public_key/name/version
        let extension_dir =
            self.get_extension_dir(app_handle, public_key, extension_name, extension_version)?;

        if extension_dir.exists() {
            std::fs::remove_dir_all(&extension_dir).map_err(|e| {
                ExtensionError::filesystem_with_path(extension_dir.display().to_string(), e)
            })?;

            // Try to delete empty parent folders
            // 1. Extension name folder (key_hash/name)
            if let Some(name_dir) = extension_dir.parent() {
                if name_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(name_dir) {
                        if entries.count() == 0 {
                            let _ = std::fs::remove_dir(name_dir);

                            // 2. Key hash folder (key_hash) - only if also empty
                            if let Some(key_hash_dir) = name_dir.parent() {
                                if key_hash_dir.exists() {
                                    if let Ok(entries) = std::fs::read_dir(key_hash_dir) {
                                        if entries.count() == 0 {
                                            let _ = std::fs::remove_dir(key_hash_dir);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
