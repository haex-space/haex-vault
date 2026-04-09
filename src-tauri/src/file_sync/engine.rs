//! Sync engine — orchestration, execution, and periodic loop.
//!
//! Ties together providers, diff computation, and database state tracking.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value as JsonValue;
use tokio_util::sync::CancellationToken;

use crate::database::DbConnection;

use super::diff::compute_sync_actions;
use super::provider::{SyncProvider, SyncProviderError};
use super::types::{DeleteMode, SyncDirection, SyncProgress, SyncResult};

/// Get the current Unix timestamp in seconds.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SyncEngineError {
    #[error("Provider error: {0}")]
    Provider(#[from] SyncProviderError),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Cancelled")]
    Cancelled,
}

impl serde::Serialize for SyncEngineError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ---------------------------------------------------------------------------
// Sync-state DB types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SyncStateEntry {
    pub id: String,
    pub rule_id: String,
    pub relative_path: String,
    pub file_size: u64,
    pub modified_at: u64,
    pub synced_at: String,
    pub deleted: bool,
}

// ---------------------------------------------------------------------------
// Sync-state DB operations
// ---------------------------------------------------------------------------

/// Load all sync state entries for a rule.
pub fn load_sync_state(
    db: &DbConnection,
    rule_id: &str,
) -> Result<Vec<SyncStateEntry>, SyncEngineError> {
    let sql = "SELECT id, rule_id, relative_path, file_size, modified_at, synced_at, deleted FROM haex_sync_state_no_sync WHERE rule_id = ?1".to_string();
    let params = vec![JsonValue::String(rule_id.to_string())];

    let rows = crate::database::core::select(sql, params, db)
        .map_err(|e| SyncEngineError::Database(e.to_string()))?;

    let entries = rows
        .iter()
        .map(|row| SyncStateEntry {
            id: row
                .first()
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            rule_id: row
                .get(1)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            relative_path: row
                .get(2)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            file_size: row
                .get(3)
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            modified_at: row
                .get(4)
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            synced_at: row
                .get(5)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            deleted: row
                .get(6)
                .and_then(|v| v.as_i64())
                .map(|v| v != 0)
                .unwrap_or(false),
        })
        .collect();

    Ok(entries)
}

/// Insert or update a sync state entry after a file is synced.
///
/// Uses INSERT OR REPLACE on the unique `(rule_id, relative_path)` index.
pub fn upsert_sync_state(
    db: &DbConnection,
    rule_id: &str,
    relative_path: &str,
    file_size: u64,
    modified_at: u64,
) -> Result<(), SyncEngineError> {
    let now = unix_now().to_string();
    let id = uuid::Uuid::new_v4().to_string();

    // Use INSERT OR REPLACE — the unique index on (rule_id, relative_path) ensures
    // an existing row is replaced rather than duplicated.
    let sql = "INSERT OR REPLACE INTO haex_sync_state_no_sync (id, rule_id, relative_path, file_size, modified_at, synced_at, deleted) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)".to_string();
    let params = vec![
        JsonValue::String(id),
        JsonValue::String(rule_id.to_string()),
        JsonValue::String(relative_path.to_string()),
        JsonValue::Number(serde_json::Number::from(file_size)),
        JsonValue::Number(serde_json::Number::from(modified_at)),
        JsonValue::String(now),
    ];

    crate::database::core::execute(sql, params, db)
        .map_err(|e| SyncEngineError::Database(e.to_string()))?;

    Ok(())
}

/// Mark a file as deleted in sync state.
pub fn mark_deleted(
    db: &DbConnection,
    rule_id: &str,
    relative_path: &str,
) -> Result<(), SyncEngineError> {
    let now = unix_now().to_string();

    let sql = "UPDATE haex_sync_state_no_sync SET deleted = 1, synced_at = ?1 WHERE rule_id = ?2 AND relative_path = ?3".to_string();
    let params = vec![
        JsonValue::String(now),
        JsonValue::String(rule_id.to_string()),
        JsonValue::String(relative_path.to_string()),
    ];

    crate::database::core::execute(sql, params, db)
        .map_err(|e| SyncEngineError::Database(e.to_string()))?;

    Ok(())
}

