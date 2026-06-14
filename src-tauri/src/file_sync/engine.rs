//! Sync engine — orchestration, execution, and periodic loop.
//!
//! Ties together providers, diff computation, and database state tracking.
//!
//! ## Mutex poisoning in progress-tracking locks
//!
//! Many `Mutex`/`RwLock` accesses in this file (`speed_tracker`,
//! `file_progress`, `byte_progress`, `last_emit` timestamps) use
//! `unwrap_or_else(|e| e.into_inner())`. These guard *UI progress state* —
//! transient counters and timestamps used to feed the sync-status emitter.
//! A poison there results in a momentarily wrong byte counter; the next
//! `add()` overwrites with fresh data. There is no durable state behind
//! these locks and no CRDT involvement, so a banner row would be misleading.
//!
//! HLC and DB-mutating paths in this file (e.g. `update_last_synced_at`,
//! `auto_disable_rule`) DO use `lock_or_fail` and surface a banner row.

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

    /// Source manifest could not be fetched (peer offline, network down,
    /// cloud bucket unreachable, …). Treated as a transient condition: the
    /// loop keeps retrying with exponential backoff and never auto-pauses
    /// the rule. Sync simply resumes when the source becomes reachable.
    #[error("Source unavailable: {0}")]
    SourceUnavailable(SyncProviderError),

    /// Target manifest could not be fetched. Same semantics as
    /// `SourceUnavailable` — the target may equally be a phone, peer, or
    /// cloud bucket that goes offline temporarily, so the loop retries
    /// indefinitely with backoff rather than disabling the rule.
    #[error("Target unavailable: {0}")]
    TargetUnavailable(SyncProviderError),

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
    /// SHA-256 of the file content as advertised by the sender at the time
    /// this row was last upserted. Lets the next manifest comparison reuse
    /// the sender's hash instead of re-hashing locally — without it, the
    /// receiver's mtime drift after `tokio::fs::copy` would force the diff
    /// engine to fall back to the size+mtime heuristic and re-fire transfers.
    pub hash: Option<String>,
}

// ---------------------------------------------------------------------------
// Sync-state DB operations
// ---------------------------------------------------------------------------

/// Load all sync state entries for a rule.
pub fn load_sync_state(
    db: &DbConnection,
    rule_id: &str,
) -> Result<Vec<SyncStateEntry>, SyncEngineError> {
    let sql = "SELECT id, rule_id, relative_path, file_size, modified_at, synced_at, deleted, hash FROM haex_sync_state_no_sync WHERE rule_id = ?1".to_string();
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
            hash: row
                .get(7)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
        .collect();

    Ok(entries)
}

