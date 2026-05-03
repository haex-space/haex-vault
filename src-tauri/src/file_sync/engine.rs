//! Sync engine — orchestration, execution, and periodic loop.
//!
//! Ties together providers, diff computation, and database state tracking.

use std::collections::{HashMap, VecDeque};
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value as JsonValue;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::database::DbConnection;

use super::diff::compute_sync_actions;
use super::provider::{SyncProvider, SyncProviderError};
use super::types::{DeleteMode, SyncDirection, SyncResult};

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
// Speed tracker — sliding window bytes/second
// ---------------------------------------------------------------------------

struct SpeedTracker {
    samples: VecDeque<(Instant, u64)>,
}

impl SpeedTracker {
    fn new() -> Self {
        Self { samples: VecDeque::new() }
    }

    fn add(&mut self, bytes: u64) {
        self.samples.push_back((Instant::now(), bytes));
        let cutoff = Instant::now() - Duration::from_secs(5);
        while self.samples.front().map(|(t, _)| *t < cutoff).unwrap_or(false) {
            self.samples.pop_front();
        }
    }

    fn bytes_per_second(&self) -> u64 {
        if self.samples.len() < 2 {
            return 0;
        }
        let oldest = self.samples.front().unwrap().0;
        let newest = self.samples.back().unwrap().0;
        let elapsed = newest.duration_since(oldest).as_secs_f64();
        let total: u64 = self.samples.iter().map(|(_, b)| b).sum();
        if elapsed < 0.05 {
            return 0;
        }
        (total as f64 / elapsed) as u64
    }
}

// ---------------------------------------------------------------------------
// Execute sync
// ---------------------------------------------------------------------------

/// How many files to transfer in parallel. Higher values help on fast LAN
/// connections with many small files; lower values reduce memory pressure for
/// large files. 4 is a safe default for both LAN and WAN.
const TRANSFER_CONCURRENCY: usize = 4;

