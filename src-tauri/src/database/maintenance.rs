use super::*;

use crate::database::error::DatabaseError;
use crate::AppState;
use serde::Serialize;
use tauri::State;
use ts_rs::TS;

/// Result of the delete-log cleanup operation. Thin vault-side mirror of
/// `haex_crdt::CleanupResult` — the crate's type does not derive `ts-rs::TS`,
/// so we redeclare the shape here to keep the frontend binding auto-generated.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    /// Number of rows removed from `haex_deleted_rows`.
    pub rows_deleted: usize,
    /// Max HLC of the entries that were pruned. `None` when the pass was a
    /// no-op (no entries matched the policy).
    pub max_pruned_hlc: Option<String>,
}

impl From<haex_crdt::CleanupResult> for CleanupResult {
    fn from(v: haex_crdt::CleanupResult) -> Self {
        Self {
            rows_deleted: v.rows_deleted,
            max_pruned_hlc: v.max_pruned_hlc,
        }
    }
}

/// Snapshot of the CRDT layer's contents. Thin vault-side mirror of
/// `haex_crdt::CrdtStats` — see `CleanupResult` for the ts-rs rationale.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CrdtStats {
    /// Live rows across every CRDT-managed table (identified by the presence
    /// of the row-level HLC column). Excludes `_no_sync` tables, SQLite
    /// internals, and the delete-log table itself.
    pub live_row_count: i64,
    /// Number of CRDT-managed tables discovered.
    pub crdt_table_count: i64,
    /// Rows currently in `haex_deleted_rows`.
    pub delete_log_row_count: i64,
}

impl From<haex_crdt::CrdtStats> for CrdtStats {
    fn from(v: haex_crdt::CrdtStats) -> Self {
        Self {
            live_row_count: v.live_row_count,
            crdt_table_count: v.crdt_table_count,
            delete_log_row_count: v.delete_log_row_count,
        }
    }
}

/// Cleans up the delete-log by hard-deleting entries older than the retention
/// period. `retention_days == 0` clears the full delete-log
/// (`RetentionPolicy::All`); a positive value maps to
/// `RetentionPolicy::TimeBasedDays { days }`.
///
/// Wraps `haex_crdt::cleanup_deleted_rows` and, inside the crate's
/// `before_prune` closure, runs the vault-owned anchor advances so the
/// crate-side owner-domain prune and the vault-side shared-space prune all
/// commit atomically in the same transaction.
#[tauri::command]
pub fn crdt_cleanup_deleted_rows(
    retention_days: u32,
    state: State<'_, AppState>,
) -> Result<CleanupResult, DatabaseError> {
    let policy = if retention_days == 0 {
        haex_crdt::RetentionPolicy::All
    } else {
        haex_crdt::RetentionPolicy::TimeBasedDays {
            days: retention_days,
        }
    };
    core::with_connection(&state.db, |conn| {
        let result = haex_crdt::cleanup_deleted_rows(conn, policy, |tx, owner_max_hlc| {
            // Owner-domain anchor advances first (using the max HLC of
            // rows the crate is about to prune from haex_deleted_rows),
            // then the vault-only shared-space delete-log gets pruned +
            // its per-space anchors advanced. Both writes share the crate's
            // transaction so a crash mid-way leaves the anchor set at most
            // as strict as the surviving entries (ADR 0002 §6.5).
            if let Some(hlc) = owner_max_hlc {
                crate::crdt::compaction_anchor::advance_owner_delete_log_anchor(&*tx, hlc)?;
            }
            crate::crdt::compaction_anchor::prune_shared_space_delete_log_and_advance_anchors(
                tx, policy,
            )?;
            Ok(())
        })?;
        Ok(CleanupResult::from(result))
    })
}

/// Gets statistics about CRDT tables (total entries, tombstoned entries, etc.)
#[tauri::command]
pub fn crdt_get_stats(state: State<'_, AppState>) -> Result<CrdtStats, DatabaseError> {
    core::with_connection(&state.db, |conn| {
        let stats = haex_crdt::get_crdt_stats(conn)?;
        Ok(CrdtStats::from(stats))
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