/// Insert or update a sync state entry after a file is synced.
///
/// Uses INSERT OR REPLACE on the unique `(rule_id, relative_path)` index.
/// `hash` is the sender's SHA-256 — pass `None` only when the source did not
/// provide one (legacy peer or hashing disabled).
pub fn upsert_sync_state(
    db: &DbConnection,
    rule_id: &str,
    relative_path: &str,
    file_size: u64,
    modified_at: u64,
    hash: Option<&str>,
) -> Result<(), SyncEngineError> {
    let now = unix_now().to_string();
    let id = uuid::Uuid::new_v4().to_string();

    // Use INSERT OR REPLACE — the unique index on (rule_id, relative_path) ensures
    // an existing row is replaced rather than duplicated.
    let sql = "INSERT OR REPLACE INTO haex_sync_state_no_sync (id, rule_id, relative_path, file_size, modified_at, synced_at, deleted, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)".to_string();
    let params = vec![
        JsonValue::String(id),
        JsonValue::String(rule_id.to_string()),
        JsonValue::String(relative_path.to_string()),
        JsonValue::Number(serde_json::Number::from(file_size)),
        JsonValue::Number(serde_json::Number::from(modified_at)),
        JsonValue::String(now),
        match hash {
            Some(h) => JsonValue::String(h.to_string()),
            None => JsonValue::Null,
        },
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
        let oldest = self
            .samples
            .front()
            .expect("invariant: samples.len() >= 2 checked above")
            .0;
        let newest = self
            .samples
            .back()
            .expect("invariant: samples.len() >= 2 checked above")
            .0;
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
///
/// `cancel` is checked before/between each phase (mkdir, downloads, uploads,
/// deletes, conflicts) and inside per-file loops, so a `cancel.cancel()` from
/// `file_sync_stop_all` aborts the current run within at most one in-flight
/// transfer instead of waiting for the whole batch. Without this, a sync that
/// re-fires every cycle (e.g. a misconfigured rule) blocks the vault close
/// because the cancellation only used to take effect at the outer
/// `tokio::select!` between cycles.
pub async fn execute_sync(
    source: Arc<dyn SyncProvider>,
    target: Arc<dyn SyncProvider>,
    direction: SyncDirection,
    delete_mode: DeleteMode,
    rule_id: &str,
    db: &DbConnection,
    app_handle: Option<tauri::AppHandle>,
    cancel: Option<CancellationToken>,
) -> Result<SyncResult, SyncEngineError> {
    macro_rules! check_cancel {
        () => {
            if let Some(ref token) = cancel {
                if token.is_cancelled() {
                    return Err(SyncEngineError::Cancelled);
                }
            }
        };
    }

    check_cancel!();

    // 1. Get manifests (sequential — each is a single network roundtrip)
    // Tag each side's error so the loop can distinguish a transient source
    // outage (peer offline → keep retrying forever) from a target outage
    // (bucket gone → count toward auto-pause).
    let source_manifest = source
        .manifest()
        .await
        .map_err(SyncEngineError::SourceUnavailable)?;
    check_cancel!();
    let target_manifest = target
        .manifest()
        .await
        .map_err(SyncEngineError::TargetUnavailable)?;
    check_cancel!();

    // 2. Compute diff
    let mut actions = compute_sync_actions(&source_manifest, &target_manifest, direction, delete_mode);

    // Drop `mkdir` actions when the target has no real directories (cloud
    // object stores: directories are implicit from object keys and never
    // appear in `manifest()`). Without this, every cycle re-plans the same
    // `mkdir`s, the engine returns `directories_created > 0` for each cycle,
    // and the resulting `update_last_synced_at` + CRDT-dirty event spams the
    // frontend with reloads forever.
    if !target.supports_directories() {
        actions.to_create_directories.clear();
    }
    if !source.supports_directories() && direction == SyncDirection::TwoWay {
        // Symmetric guard for the two-way case where target dirs would be
        // pushed back to a cloud "source".
        actions.to_create_directories.clear();
    }

    let total_files = (actions.to_download.len()
        + actions.to_upload.len()
        + actions.to_delete.len()
        + actions.to_create_directories.len()
        + actions.conflicts.len()) as u32;
    let total_bytes: u64 = actions.to_download.iter().map(|f| f.size).sum::<u64>()
        + actions.to_upload.iter().map(|f| f.size).sum::<u64>()
        + actions.conflicts.iter().map(|c| c.source_state.size).sum::<u64>();

    // Diff diagnostics — only emit when the planner produced work or
    // detected conflicts. Logging every cycle would spam stderr on idle
    // rules (sync runs on a poll interval and most cycles are no-ops).
    if total_files > 0 {
        let source_hashed = source_manifest.iter().filter(|f| !f.is_directory && f.hash.is_some()).count();
        let source_files = source_manifest.iter().filter(|f| !f.is_directory).count();
        let target_hashed = target_manifest.iter().filter(|f| !f.is_directory && f.hash.is_some()).count();
        let target_files = target_manifest.iter().filter(|f| !f.is_directory).count();
        eprintln!(
            "[FileSyncEngine] Rule {} diff: source={}f ({}h), target={}f ({}h). \
             Plan: dl={}, up={}, del={}, mkdir={}, conflicts={}. Bytes={} ({:.1} MB)",
            rule_id,
            source_files, source_hashed,
            target_files, target_hashed,
            actions.to_download.len(),
            actions.to_upload.len(),
            actions.to_delete.len(),
            actions.to_create_directories.len(),
            actions.conflicts.len(),
            total_bytes,
            total_bytes as f64 / (1024.0 * 1024.0),
        );
    }
    if !actions.to_download.is_empty() {
        let sample: Vec<&str> = actions
            .to_download
            .iter()
            .take(3)
            .map(|f| f.relative_path.as_str())
            .collect();
        eprintln!("[FileSyncEngine] First downloads: {:?}", sample);
    }
    if !actions.to_upload.is_empty() {
        let sample: Vec<&str> = actions
            .to_upload
            .iter()
            .take(3)
            .map(|f| f.relative_path.as_str())
            .collect();
        eprintln!("[FileSyncEngine] First uploads: {:?}", sample);
    }

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
            // emit_to(label, …) targets only the main window — .emit() would
            // broadcast file paths to every extension webview (Tauri v2 emit
            // is a fan-out, not a scoped send).
            let _ = app.emit_to(
                "main",
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
        check_cancel!();
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
        check_cancel!();
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
            let cancel_task = cancel.clone();

            join_set.spawn(async move {
                let _permit = sem
                    .acquire()
                    .await
                    .expect("invariant: semaphore is never closed in this engine");
                // Drop the task without doing any I/O if the rule was
                // cancelled while this task was queued behind the semaphore.
                if let Some(ref t) = cancel_task {
                    if t.is_cancelled() {
                        return;
                    }
                }

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

                // Verify the streamed bytes against the manifest hash before
                // touching the destination. A mismatch means corruption in
                // flight (or a malicious peer); writing it would taint the
                // target. We do not retry — QUIC's TLS already covers
                // accidental wire corruption, so a hash mismatch is structural.
                let verified = match &read_result {
                    Ok(info) => match (file.hash.as_deref(), info.hash.as_deref()) {
                        (Some(claimed), Some(observed)) if claimed != observed => {
                            Err(SyncProviderError::Other {
                                reason: format!(
                                    "hash mismatch: manifest claimed {claimed}, received {observed}"
                                ),
                            })
                        }
                        _ => Ok(()),
                    },
                    Err(_) => Ok(()),
                };

                let res: Result<u64, SyncProviderError> = match (read_result, verified) {
                    (Ok(info), Ok(())) => target
                        .write_file_from_path(&file.relative_path, tmp.path())
                        .await
                        .map(|_| info.bytes),
                    (_, Err(e)) => Err(e),
                    (Err(e), _) => Err(e),
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
                        if let Some(h) = file.hash.as_deref() {
                            target.prime_hash_after_write(&file.relative_path, h).await;
                        }
                        if let Err(e) = upsert_sync_state(
                            &db_clone,
                            &rule_id_clone,
                            &file.relative_path,
                            file.size,
                            file.modified_at,
                            file.hash.as_deref(),
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
        check_cancel!();
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
            let cancel_task = cancel.clone();

            join_set.spawn(async move {
                let _permit = sem
                    .acquire()
                    .await
                    .expect("invariant: semaphore is never closed in this engine");
                if let Some(ref t) = cancel_task {
                    if t.is_cancelled() {
                        return;
                    }
                }

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

                let verified = match &read_result {
                    Ok(info) => match (file.hash.as_deref(), info.hash.as_deref()) {
                        (Some(claimed), Some(observed)) if claimed != observed => {
                            Err(SyncProviderError::Other {
                                reason: format!(
                                    "hash mismatch: manifest claimed {claimed}, received {observed}"
                                ),
                            })
                        }
                        _ => Ok(()),
                    },
                    Err(_) => Ok(()),
                };

                let res: Result<u64, SyncProviderError> = match (read_result, verified) {
                    (Ok(info), Ok(())) => source
                        .write_file_from_path(&file.relative_path, tmp.path())
                        .await
                        .map(|_| info.bytes),
                    (_, Err(e)) => Err(e),
                    (Err(e), _) => Err(e),
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
                        if let Some(h) = file.hash.as_deref() {
                            source.prime_hash_after_write(&file.relative_path, h).await;
                        }
                        if let Err(e) = upsert_sync_state(
                            &db_clone,
                            &rule_id_clone,
                            &file.relative_path,
                            file.size,
                            file.modified_at,
                            file.hash.as_deref(),
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
    check_cancel!();
    let to_trash = matches!(delete_mode, DeleteMode::Trash);
    for path in &actions.to_delete {
        check_cancel!();
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
    //
    // Stages each side through a temp file via the streaming `*_to_path` /
    // `*_from_path` provider APIs so a multi-GB conflict does not buffer the
    // entire payload in RAM.
    // -------------------------------------------------------------------------
    check_cancel!();
    let noop_progress: Arc<dyn Fn(u64, u64) + Send + Sync> = Arc::new(|_, _| {});
    for conflict in &actions.conflicts {
        check_cancel!();
        let seq = active_seq.fetch_add(1, Ordering::Relaxed);
        active_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((seq, conflict.relative_path.clone()));
        emit_progress(true);

        let timestamp = unix_now() as i64;
        let conflict_path = make_conflict_path(&conflict.relative_path, timestamp);
        let mut resolved = false;

        // Step 1: stage target's current file into a temp, then write it
        //         out at `conflict_path` (so we don't lose it).
        let target_tmp = match tempfile::NamedTempFile::new() {
            Ok(f) => Some(f),
            Err(e) => {
                errors
                    .lock()
                    .unwrap_or_else(|e2| e2.into_inner())
                    .push(format!("conflict tmpfile {}: {e}", conflict.relative_path));
                None
            }
        };

        if let Some(tmp) = target_tmp {
            match target
                .read_file_to_path(&conflict.relative_path, tmp.path(), noop_progress.clone())
                .await
            {
                Ok(_) => {
                    if let Err(e) = target.write_file_from_path(&conflict_path, tmp.path()).await {
                        errors
                            .lock()
                            .unwrap_or_else(|e2| e2.into_inner())
                            .push(format!("conflict rename {}: {e}", conflict.relative_path));
                    } else {
                        // Step 2: stage source into a fresh temp, then write it
                        //         to `target` at the original path.
                        let source_tmp = match tempfile::NamedTempFile::new() {
                            Ok(f) => Some(f),
                            Err(e) => {
                                errors
                                    .lock()
                                    .unwrap_or_else(|e2| e2.into_inner())
                                    .push(format!(
                                        "conflict tmpfile source {}: {e}",
                                        conflict.relative_path
                                    ));
                                None
                            }
                        };

                        if let Some(src_tmp) = source_tmp {
                            match source
                                .read_file_to_path(
                                    &conflict.relative_path,
                                    src_tmp.path(),
                                    noop_progress.clone(),
                                )
                                .await
                            {
                                Ok(info) => {
                                    let claimed = conflict.source_state.hash.as_deref();
                                    let observed = info.hash.as_deref();
                                    let mismatch = matches!(
                                        (claimed, observed),
                                        (Some(c), Some(o)) if c != o
                                    );
                                    if mismatch {
                                        errors
                                            .lock()
                                            .unwrap_or_else(|e2| e2.into_inner())
                                            .push(format!(
                                                "conflict hash mismatch {}: claimed {} received {}",
                                                conflict.relative_path,
                                                claimed.unwrap_or("?"),
                                                observed.unwrap_or("?"),
                                            ));
                                    } else {
                                        match target
                                            .write_file_from_path(
                                                &conflict.relative_path,
                                                src_tmp.path(),
                                            )
                                            .await
                                        {
                                            Ok(()) => {
                                                bytes_done
                                                    .fetch_add(info.bytes, Ordering::Relaxed);
                                                bytes_transferred
                                                    .fetch_add(info.bytes, Ordering::Relaxed);
                                                speed_tracker
                                                    .lock()
                                                    .unwrap_or_else(|e| e.into_inner())
                                                    .add(info.bytes);
                                                conflicts_resolved
                                                    .fetch_add(1, Ordering::Relaxed);
                                                resolved = true;
                                                if let Some(h) =
                                                    conflict.source_state.hash.as_deref()
                                                {
                                                    target
                                                        .prime_hash_after_write(
                                                            &conflict.relative_path,
                                                            h,
                                                        )
                                                        .await;
                                                }
                                                let _ = upsert_sync_state(
                                                    db,
                                                    rule_id,
                                                    &conflict.relative_path,
                                                    conflict.source_state.size,
                                                    conflict.source_state.modified_at,
                                                    conflict.source_state.hash.as_deref(),
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
                }
                Err(e) => {
                    errors
                        .lock()
                        .unwrap_or_else(|e2| e2.into_inner())
                        .push(format!(
                            "conflict read target {}: {e}",
                            conflict.relative_path
                        ));
                }
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
    // Phase 2: route HLC poison through `AppState::lock_or_fail` so the
    // user sees a banner via `haex_critical_notifications_no_sync`
    // instead of a silent skip + stderr-only log. Function returns ()
    // so we can't propagate the Err — but the banner row is persisted
    // regardless, and skipping the last-synced-at update is the
    // correct fallback (the alternative would be writing a CRDT row
    // with a corrupted HLC).
    let hlc = match state.lock_or_fail(
        &state.hlc,
        crate::critical::CriticalFailureCode::HlcMutexPoisoned,
        "file_sync::engine::update_last_synced_at",
        serde_json::json!({"rule_id": rule_id}),
    ) {
        Ok(g) => g,
        Err(_) => return,
    };
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
    let _ = app.emit_to("main", crate::event_names::EVENT_CRDT_DIRTY_TABLES_CHANGED, ());
}

// ---------------------------------------------------------------------------
// Event emission
// ---------------------------------------------------------------------------

/// Persist a sync log entry into the CRDT-synced `haex_logs` table.
///
/// `source = "file-sync"`, and the rule ID is stored in the `metadata` JSON
/// (`{ "ruleId": <id> }`) — NOT in `extension_id`, because that column has a
/// FK on `haex_extensions(id)` and sync rules are not extensions, so the
/// INSERT would fail with `FOREIGN KEY constraint failed` on every cycle.
///
/// `message` is encoded as JSON `{ code, params?, raw? }` — a stable
/// machine-readable shape so the frontend can localize the rendered string per
/// device locale. Persisting a pre-rendered locale-specific string here would
/// freeze that locale into the CRDT row forever (it gets replicated to every
/// peer regardless of their locale).
fn write_sync_log_entry(
    app: &tauri::AppHandle,
    rule_id: &str,
    level: &str,
    code: &str,
    params: serde_json::Value,
    raw: Option<&str>,
) {
    use tauri::Manager;
    let state = app.state::<crate::AppState>();
    let device_id = state
        .context
        .lock()
        .map(|ctx| ctx.device_id.clone())
        .unwrap_or_default();
    let mut message = serde_json::json!({ "code": code, "params": params });
    if let Some(r) = raw {
        message["raw"] = serde_json::Value::String(r.to_string());
    }
    let metadata = serde_json::json!({ "ruleId": rule_id });
    if let Err(e) = crate::logging::insert_log(
        &state,
        level,
        "file-sync",
        None,
        &message.to_string(),
        Some(metadata),
        &device_id,
    ) {
        eprintln!("[FileSyncEngine] Failed to persist sync log for rule {rule_id}: {e}");
    }
}

fn emit_sync_result(
    app: &tauri::AppHandle,
    rule_id: &str,
    result: &Result<SyncResult, SyncEngineError>,
) {
    use tauri::Emitter;
    // emit_to(label, …) keeps these UI-only events out of extension webviews.
    match result {
        Ok(r) => {
            update_last_synced_at(app, rule_id);
            // Per-file transfer failures are collected into r.errors instead of
            // surfacing as Err — without this branch a cycle where every
            // transfer failed (counters all 0, errors populated) would leave
            // no trace in the persistent log.
            if !r.errors.is_empty() {
                let raw = r.errors.join("; ");
                write_sync_log_entry(
                    app,
                    rule_id,
                    "error",
                    "syncCompletedWithErrors",
                    serde_json::json!({ "errorCount": r.errors.len() }),
                    Some(&raw),
                );
            } else if r.files_downloaded > 0
                || r.files_deleted > 0
                || r.directories_created > 0
                || r.conflicts_resolved > 0
            {
                // Only log non-trivial cycles so the persistent log doesn't fill up
                // with empty no-op syncs — mirrors the in-memory append logic in
                // the frontend store. All non-zero counters are persisted so
                // delete-only / mkdir-only / conflict-only cycles don't render
                // as "0 files / 0 bytes" in the history.
                write_sync_log_entry(
                    app,
                    rule_id,
                    "info",
                    "syncSuccess",
                    serde_json::json!({
                        "filesDownloaded": r.files_downloaded,
                        "filesDeleted": r.files_deleted,
                        "directoriesCreated": r.directories_created,
                        "conflictsResolved": r.conflicts_resolved,
                        "bytesTransferred": r.bytes_transferred,
                    }),
                    None,
                );
            }
            let _ = app.emit_to(
                "main",
                "file-sync:complete",
                serde_json::json!({ "ruleId": rule_id, "result": r }),
            );
        }
        Err(e) => {
            // Cancellation is a user-initiated control-flow signal, not a
            // sync failure — persisting it as `syncFailed` would pollute the
            // CRDT log on every stop/disable. The frontend already removes
            // the in-flight state when a cancel emits, so skipping here
            // leaves no orphaned UI artifacts either.
            if matches!(e, SyncEngineError::Cancelled) {
                return;
            }
            let raw = e.to_string();
            // Source/target unavailability (peer offline, network blip,
            // bucket unreachable) is expected — the loop retries with
            // backoff and the rule must not auto-pause. Persisting
            // "syncFailed" every cycle would spam the CRDT log and bounce
            // the frontend on every retry. The runtime event is still
            // emitted so the UI can show a transient state.
            let unavailable_side: Option<&'static str> = match e {
                SyncEngineError::SourceUnavailable(_) => Some("source"),
                SyncEngineError::TargetUnavailable(_) => Some("target"),
                _ => None,
            };
            if unavailable_side.is_none() {
                // Genuine error (DB failure, provider crash, …). Render the
                // raw text verbatim — it already includes whatever
                // provider-specific detail the user needs to debug.
                write_sync_log_entry(
                    app,
                    rule_id,
                    "error",
                    "syncFailed",
                    serde_json::json!({}),
                    Some(&raw),
                );
            }
            let _ = app.emit_to(
                "main",
                "file-sync:error",
                serde_json::json!({
                    "ruleId": rule_id,
                    "error": raw,
                    "unavailable": unavailable_side,
                }),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Periodic sync loop
// ---------------------------------------------------------------------------

/// Base wait after the first failed sync cycle. Subsequent failures double
/// this duration (exponential backoff) up to `MAX_RETRY_INTERVAL`.
const INITIAL_RETRY: Duration = Duration::from_secs(30);

/// Hard cap on the retry interval. Reached after ~6 consecutive failures.
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// After this many consecutive failures the rule is auto-disabled so it
/// stops hammering a broken target. The user has to re-enable it manually
/// after fixing the underlying issue.
const MAX_CONSECUTIVE_FAILURES: u32 = 20;

/// 30s * 2^(failures-1), capped at MAX_RETRY_INTERVAL.
fn backoff_duration(consecutive_failures: u32) -> Duration {
    if consecutive_failures == 0 {
        return INITIAL_RETRY;
    }
    let shift = (consecutive_failures - 1).min(10);
    let secs = INITIAL_RETRY
        .as_secs()
        .saturating_mul(1u64 << shift);
    Duration::from_secs(secs.min(MAX_RETRY_INTERVAL.as_secs()))
}

/// Persist `enabled = false` on a sync rule, tear down its runtime
/// state (SyncManager registration + file watchers) and notify the frontend.
async fn auto_disable_rule(
    app: &tauri::AppHandle,
    rule_id: &str,
    failures: u32,
    last_error: &str,
) {
    use tauri::{Emitter, Manager};
    let state = app.state::<crate::AppState>();
    {
        let hlc = match state.lock_or_fail(
            &state.hlc,
            crate::critical::CriticalFailureCode::HlcMutexPoisoned,
            "file_sync::engine::auto_disable_rule",
            // Surface the failing rule_id in the banner row so an operator
            // looking at `haex_critical_notifications_no_sync` can correlate
            // the poison to a specific user-visible sync rule.
            serde_json::json!({ "rule_id": rule_id }),
        ) {
            Ok(g) => g,
            Err(_) => return,
        };

        let sql = "UPDATE haex_sync_rules SET enabled = 0 WHERE id = ?1".to_string();
        let params = vec![JsonValue::String(rule_id.to_string())];

        if let Err(e) = crate::database::core::execute_with_crdt(sql, params, &state.db, &hlc) {
            eprintln!(
                "[FileSyncEngine] Failed to persist auto-pause for rule {rule_id}: {e}"
            );
        }
    }

    // Unregister from SyncManager so `is_running` reflects reality, and stop
    // any file watchers that were started alongside this rule. Without this,
    // the loop exits but the runtime state stays as a zombie entry.
    {
        let mut manager = state.sync_manager.lock().await;
        manager.stop(rule_id);
    }
    let _ = state.file_watcher.unwatch(rule_id);
    let _ = state.file_watcher.unwatch(&format!("{}_source", rule_id));

    write_sync_log_entry(
        app,
        rule_id,
        "error",
        "autoPaused",
        serde_json::json!({ "failures": failures }),
        Some(last_error),
    );

    let _ = app.emit_to(
        "main",
        "file-sync:auto-paused",
        serde_json::json!({
            "ruleId": rule_id,
            "consecutiveFailures": failures,
            "lastError": last_error,
        }),
    );
    let _ = app.emit_to(
        "main",
        crate::event_names::EVENT_CRDT_DIRTY_TABLES_CHANGED,
        (),
    );
}

/// Run periodic sync for a rule. Cancellable via `CancellationToken`.
///
/// The optional `trigger_receiver` allows external events (e.g. file watcher)
/// to interrupt the sleep timer and trigger an immediate sync cycle.
///
/// On failure, exponential backoff is applied: 30s, 60s, 120s, … up to
/// `MAX_RETRY_INTERVAL`. The counter resets on the first successful cycle
/// so transient failures still self-heal quickly.
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
        Some(cancel.clone()),
    )
    .await;
    eprintln!("[FileSyncEngine] Rule {} initial sync done: {:?}", rule_id, result.as_ref().map(|r| r.files_downloaded));

    // Two independent counters:
    // - `consecutive_failures` drives the exponential backoff (any failure
    //   slows the retry cadence, including transient outages).
    // - `pause_failures` is the subset that counts toward auto-pause.
    //   Source/target-unavailable errors are excluded because the remote
    //   side may equally be a phone, peer, or cloud bucket that goes
    //   offline temporarily — the rule must keep pinging and resume on
    //   reconnect instead of disabling itself.
    fn is_unavailable(e: &SyncEngineError) -> bool {
        matches!(
            e,
            SyncEngineError::SourceUnavailable(_) | SyncEngineError::TargetUnavailable(_)
        )
    }
    let initial_is_unavail = result.as_ref().err().map(is_unavailable).unwrap_or(false);
    let mut consecutive_failures: u32 = if result.is_err() { 1 } else { 0 };
    let mut pause_failures: u32 = if result.is_err() && !initial_is_unavail { 1 } else { 0 };
    let mut next_wait = if consecutive_failures > 0 {
        let w = backoff_duration(consecutive_failures);
        eprintln!(
            "[FileSyncEngine] Rule {} failed (attempt {}), next retry in {}s",
            rule_id,
            consecutive_failures,
            w.as_secs()
        );
        w
    } else {
        interval
    };
    // Last error message, used when auto-pausing the rule.
    let mut last_error_text: String = result
        .as_ref()
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    // Marker used by the trigger arm to honour the backoff window: it skips
    // any trigger that fires before the next allowed attempt.
    let mut next_attempt_at: std::time::Instant = std::time::Instant::now() + next_wait;
    emit_sync_result(&app_handle, &rule_id, &result);

    // Stop immediately if the very first sync already exhausted the budget
    // (only realistic with MAX = 1, but keeps the invariant clean).
    if pause_failures >= MAX_CONSECUTIVE_FAILURES {
        eprintln!(
            "[FileSyncEngine] Rule {} auto-paused after {} consecutive failures",
            rule_id, pause_failures
        );
        auto_disable_rule(&app_handle, &rule_id, pause_failures, &last_error_text).await;
        return;
    }

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
                    Some(cancel.clone()),
                )
                .await;
                if let Err(ref e) = result {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    if !is_unavailable(e) {
                        pause_failures = pause_failures.saturating_add(1);
                    }
                    last_error_text = e.to_string();
                    next_wait = backoff_duration(consecutive_failures);
                    eprintln!(
                        "[FileSyncEngine] Rule {} failed (attempt {}), next retry in {}s",
                        rule_id,
                        consecutive_failures,
                        next_wait.as_secs()
                    );
                } else {
                    if consecutive_failures > 0 {
                        eprintln!(
                            "[FileSyncEngine] Rule {} recovered after {} failures",
                            rule_id, consecutive_failures
                        );
                    }
                    consecutive_failures = 0;
                    pause_failures = 0;
                    last_error_text.clear();
                    next_wait = interval;
                }
                next_attempt_at = std::time::Instant::now() + next_wait;
                emit_sync_result(&app_handle, &rule_id, &result);

                if pause_failures >= MAX_CONSECUTIVE_FAILURES {
                    eprintln!(
                        "[FileSyncEngine] Rule {} auto-paused after {} consecutive failures",
                        rule_id, pause_failures
                    );
                    auto_disable_rule(
                        &app_handle,
                        &rule_id,
                        pause_failures,
                        &last_error_text,
                    )
                    .await;
                    break;
                }
            }
            msg = trigger_receiver.recv() => {
                if msg.is_none() {
                    // All senders dropped — stop the loop cleanly
                    break;
                }
                // Drain any additional pending triggers to avoid redundant syncs
                while trigger_receiver.try_recv().is_ok() {}

                // While the backoff window is open, ignore file-watcher triggers
                // — otherwise filesystem activity bypasses the retry slowdown
                // and we hammer the failing target. Triggers reaching us after
                // the backoff has elapsed proceed normally.
                if consecutive_failures > 0 && std::time::Instant::now() < next_attempt_at {
                    let remaining = next_attempt_at - std::time::Instant::now();
                    eprintln!(
                        "[FileSyncEngine] Rule {} trigger suppressed during backoff (~{}s left)",
                        rule_id,
                        remaining.as_secs()
                    );
                    continue;
                }

                let result = execute_sync(
                    source.clone(),
                    target.clone(),
                    direction,
                    delete_mode,
                    &rule_id,
                    &db,
                    Some(app_handle.clone()),
                    Some(cancel.clone()),
                )
                .await;
                if let Err(ref e) = result {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    if !is_unavailable(e) {
                        pause_failures = pause_failures.saturating_add(1);
                    }
                    last_error_text = e.to_string();
                    next_wait = backoff_duration(consecutive_failures);
                } else {
                    consecutive_failures = 0;
                    pause_failures = 0;
                    last_error_text.clear();
                    next_wait = interval;
                }
                next_attempt_at = std::time::Instant::now() + next_wait;
                emit_sync_result(&app_handle, &rule_id, &result);

                if pause_failures >= MAX_CONSECUTIVE_FAILURES {
                    eprintln!(
                        "[FileSyncEngine] Rule {} auto-paused after {} consecutive failures",
                        rule_id, pause_failures
                    );
                    auto_disable_rule(
                        &app_handle,
                        &rule_id,
                        pause_failures,
                        &last_error_text,
                    )
                    .await;
                    break;
                }
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