/// Clear all sync state for a rule (when the rule is deleted).
pub fn clear_sync_state(db: &DbConnection, rule_id: &str) -> Result<(), SyncEngineError> {
    let sql =
        "DELETE FROM haex_sync_state_no_sync WHERE rule_id = ?1".to_string();
    let params = vec![JsonValue::String(rule_id.to_string())];

    crate::database::core::execute(sql, params, db)
        .map_err(|e| SyncEngineError::Database(e.to_string()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Execute sync
// ---------------------------------------------------------------------------

/// Execute a one-shot sync: get manifests, compute diff, transfer files, update state.
pub async fn execute_sync(
    source: &dyn SyncProvider,
    target: &dyn SyncProvider,
    direction: SyncDirection,
    delete_mode: DeleteMode,
    rule_id: &str,
    db: &DbConnection,
    app_handle: Option<&tauri::AppHandle>,
) -> Result<SyncResult, SyncEngineError> {
    // 1. Get manifests
    let source_manifest = source.manifest().await?;
    let target_manifest = target.manifest().await?;

    // 2. Compute diff
    let actions = compute_sync_actions(&source_manifest, &target_manifest, direction, delete_mode);

    let total_files = actions.to_download.len()
        + actions.to_upload.len()
        + actions.to_delete.len()
        + actions.to_create_directories.len()
        + actions.conflicts.len();
    let total_bytes: u64 = actions.to_download.iter().map(|f| f.size).sum::<u64>()
        + actions.to_upload.iter().map(|f| f.size).sum::<u64>();

    let mut result = SyncResult {
        files_downloaded: 0,
        files_deleted: 0,
        directories_created: 0,
        bytes_transferred: 0,
        conflicts_resolved: 0,
        errors: Vec::new(),
    };

    let mut files_done: u32 = 0;
    let mut bytes_done: u64 = 0;

    // Helper to emit progress
    let emit_progress =
        |app: Option<&tauri::AppHandle>, current: &str, done: u32, bytes: u64| {
            if let Some(app) = app {
                use tauri::Emitter;
                let _ = app.emit(
                    "file-sync:progress",
                    SyncProgress {
                        current_file: current.to_string(),
                        files_done: done,
                        files_total: total_files as u32,
                        bytes_done: bytes,
                        bytes_total: total_bytes,
                    },
                );
            }
        };

    // 3a. Create directories on target
    for dir_path in &actions.to_create_directories {
        emit_progress(app_handle, dir_path, files_done, bytes_done);
        match target.create_directory(dir_path).await {
            Ok(()) => result.directories_created += 1,
            Err(e) => result.errors.push(format!("mkdir {dir_path}: {e}")),
        }
        files_done += 1;
    }

    // 3b. Download files (source → target)
    for file_state in &actions.to_download {
        emit_progress(
            app_handle,
            &file_state.relative_path,
            files_done,
            bytes_done,
        );
        match source.read_file(&file_state.relative_path).await {
            Ok(data) => {
                match target
                    .write_file(&file_state.relative_path, &data)
                    .await
                {
                    Ok(()) => {
                        bytes_done += data.len() as u64;
                        result.files_downloaded += 1;
                        result.bytes_transferred += data.len() as u64;
                        if let Err(e) = upsert_sync_state(
                            db,
                            rule_id,
                            &file_state.relative_path,
                            file_state.size,
                            file_state.modified_at,
                        ) {
                            result.errors.push(format!(
                                "db upsert {}: {e}",
                                file_state.relative_path
                            ));
                        }
                    }
                    Err(e) => result.errors.push(format!(
                        "write {}: {e}",
                        file_state.relative_path
                    )),
                }
            }
            Err(e) => result
                .errors
                .push(format!("read {}: {e}", file_state.relative_path)),
        }
        files_done += 1;
    }

    // 3c. Upload files (target → source, two-way only)
    for file_state in &actions.to_upload {
        emit_progress(
            app_handle,
            &file_state.relative_path,
            files_done,
            bytes_done,
        );
        match target.read_file(&file_state.relative_path).await {
            Ok(data) => {
                match source
                    .write_file(&file_state.relative_path, &data)
                    .await
                {
                    Ok(()) => {
                        bytes_done += data.len() as u64;
                        result.bytes_transferred += data.len() as u64;
                        if let Err(e) = upsert_sync_state(
                            db,
                            rule_id,
                            &file_state.relative_path,
                            file_state.size,
                            file_state.modified_at,
                        ) {
                            result.errors.push(format!(
                                "db upsert {}: {e}",
                                file_state.relative_path
                            ));
                        }
                    }
                    Err(e) => result.errors.push(format!(
                        "write {}: {e}",
                        file_state.relative_path
                    )),
                }
            }
            Err(e) => result
                .errors
                .push(format!("read {}: {e}", file_state.relative_path)),
        }
        files_done += 1;
    }

    // 3d. Delete files
    let to_trash = matches!(delete_mode, DeleteMode::Trash);
    for path in &actions.to_delete {
        emit_progress(app_handle, path, files_done, bytes_done);
        match target.delete_file(path, to_trash).await {
            Ok(()) => {
                result.files_deleted += 1;
                if let Err(e) = mark_deleted(db, rule_id, path) {
                    result
                        .errors
                        .push(format!("db mark_deleted {path}: {e}"));
                }
            }
            Err(e) => result.errors.push(format!("delete {path}: {e}")),
        }
        files_done += 1;
    }

    // 3e. Handle conflicts — source wins, loser is renamed with .conflict.{timestamp}
    for conflict in &actions.conflicts {
        emit_progress(
            app_handle,
            &conflict.relative_path,
            files_done,
            bytes_done,
        );

        // Rename the target's version to a conflict file
        let timestamp = unix_now() as i64;
        let conflict_path = make_conflict_path(&conflict.relative_path, timestamp);

        let mut resolved = false;
        match target.read_file(&conflict.relative_path).await {
            Ok(target_data) => {
                // Write the target version as the conflict file
                if let Err(e) = target.write_file(&conflict_path, &target_data).await {
                    result.errors.push(format!(
                        "conflict rename {}: {e}",
                        conflict.relative_path
                    ));
                } else {
                    // Now overwrite target with source version
                    match source.read_file(&conflict.relative_path).await {
                        Ok(source_data) => {
                            match target
                                .write_file(&conflict.relative_path, &source_data)
                                .await
                            {
                                Ok(()) => {
                                    bytes_done += source_data.len() as u64;
                                    result.bytes_transferred += source_data.len() as u64;
                                    result.conflicts_resolved += 1;
                                    resolved = true;
                                    let _ = upsert_sync_state(
                                        db,
                                        rule_id,
                                        &conflict.relative_path,
                                        conflict.source_state.size,
                                        conflict.source_state.modified_at,
                                    );
                                }
                                Err(e) => result.errors.push(format!(
                                    "conflict write {}: {e}",
                                    conflict.relative_path
                                )),
                            }
                        }
                        Err(e) => result.errors.push(format!(
                            "conflict read source {}: {e}",
                            conflict.relative_path
                        )),
                    }
                }
            }
            Err(e) => result.errors.push(format!(
                "conflict read target {}: {e}",
                conflict.relative_path
            )),
        }

        if !resolved {
            result.errors.push(format!(
                "conflict unresolved: {}",
                conflict.relative_path
            ));
        }

        files_done += 1;
    }

    Ok(result)
}

/// Build a conflict file path: `name.conflict.{timestamp}.ext`
fn make_conflict_path(relative_path: &str, timestamp: i64) -> String {
    let path = std::path::Path::new(relative_path);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(relative_path);
    let extension = path.extension().and_then(|e| e.to_str());
    let parent = path.parent().and_then(|p| p.to_str()).unwrap_or("");

    let conflict_name = match extension {
        Some(ext) => format!("{stem}.conflict.{timestamp}.{ext}"),
        None => format!("{stem}.conflict.{timestamp}"),
    };

    if parent.is_empty() {
        conflict_name
    } else {
        format!("{parent}/{conflict_name}")
    }
}

// ---------------------------------------------------------------------------
// Update lastSyncedAt via CRDT (propagates to other devices)
// ---------------------------------------------------------------------------

fn update_last_synced_at(app: &tauri::AppHandle, rule_id: &str) {
    use tauri::Manager;
    let state = app.state::<crate::AppState>();
    let hlc = state.hlc.lock().unwrap();
    let now = unix_now();

    let sql = "UPDATE haex_sync_rules SET last_synced_at = ?1 WHERE id = ?2".to_string();
    let params = vec![
        JsonValue::Number(serde_json::Number::from(now)),
        JsonValue::String(rule_id.to_string()),
    ];

    if let Err(e) = crate::database::core::execute_with_crdt(sql, params, &state.db, &hlc) {
        eprintln!("[FileSyncEngine] Failed to update lastSyncedAt for rule {rule_id}: {e}");
    }

    // Notify frontend that CRDT dirty tables changed (triggers sync push)
    use tauri::Emitter;
    let _ = app.emit(crate::event_names::EVENT_CRDT_DIRTY_TABLES_CHANGED, ());
}

// ---------------------------------------------------------------------------
// Event emission
// ---------------------------------------------------------------------------

fn emit_sync_result(
    app: &tauri::AppHandle,
    rule_id: &str,
    result: &Result<SyncResult, SyncEngineError>,
) {
    use tauri::Emitter;
    match result {
        Ok(r) => {
            update_last_synced_at(app, rule_id);
            let _ = app.emit(
                "file-sync:complete",
                serde_json::json!({ "ruleId": rule_id, "result": r }),
            );
        }
        Err(e) => {
            let _ = app.emit(
                "file-sync:error",
                serde_json::json!({ "ruleId": rule_id, "error": e.to_string() }),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Periodic sync loop
// ---------------------------------------------------------------------------

/// Run periodic sync for a rule. Cancellable via `CancellationToken`.
///
/// The optional `trigger_receiver` allows external events (e.g. file watcher)
/// to interrupt the sleep timer and trigger an immediate sync cycle.
pub async fn run_sync_loop(
    source: Box<dyn SyncProvider>,
    target: Box<dyn SyncProvider>,
    direction: SyncDirection,
    delete_mode: DeleteMode,
    rule_id: String,
    interval: Duration,
    cancel: CancellationToken,
    mut trigger_receiver: tokio::sync::mpsc::Receiver<()>,
    db: DbConnection,
    app_handle: tauri::AppHandle,
) {
    // Run initial sync immediately
    eprintln!("[FileSyncEngine] Rule {} initial sync starting", rule_id);
    let result = execute_sync(
        &*source,
        &*target,
        direction,
        delete_mode,
        &rule_id,
        &db,
        Some(&app_handle),
    )
    .await;
    eprintln!("[FileSyncEngine] Rule {} initial sync done: {:?}", rule_id, result.as_ref().map(|r| r.files_downloaded));
    emit_sync_result(&app_handle, &rule_id, &result);

    // Manual mode (interval = 0): only sync on trigger, no periodic timer
    let use_timer = !interval.is_zero();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                eprintln!("[FileSyncEngine] Rule {} cancelled", rule_id);
                break;
            }
            _ = tokio::time::sleep(interval), if use_timer => {
                let result = execute_sync(
                    &*source,
                    &*target,
                    direction,
                    delete_mode,
                    &rule_id,
                    &db,
                    Some(&app_handle),
                )
                .await;
                emit_sync_result(&app_handle, &rule_id, &result);
            }
            _ = trigger_receiver.recv() => {
                // Drain any additional pending triggers to avoid redundant syncs
                while trigger_receiver.try_recv().is_ok() {}
                let result = execute_sync(
                    &*source,
                    &*target,
                    direction,
                    delete_mode,
                    &rule_id,
                    &db,
                    Some(&app_handle),
                )
                .await;
                emit_sync_result(&app_handle, &rule_id, &result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_path_with_extension() {
        let result = make_conflict_path("docs/report.pdf", 1700000000);
        assert_eq!(result, "docs/report.conflict.1700000000.pdf");
    }

    #[test]
    fn conflict_path_without_extension() {
        let result = make_conflict_path("Makefile", 1700000000);
        assert_eq!(result, "Makefile.conflict.1700000000");
    }

    #[test]
    fn conflict_path_root_level() {
        let result = make_conflict_path("file.txt", 1700000000);
        assert_eq!(result, "file.conflict.1700000000.txt");
    }
}