/// Minimum interval between two `file-sync:progress` Tauri events. Per-chunk
/// callbacks would otherwise fire dozens of times per second per active
/// transfer; the IPC + JSON cost competes with the streaming I/O loop.
const PROGRESS_EMIT_INTERVAL_MS: u64 = 100;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Execute a one-shot sync: get manifests, compute diff, transfer files in
/// parallel, update state.
pub async fn execute_sync(
    source: Arc<dyn SyncProvider>,
    target: Arc<dyn SyncProvider>,
    direction: SyncDirection,
    delete_mode: DeleteMode,
    rule_id: &str,
    db: &DbConnection,
    app_handle: Option<tauri::AppHandle>,
) -> Result<SyncResult, SyncEngineError> {
    // 1. Get manifests (sequential — each is a single network roundtrip)
    let source_manifest = source.manifest().await?;
    let target_manifest = target.manifest().await?;

    // 2. Compute diff
    let actions = compute_sync_actions(&source_manifest, &target_manifest, direction, delete_mode);

    let total_files = (actions.to_download.len()
        + actions.to_upload.len()
        + actions.to_delete.len()
        + actions.to_create_directories.len()
        + actions.conflicts.len()) as u32;
    let total_bytes: u64 = actions.to_download.iter().map(|f| f.size).sum::<u64>()
        + actions.to_upload.iter().map(|f| f.size).sum::<u64>()
        + actions.conflicts.iter().map(|c| c.source_state.size).sum::<u64>();

    // 3. Shared progress counters (atomics for concurrent access from tasks)
    let files_done = Arc::new(AtomicU32::new(0));
    let bytes_done = Arc::new(AtomicU64::new(0));
    // Active files tracked with a monotonic insertion sequence so display order
    // stays stable: each path keeps its slot from start to finish, instead of
    // re-sorting alphabetically every emit (which makes the list jump around
    // when files start/complete in parallel).
    let active_seq = Arc::new(AtomicU64::new(0));
    let active_files: Arc<Mutex<Vec<(u64, String)>>> = Arc::new(Mutex::new(Vec::new()));
    // Per-file progress: path → (bytes_done, bytes_total)
    let file_progress: Arc<Mutex<HashMap<String, (u64, u64)>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let speed_tracker: Arc<Mutex<SpeedTracker>> = Arc::new(Mutex::new(SpeedTracker::new()));
    // Throttle: per-chunk progress callbacks fire many times per second per
    // active transfer. Emitting a Tauri event for each one (with JSON
    // serialization + IPC) starves the streaming I/O loop. Coalesce to at
    // most one emit every PROGRESS_EMIT_INTERVAL_MS.
    let last_emit_ms: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));

    // Result accumulators
    let files_downloaded = Arc::new(AtomicU32::new(0));
    let files_deleted = Arc::new(AtomicU32::new(0));
    let directories_created = Arc::new(AtomicU32::new(0));
    let bytes_transferred = Arc::new(AtomicU64::new(0));
    let conflicts_resolved = Arc::new(AtomicU32::new(0));
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // Semaphore caps the number of in-flight file transfers
    let semaphore = Arc::new(tokio::sync::Semaphore::new(TRANSFER_CONCURRENCY));

    // Progress emitter — clones all shared state and emits a JSON event.
    // `force=false` (default) throttles to PROGRESS_EMIT_INTERVAL_MS; lifecycle
    // events (file start/end, dir create, etc.) pass `force=true` so important
    // transitions are never dropped.
    let rule_id_str = rule_id.to_string();
    let emit_progress: Arc<dyn Fn(bool) + Send + Sync> = {
        let files_done = files_done.clone();
        let bytes_done = bytes_done.clone();
        let active_files = active_files.clone();
        let file_progress = file_progress.clone();
        let speed_tracker = speed_tracker.clone();
        let last_emit_ms = last_emit_ms.clone();
        let app = app_handle.clone();
        let rule_id_str = rule_id_str.clone();

        Arc::new(move |force: bool| {
            let Some(ref app) = app else { return };
            use tauri::Emitter;
            if !force {
                let now = now_ms();
                let prev = last_emit_ms.load(Ordering::Relaxed);
                if now.saturating_sub(prev) < PROGRESS_EMIT_INTERVAL_MS {
                    return;
                }
                if last_emit_ms
                    .compare_exchange(prev, now, Ordering::Relaxed, Ordering::Relaxed)
                    .is_err()
                {
                    return;
                }
            } else {
                last_emit_ms.store(now_ms(), Ordering::Relaxed);
            }

            let done = files_done.load(Ordering::Relaxed);
            let committed = bytes_done.load(Ordering::Relaxed);
            // Snapshot active files in insertion order (sequence number sort
            // is monotonic and stable, so each entry keeps its slot until it
            // completes — the list does not reshuffle as new files start).
            let mut active_pairs: Vec<(u64, String)> = active_files
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            active_pairs.sort_by_key(|(seq, _)| *seq);
            let fp = file_progress.lock().unwrap_or_else(|e| e.into_inner());
            // Include in-progress bytes so the bar fills as chunks arrive,
            // not only when entire files complete.
            let in_progress: u64 = fp.values().map(|(done, _)| *done).sum();
            let bytes = committed + in_progress;
            let active: Vec<serde_json::Value> = active_pairs
                .iter()
                .map(|(_, path)| {
                    let (fd, ft) = fp.get(path).copied().unwrap_or((0, 0));
                    serde_json::json!({
                        "path": path,
                        "bytesDone": fd,
                        "bytesTotal": ft,
                    })
                })
                .collect();
            drop(fp);
            let speed = speed_tracker
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .bytes_per_second();
            let current = active_pairs
                .first()
                .map(|(_, p)| p.clone())
                .unwrap_or_default();
            let _ = app.emit(
                "file-sync:progress",
                serde_json::json!({
                    "ruleId": rule_id_str,
                    "currentFile": current,
                    "filesDone": done,
                    "filesTotal": total_files,
                    "bytesDone": bytes,
                    "bytesTotal": total_bytes,
                    "activeFiles": active,
                    "bytesPerSecond": speed,
                }),
            );
        })
    };

    // -------------------------------------------------------------------------
    // 3a. Create directories (sequential — cheap, order matters)
    // -------------------------------------------------------------------------
    for dir_path in &actions.to_create_directories {
        let seq = active_seq.fetch_add(1, Ordering::Relaxed);
        active_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((seq, dir_path.clone()));
        emit_progress(true);
        match target.create_directory(dir_path).await {
            Ok(()) => { directories_created.fetch_add(1, Ordering::Relaxed); }
            Err(e) => {
                errors
                    .lock()
                    .unwrap_or_else(|e2| e2.into_inner())
                    .push(format!("mkdir {dir_path}: {e}"));
            }
        }
        active_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(_, p)| p != dir_path);
        files_done.fetch_add(1, Ordering::Relaxed);
        emit_progress(true);
    }

    // -------------------------------------------------------------------------
    // 3b. Download files (source → target) — parallel
    // -------------------------------------------------------------------------
    {
        let mut join_set: JoinSet<()> = JoinSet::new();

        for file in actions.to_download {
            let source = source.clone();
            let target = target.clone();
            let sem = semaphore.clone();
            let files_done = files_done.clone();
            let bytes_done = bytes_done.clone();
            let bytes_transferred = bytes_transferred.clone();
            let files_downloaded = files_downloaded.clone();
            let active_files = active_files.clone();
            let active_seq = active_seq.clone();
            let file_progress = file_progress.clone();
            let speed_tracker = speed_tracker.clone();
            let errors = errors.clone();
            let db_clone = DbConnection(db.0.clone());
            let rule_id_clone = rule_id_str.clone();
            let emit = emit_progress.clone();

            join_set.spawn(async move {
                let _permit = sem.acquire().await.unwrap();

                // Register per-file progress entry
                file_progress
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(file.relative_path.clone(), (0, file.size));

                let seq = active_seq.fetch_add(1, Ordering::Relaxed);
                active_files
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push((seq, file.relative_path.clone()));
                emit(true);

                // Build progress callback: updates per-file map and speed tracker per chunk.
                let fp_cb = file_progress.clone();
                let path_cb = file.relative_path.clone();
                let emit_cb = emit.clone();
                let speed_cb = speed_tracker.clone();
                let last_chunk = Arc::new(std::sync::atomic::AtomicU64::new(0));
                let last_chunk_cb = last_chunk.clone();
                let progress_cb: Arc<dyn Fn(u64, u64) + Send + Sync> =
                    Arc::new(move |done, total| {
                        fp_cb
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .insert(path_cb.clone(), (done, total));
                        let prev = last_chunk_cb.swap(done, std::sync::atomic::Ordering::Relaxed);
                        let delta = done.saturating_sub(prev);
                        if delta > 0 {
                            speed_cb.lock().unwrap_or_else(|e| e.into_inner()).add(delta);
                        }
                        emit_cb(false);
                    });

                // Temp file as staging area — provider streams directly to disk,
                // so no full-file buffer in RAM even for multi-GB files.
                let tmp: tempfile::NamedTempFile = match tempfile::NamedTempFile::new() {
                    Ok(f) => f,
                    Err(e) => {
                        active_files
                            .lock()
                            .unwrap_or_else(|e2| e2.into_inner())
                            .retain(|(_, p)| p != &file.relative_path);
                        file_progress
                            .lock()
                            .unwrap_or_else(|e2| e2.into_inner())
                            .remove(&file.relative_path);
                        errors
                            .lock()
                            .unwrap_or_else(|e2| e2.into_inner())
                            .push(format!("tmpfile {}: {e}", file.relative_path));
                        files_done.fetch_add(1, Ordering::Relaxed);
                        emit(true);
                        return;
                    }
                };

                // Transfer with one retry on failure
                let read_result = source
                    .read_file_to_path(&file.relative_path, tmp.path(), progress_cb.clone())
                    .await;
                let read_result = if read_result.is_err() {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    // Reset per-file counter and chunk baseline before retry
                    file_progress
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(file.relative_path.clone(), (0, file.size));
                    last_chunk.store(0, std::sync::atomic::Ordering::Relaxed);
                    source
                        .read_file_to_path(&file.relative_path, tmp.path(), progress_cb)
                        .await
                } else {
                    read_result
                };

                let res: Result<u64, SyncProviderError> = match read_result {
                    Ok(n) => target
                        .write_file_from_path(&file.relative_path, tmp.path())
                        .await
                        .map(|_| n),
                    Err(e) => Err(e),
                };

                active_files
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .retain(|(_, p)| p != &file.relative_path);
                file_progress
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&file.relative_path);

                match res {
                    Ok(n) => {
                        bytes_done.fetch_add(n, Ordering::Relaxed);
                        bytes_transferred.fetch_add(n, Ordering::Relaxed);
                        files_downloaded.fetch_add(1, Ordering::Relaxed);
                        // Speed tracker already fed per-chunk in progress_cb; no add here.
                        if let Err(e) = upsert_sync_state(
                            &db_clone,
                            &rule_id_clone,
                            &file.relative_path,
                            file.size,
                            file.modified_at,
                        ) {
                            errors
                                .lock()
                                .unwrap_or_else(|e2| e2.into_inner())
                                .push(format!("db upsert {}: {e}", file.relative_path));
                        }
                    }
                    Err(e) => {
                        errors
                            .lock()
                            .unwrap_or_else(|e2| e2.into_inner())
                            .push(format!("transfer {}: {e}", file.relative_path));
                    }
                }

                files_done.fetch_add(1, Ordering::Relaxed);
                emit(true);
            });
        }

        while join_set.join_next().await.is_some() {}
    }

    // -------------------------------------------------------------------------
    // 3c. Upload files (target → source) — parallel (two-way only)
    // -------------------------------------------------------------------------
    {
        let mut join_set: JoinSet<()> = JoinSet::new();

        for file in actions.to_upload {
            let source = source.clone();
            let target = target.clone();
            let sem = semaphore.clone();
            let files_done = files_done.clone();
            let bytes_done = bytes_done.clone();
            let bytes_transferred = bytes_transferred.clone();
            let active_files = active_files.clone();
            let active_seq = active_seq.clone();
            let file_progress = file_progress.clone();
            let speed_tracker = speed_tracker.clone();
            let errors = errors.clone();
            let db_clone = DbConnection(db.0.clone());
            let rule_id_clone = rule_id_str.clone();
            let emit = emit_progress.clone();

            join_set.spawn(async move {
                let _permit = sem.acquire().await.unwrap();

                file_progress
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(file.relative_path.clone(), (0, file.size));

                let seq = active_seq.fetch_add(1, Ordering::Relaxed);
                active_files
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push((seq, file.relative_path.clone()));
                emit(true);

                let fp_cb = file_progress.clone();
                let path_cb = file.relative_path.clone();
                let emit_cb = emit.clone();
                let speed_cb = speed_tracker.clone();
                let last_chunk = Arc::new(std::sync::atomic::AtomicU64::new(0));
                let last_chunk_cb = last_chunk.clone();
                let progress_cb: Arc<dyn Fn(u64, u64) + Send + Sync> =
                    Arc::new(move |done, total| {
                        fp_cb
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .insert(path_cb.clone(), (done, total));
                        let prev = last_chunk_cb.swap(done, std::sync::atomic::Ordering::Relaxed);
                        let delta = done.saturating_sub(prev);
                        if delta > 0 {
                            speed_cb.lock().unwrap_or_else(|e| e.into_inner()).add(delta);
                        }
                        emit_cb(false);
                    });

                let tmp: tempfile::NamedTempFile = match tempfile::NamedTempFile::new() {
                    Ok(f) => f,
                    Err(e) => {
                        active_files
                            .lock()
                            .unwrap_or_else(|e2| e2.into_inner())
                            .retain(|(_, p)| p != &file.relative_path);
                        file_progress
                            .lock()
                            .unwrap_or_else(|e2| e2.into_inner())
                            .remove(&file.relative_path);
                        errors
                            .lock()
                            .unwrap_or_else(|e2| e2.into_inner())
                            .push(format!("tmpfile {}: {e}", file.relative_path));
                        files_done.fetch_add(1, Ordering::Relaxed);
                        emit(true);
                        return;
                    }
                };

                let read_result = target
                    .read_file_to_path(&file.relative_path, tmp.path(), progress_cb.clone())
                    .await;
                let read_result = if read_result.is_err() {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    file_progress
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(file.relative_path.clone(), (0, file.size));
                    last_chunk.store(0, std::sync::atomic::Ordering::Relaxed);
                    target
                        .read_file_to_path(&file.relative_path, tmp.path(), progress_cb)
                        .await
                } else {
                    read_result
                };

                let res: Result<u64, SyncProviderError> = match read_result {
                    Ok(n) => source
                        .write_file_from_path(&file.relative_path, tmp.path())
                        .await
                        .map(|_| n),
                    Err(e) => Err(e),
                };

                active_files
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .retain(|(_, p)| p != &file.relative_path);
                file_progress
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&file.relative_path);

                match res {
                    Ok(n) => {
                        bytes_done.fetch_add(n, Ordering::Relaxed);
                        bytes_transferred.fetch_add(n, Ordering::Relaxed);
                        // Speed tracker already fed per-chunk in progress_cb; no add here.
                        if let Err(e) = upsert_sync_state(
                            &db_clone,
                            &rule_id_clone,
                            &file.relative_path,
                            file.size,
                            file.modified_at,
                        ) {
                            errors
                                .lock()
                                .unwrap_or_else(|e2| e2.into_inner())
                                .push(format!("db upsert {}: {e}", file.relative_path));
                        }
                    }
                    Err(e) => {
                        errors
                            .lock()
                            .unwrap_or_else(|e2| e2.into_inner())
                            .push(format!("upload {}: {e}", file.relative_path));
                    }
                }

                files_done.fetch_add(1, Ordering::Relaxed);
                emit(true);
            });
        }

        while join_set.join_next().await.is_some() {}
    }

    // -------------------------------------------------------------------------
    // 3d. Delete files (sequential — order can matter for directories)
    // -------------------------------------------------------------------------
    let to_trash = matches!(delete_mode, DeleteMode::Trash);
    for path in &actions.to_delete {
        let seq = active_seq.fetch_add(1, Ordering::Relaxed);
        active_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((seq, path.clone()));
        emit_progress(true);
        match target.delete_file(path, to_trash).await {
            Ok(()) => {
                files_deleted.fetch_add(1, Ordering::Relaxed);
                if let Err(e) = mark_deleted(db, rule_id, path) {
                    errors
                        .lock()
                        .unwrap_or_else(|e2| e2.into_inner())
                        .push(format!("db mark_deleted {path}: {e}"));
                }
            }
            Err(e) => {
                errors
                    .lock()
                    .unwrap_or_else(|e2| e2.into_inner())
                    .push(format!("delete {path}: {e}"));
            }
        }
        active_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(_, p)| p != path);
        files_done.fetch_add(1, Ordering::Relaxed);
        emit_progress(true);
    }

    // -------------------------------------------------------------------------
    // 3e. Conflicts — source wins, target version renamed with .conflict.{ts}
    //     (sequential: each conflict is a multi-step read/write sequence)
    // -------------------------------------------------------------------------
    for conflict in &actions.conflicts {
        let seq = active_seq.fetch_add(1, Ordering::Relaxed);
        active_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((seq, conflict.relative_path.clone()));
        emit_progress(true);

        let timestamp = unix_now() as i64;
        let conflict_path = make_conflict_path(&conflict.relative_path, timestamp);
        let mut resolved = false;

        match target.read_file(&conflict.relative_path).await {
            Ok(target_data) => {
                if let Err(e) = target.write_file(&conflict_path, &target_data).await {
                    errors
                        .lock()
                        .unwrap_or_else(|e2| e2.into_inner())
                        .push(format!("conflict rename {}: {e}", conflict.relative_path));
                } else {
                    match source.read_file(&conflict.relative_path).await {
                        Ok(source_data) => {
                            match target
                                .write_file(&conflict.relative_path, &source_data)
                                .await
                            {
                                Ok(()) => {
                                    let n = source_data.len() as u64;
                                    bytes_done.fetch_add(n, Ordering::Relaxed);
                                    bytes_transferred.fetch_add(n, Ordering::Relaxed);
                                    speed_tracker
                                        .lock()
                                        .unwrap_or_else(|e| e.into_inner())
                                        .add(n);
                                    conflicts_resolved.fetch_add(1, Ordering::Relaxed);
                                    resolved = true;
                                    let _ = upsert_sync_state(
                                        db,
                                        rule_id,
                                        &conflict.relative_path,
                                        conflict.source_state.size,
                                        conflict.source_state.modified_at,
                                    );
                                }
                                Err(e) => {
                                    errors
                                        .lock()
                                        .unwrap_or_else(|e2| e2.into_inner())
                                        .push(format!(
                                            "conflict write {}: {e}",
                                            conflict.relative_path
                                        ));
                                }
                            }
                        }
                        Err(e) => {
                            errors
                                .lock()
                                .unwrap_or_else(|e2| e2.into_inner())
                                .push(format!(
                                    "conflict read source {}: {e}",
                                    conflict.relative_path
                                ));
                        }
                    }
                }
            }
            Err(e) => {
                errors
                    .lock()
                    .unwrap_or_else(|e2| e2.into_inner())
                    .push(format!("conflict read target {}: {e}", conflict.relative_path));
            }
        }

        if !resolved {
            errors
                .lock()
                .unwrap_or_else(|e2| e2.into_inner())
                .push(format!("conflict unresolved: {}", conflict.relative_path));
        }

        active_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(_, p)| p != &conflict.relative_path);
        files_done.fetch_add(1, Ordering::Relaxed);
        emit_progress(true);
    }

    let errors_vec = errors.lock().unwrap_or_else(|e| e.into_inner()).clone();
    Ok(SyncResult {
        files_downloaded: files_downloaded.load(Ordering::Relaxed),
        files_deleted: files_deleted.load(Ordering::Relaxed),
        directories_created: directories_created.load(Ordering::Relaxed),
        bytes_transferred: bytes_transferred.load(Ordering::Relaxed),
        conflicts_resolved: conflicts_resolved.load(Ordering::Relaxed),
        errors: errors_vec,
    })
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

