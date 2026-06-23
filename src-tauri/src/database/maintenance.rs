use super::*;

use crate::database::error::DatabaseError;
use crate::AppState;
use tauri::State;

/// Cleans up the delete-log by hard-deleting entries older than the retention
/// period. `retention_days == 0` clears the full delete-log.
#[tauri::command]
pub fn crdt_cleanup_deleted_rows(
    retention_days: u32,
    state: State<'_, AppState>,
) -> Result<crate::crdt::cleanup::CleanupResult, DatabaseError> {
    core::with_connection(&state.db, |conn| {
        crate::crdt::cleanup::cleanup_deleted_rows(conn, retention_days).map_err(|e| {
            DatabaseError::ExecutionError {
                sql: "CRDT cleanup".to_string(),
                reason: e.to_string(),
                table: None,
            }
        })
    })
}

/// Gets statistics about CRDT tables (total entries, tombstoned entries, etc.)
#[tauri::command]
pub fn crdt_get_stats(
    state: State<'_, AppState>,
) -> Result<crate::crdt::cleanup::CrdtStats, DatabaseError> {
    core::with_connection(&state.db, |conn| {
        crate::crdt::cleanup::get_crdt_stats(conn).map_err(|e| DatabaseError::ExecutionError {
            sql: "CRDT stats".to_string(),
            reason: e.to_string(),
            table: None,
        })
    })
}

/// Runs SQLite VACUUM command to reclaim disk space
#[tauri::command]
pub fn database_vacuum(state: State<'_, AppState>) -> Result<String, DatabaseError> {
    core::with_connection(&state.db, |conn| {
        conn.execute("VACUUM", [])
            .map_err(|e| DatabaseError::ExecutionError {
                sql: "VACUUM".to_string(),
                reason: e.to_string(),
                table: None,
            })?;
        Ok("Database vacuumed successfully".to_string())
    })
}

/// Changes the vault password using SQLCipher's rekey functionality.
/// This re-encrypts the entire database with the new password.
///
/// IMPORTANT: The old password must be correct (database must already be open).
/// After this operation, the database will be encrypted with the new password.
///
/// NOTE: SQLCipher's rekey does NOT work in WAL mode. We must:
/// 1. Checkpoint and switch to DELETE journal mode
/// 2. Perform the rekey
/// 3. Switch back to WAL mode
#[tauri::command]
pub fn change_vault_password(
    new_password: String,
    state: State<'_, AppState>,
) -> Result<String, DatabaseError> {
    core::with_connection(&state.db, |conn| {
        println!("[REKEY] Starting vault password change...");

        // Step 1: Checkpoint the WAL file to ensure all data is in the main database
        println!("[REKEY] Checkpointing WAL file (TRUNCATE mode)...");
        conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")
            .map_err(|e| DatabaseError::PragmaError {
                pragma: "wal_checkpoint".to_string(),
                reason: e.to_string(),
            })?;

        // Step 2: Switch from WAL to DELETE journal mode
        // This is required because rekey does not work properly in WAL mode
        println!("[REKEY] Switching to DELETE journal mode...");
        let _: String = conn
            .pragma_update_and_check(None, "journal_mode", "DELETE", |row| row.get(0))
            .map_err(|e| DatabaseError::PragmaError {
                pragma: "journal_mode=DELETE".to_string(),
                reason: e.to_string(),
            })?;

        // Step 3: Use PRAGMA rekey to change the encryption key
        // This re-encrypts the entire database with the new key
        println!("[REKEY] Executing rekey with new password...");
        conn.pragma_update(None, "rekey", &new_password)
            .map_err(|e| DatabaseError::PragmaError {
                pragma: "rekey".to_string(),
                reason: e.to_string(),
            })?;

        // Step 4: Switch back to WAL mode for better performance
        println!("[REKEY] Switching back to WAL journal mode...");
        let _: String = conn
            .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
            .map_err(|e| DatabaseError::PragmaError {
                pragma: "journal_mode=WAL".to_string(),
                reason: e.to_string(),
            })?;

        println!("✅ Vault password changed successfully via SQLCipher rekey");
        Ok("Vault password changed successfully".to_string())
    })
}
