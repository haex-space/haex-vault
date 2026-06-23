use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::database::DbConnection;

use super::super::diff::compute_sync_actions;
use super::super::provider::{SyncProvider, SyncProviderError};
use super::super::types::{DeleteMode, SyncDirection, SyncResult};

use super::conflict::make_conflict_path;
use super::error::SyncEngineError;
use super::speed::{now_ms, unix_now, SpeedTracker};
use super::state::{mark_deleted, upsert_sync_state};

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
    let mut actions =
        compute_sync_actions(&source_manifest, &target_manifest, direction, delete_mode);

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
        + actions
            .conflicts
            .iter()
            .map(|c| c.source_state.size)
            .sum::<u64>();

    // Diff diagnostics — only emit when the planner produced work or
    // detected conflicts. Logging every cycle would spam stderr on idle
    // rules (sync runs on a poll interval and most cycles are no-ops).
    if total_files > 0 {
        let source_hashed = source_manifest
            .iter()
            .filter(|f| !f.is_directory && f.hash.is_some())
            .count();
        let source_files = source_manifest.iter().filter(|f| !f.is_directory).count();
        let target_hashed = target_manifest
            .iter()
            .filter(|f| !f.is_directory && f.hash.is_some())
            .count();
        let target_files = target_manifest.iter().filter(|f| !f.is_directory).count();
        eprintln!(
            "[FileSyncEngine] Rule {} diff: source={}f ({}h), target={}f ({}h). \
             Plan: dl={}, up={}, del={}, mkdir={}, conflicts={}. Bytes={} ({:.1} MB)",
            rule_id,
            source_files,
            source_hashed,
            target_files,
            target_hashed,
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
            Ok(()) => {
                directories_created.fetch_add(1, Ordering::Relaxed);
            }
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
                            speed_cb
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .add(delta);
                        }
                        emit_cb(false);
                    });

                // If the target is a local filesystem, stream the bytes
                // directly into its final destination — the resume sidecar
                // lives next to the file and survives engine retries. For
                // cloud / peer targets we stage to a tempfile because their
                // "final" destination is remote and a sidecar on local disk
                // would be orphaned.
                let direct_target = target.local_target_path(&file.relative_path);
                let tmp: Option<tempfile::NamedTempFile> = if direct_target.is_some() {
                    None
                } else {
                    match tempfile::NamedTempFile::new() {
                        Ok(f) => Some(f),
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
                    }
                };
                let staging_path: std::path::PathBuf = match (&direct_target, &tmp) {
                    (Some(p), _) => p.clone(),
                    (None, Some(t)) => t.path().to_path_buf(),
                    // Unreachable: one of direct_target / tmp is always Some.
                    (None, None) => unreachable!(
                        "either local_target_path or NamedTempFile must yield a staging path"
                    ),
                };

                // Transfer with one retry on failure
                let expected_chunks = file.chunked_hash();
                let read_result = source
                    .read_file_to_path(
                        &file.relative_path,
                        &staging_path,
                        expected_chunks.clone(),
                        progress_cb.clone(),
                    )
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
                        .read_file_to_path(
                            &file.relative_path,
                            &staging_path,
                            expected_chunks,
                            progress_cb,
                        )
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

                // On manifest-hash drift (source changed under us), the
                // partial bytes are stale — clear the sidecar so the next
                // sync cycle starts from scratch instead of resuming bad
                // data. Transient transfer errors deliberately leave the
                // partial+sidecar intact so the engine's retry-once and
                // the next sync cycle can resume.
                //
                // Defensive: in the verified peer-download path,
                // download_file_to_path would have returned Err before
                // reaching here if the per-chunk hash disagreed with the
                // manifest — `read_result` would be `Err` and `verified`
                // would short-circuit to `Ok(())`. This branch exists
                // because (a) test doubles (e.g. RecordingSource in the
                // engine tests) can produce the mismatch to exercise
                // sidecar cleanup, and (b) future SyncProvider
                // implementations may report a full-file hash without
                // per-chunk verification — in which case a mismatch here
                // means stale bytes from a drifted manifest and the
                // sidecar must not survive into the next sync cycle.
                if verified.is_err() && direct_target.is_some() {
                    let final_path = staging_path.clone();
                    if let Err(e) =
                        crate::peer_storage::resume::PartialState::clear(&final_path).await
                    {
                        errors
                            .lock()
                            .unwrap_or_else(|e2| e2.into_inner())
                            .push(format!("clear partial {}: {e}", file.relative_path));
                    }
                }

                let res: Result<u64, SyncProviderError> = match (read_result, verified) {
                    (Ok(info), Ok(())) => {
                        if direct_target.is_some() {
                            // Already at final destination — no copy needed.
                            Ok(info.bytes)
                        } else {
                            target
                                .write_file_from_path(&file.relative_path, &staging_path)
                                .await
                                .map(|_| info.bytes)
                        }
                    }
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
                        if file.hash.is_some() {
                            target.prime_hash_after_write(&file).await;
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
                            speed_cb
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .add(delta);
                        }
                        emit_cb(false);
                    });

                // NB: target.read_file_to_path through a PeerProvider creates
                // sidecar files (<tmppath>.haex-partial, .haex-partial.meta)
                // for resume support. tempfile::NamedTempFile only auto-removes
                // the tempfile itself; on failure of the read the sidecars
                // remain in the OS temp directory until the OS reaper sweeps
                // them. Acceptable trade-off: resume support for transient
                // upload-mid-failure is worth the occasional bounded leak.
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

                let expected_chunks = file.chunked_hash();
                let read_result = target
                    .read_file_to_path(
                        &file.relative_path,
                        tmp.path(),
                        expected_chunks.clone(),
                        progress_cb.clone(),
                    )
                    .await;
                let read_result = if read_result.is_err() {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    file_progress
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(file.relative_path.clone(), (0, file.size));
                    last_chunk.store(0, std::sync::atomic::Ordering::Relaxed);
                    target
                        .read_file_to_path(
                            &file.relative_path,
                            tmp.path(),
                            expected_chunks,
                            progress_cb,
                        )
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
                        if file.hash.is_some() {
                            source.prime_hash_after_write(&file).await;
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
                .read_file_to_path(
                    &conflict.relative_path,
                    tmp.path(),
                    conflict.target_state.chunked_hash(),
                    noop_progress.clone(),
                )
                .await
            {
                Ok(_) => {
                    if let Err(e) = target
                        .write_file_from_path(&conflict_path, tmp.path())
                        .await
                    {
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
                                    conflict.source_state.chunked_hash(),
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
                                        errors.lock().unwrap_or_else(|e2| e2.into_inner()).push(
                                            format!(
                                                "conflict hash mismatch {}: claimed {} received {}",
                                                conflict.relative_path,
                                                claimed.unwrap_or("?"),
                                                observed.unwrap_or("?"),
                                            ),
                                        );
                                    } else {
                                        match target
                                            .write_file_from_path(
                                                &conflict.relative_path,
                                                src_tmp.path(),
                                            )
                                            .await
                                        {
                                            Ok(()) => {
                                                bytes_done.fetch_add(info.bytes, Ordering::Relaxed);
                                                bytes_transferred
                                                    .fetch_add(info.bytes, Ordering::Relaxed);
                                                speed_tracker
                                                    .lock()
                                                    .unwrap_or_else(|e| e.into_inner())
                                                    .add(info.bytes);
                                                conflicts_resolved.fetch_add(1, Ordering::Relaxed);
                                                resolved = true;
                                                if conflict.source_state.hash.is_some() {
                                                    target
                                                        .prime_hash_after_write(
                                                            &conflict.source_state,
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
                                    errors.lock().unwrap_or_else(|e2| e2.into_inner()).push(
                                        format!(
                                            "conflict read source {}: {e}",
                                            conflict.relative_path
                                        ),
                                    );
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