/// How long to wait before retrying after a failed sync cycle.
const RETRY_INTERVAL: Duration = Duration::from_secs(30);

/// Run periodic sync for a rule. Cancellable via `CancellationToken`.
///
/// The optional `trigger_receiver` allows external events (e.g. file watcher)
/// to interrupt the sleep timer and trigger an immediate sync cycle.
/// On failure the next attempt is scheduled after `RETRY_INTERVAL` (30 s)
/// instead of the full `interval`, so peer unavailability self-heals quickly.
pub async fn run_sync_loop(
    source: Arc<dyn SyncProvider>,
    target: Arc<dyn SyncProvider>,
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
        source.clone(),
        target.clone(),
        direction,
        delete_mode,
        &rule_id,
        &db,
        Some(app_handle.clone()),
    )
    .await;
    eprintln!("[FileSyncEngine] Rule {} initial sync done: {:?}", rule_id, result.as_ref().map(|r| r.files_downloaded));

    // Retry sooner if the initial sync failed
    let mut next_wait = if result.is_err() { RETRY_INTERVAL } else { interval };
    emit_sync_result(&app_handle, &rule_id, &result);

    // Manual mode (interval = 0): only sync on trigger, no periodic timer
    let use_timer = !interval.is_zero();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                eprintln!("[FileSyncEngine] Rule {} cancelled", rule_id);
                break;
            }
            _ = tokio::time::sleep(next_wait), if use_timer => {
                let result = execute_sync(
                    source.clone(),
                    target.clone(),
                    direction,
                    delete_mode,
                    &rule_id,
                    &db,
                    Some(app_handle.clone()),
                )
                .await;
                next_wait = if result.is_err() { RETRY_INTERVAL } else { interval };
                emit_sync_result(&app_handle, &rule_id, &result);
            }
            msg = trigger_receiver.recv() => {
                if msg.is_none() {
                    // All senders dropped — stop the loop cleanly
                    break;
                }
                // Drain any additional pending triggers to avoid redundant syncs
                while trigger_receiver.try_recv().is_ok() {}
                let result = execute_sync(
                    source.clone(),
                    target.clone(),
                    direction,
                    delete_mode,
                    &rule_id,
                    &db,
                    Some(app_handle.clone()),
                )
                .await;
                // Manual triggers reset to normal interval even after a previous failure
                next_wait = if result.is_err() { RETRY_INTERVAL } else { interval };
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
