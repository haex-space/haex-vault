use std::sync::Arc;

use iroh::{EndpointId, RelayUrl};

use crate::file_sync::hashing::ChunkedHash;
use crate::peer_storage::client::StreamReadResult;
use crate::peer_storage::endpoint::PeerEndpoint;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{Request, Response};
use crate::peer_storage::streaming;

use super::retry_pool::run_bounded_retry_pool;

/// Download a file as `parallelism` parallel range reads, each on its own
/// QUIC stream. Faster than [`PeerEndpoint::read_open_streams_to_file`] for
/// large files because per-stream throughput stops being the bottleneck.
///
/// `size` must be the authoritative file size from the sender (e.g. from
/// the manifest or a stat call) — the function pre-allocates the partial
/// file to that exact length and writes each range at its own offset.
///
/// ## Per-range retry (Task 9)
///
/// Each range may fail up to [`streaming::MAX_RANGE_RETRIES`] times before
/// the whole download surfaces an error. Failures are re-queued and a sibling
/// worker (or the same worker after returning from one range to the pool)
/// picks them up — sibling ranges continue transferring while a failed range
/// is retried, so a single flaky stream no longer aborts the entire download.
///
/// ## Chunk-hash verification (Task 9)
///
/// When `chunks_to_use` is `Some(_)` (sync + file-browser flows both supply
/// chunks via [`download_file_to_path`]), each worker streams into a
/// [`streaming::ChunkVerifier`] over the slice of manifest hashes covering
/// its range. Bytes are landed in `<output_path>.haex-partial` and the
/// resume sidecar is rewritten after every verified chunk, mirroring the
/// single-stream resume path. On full success the partial file is atomically
/// renamed to `output_path` and the sidecar is cleared. On failure (after
/// retries) both are deliberately left on disk for the next attempt to
/// resume from.
///
/// ## Cross-invocation resume (Task 10)
///
/// On entry the function probes for a surviving sidecar at
/// `<output_path>.haex-partial.meta`. If `PartialState::load_if_matches`
/// finds one (file_hash matches the manifest, chunk count + chunk_size
/// agree), the worker pool is seeded with `missing_ranges()` instead of an
/// equal N-way split — already-verified chunks stay marked done in the
/// shared bitmap and their bytes survive on disk. A drifted sidecar
/// (mismatched chunk count or chunk_size) is treated like a missing one:
/// the partial file is truncated and the download starts fresh.
pub(crate) async fn read_multipart_to_file(
    endpoint: Arc<tokio::sync::RwLock<PeerEndpoint>>,
    remote_id: EndpointId,
    relay_url: Option<RelayUrl>,
    path: String,
    output_path: std::path::PathBuf,
    size: u64,
    parallelism: usize,
    chunks_to_use: &ChunkedHash,
    on_progress: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    pause_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    ucan_token: String,
) -> Result<StreamReadResult, PeerStorageError> {
    use std::sync::atomic::AtomicU64;

    if size == 0 {
        tokio::fs::File::create(&output_path)
            .await
            .map_err(PeerStorageError::Io)?;
        return Ok(StreamReadResult {
            bytes: 0,
            hash: Some(chunks_to_use.file_hash.clone()),
        });
    }

    let n = parallelism
        .max(1)
        .min(streaming::MAX_PARALLEL_STREAMS_PER_FILE);

    // Verified path streams bytes to `<output>.haex-partial`, then atomic-
    // renames + clears the sidecar on success.
    let write_path: std::path::PathBuf =
        crate::peer_storage::resume::PartialState::partial_path(&output_path);

    // Sweep any orphaned `.haex-partial.meta.tmp.<nonce>` files a previous
    // attempt left behind (interrupted save() between write + rename). The
    // prior pool has fully unwound by now, so nothing is mid-save and every
    // remaining temp file is garbage. Best-effort: a sweep failure must not
    // block the download. The `.meta`/`.haex-partial` pair is preserved for
    // the resume probe below.
    let _ = crate::peer_storage::resume::PartialState::sweep_tmp(&output_path).await;

    // Resume probe: if a surviving sidecar's file_hash matches the manifest,
    // re-use its bitmap + partial bytes. A drifted sidecar (mismatched chunk
    // count or chunk_size) is treated exactly like a missing one — we
    // discard it and start fresh so the worker pool never aligns chunks
    // against the wrong manifest.
    let resume_state: Option<crate::peer_storage::resume::PartialState> = {
        let candidate = crate::peer_storage::resume::PartialState::load_if_matches(
            &output_path,
            &chunks_to_use.file_hash,
        )
        .await
        .map_err(PeerStorageError::Io)?;
        candidate.filter(|s| {
            s.completed.len() == chunks_to_use.chunk_hashes.len()
                && s.chunk_size == chunks_to_use.chunk_size
        })
    };

    // Pre-allocation: on a fresh download we create+truncate the partial
    // file to `size` so every worker can seek to its own offset. On resume
    // the partial file already exists at the right length with verified
    // chunks at their correct offsets — truncating would discard them, so
    // we leave it untouched.
    if resume_state.is_none() {
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&write_path)
            .await
            .map_err(PeerStorageError::Io)?;
        file.set_len(size).await.map_err(PeerStorageError::Io)?;
        file.flush().await.map_err(PeerStorageError::Io)?;
    }

    // Initial range pool: on fresh download we split `size` into N equal
    // ranges; on resume we feed the pool the sidecar's `missing_ranges()`
    // (end-clamped to `size` because the last chunk may overshoot the
    // actual file end). Resume may produce N != concurrency entries —
    // could be fewer if only one gap, more if many — and that's fine: the
    // pool just drains until empty, the worker count stays at `n`.
    let mut initial_ranges: Vec<(u64, u64, u32)> = if let Some(state) = resume_state.as_ref() {
        state
            .missing_ranges()
            .into_iter()
            .filter_map(|(s, e)| {
                let end = e.min(size);
                (end > s).then_some((s, end, 0))
            })
            .collect()
    } else {
        let chunk = size.div_ceil(n as u64);
        (0..n)
            .filter_map(|i| {
                let start = (i as u64) * chunk;
                if start >= size {
                    return None;
                }
                let end = (start + chunk).min(size);
                Some((start, end, 0))
            })
            .collect()
    };
    // pop() drains from the back; reverse so the lowest range is popped first
    // and progress reports run roughly in file order.
    initial_ranges.reverse();

    // Shared bitmap: each worker writes true into `completed[absolute_chunk_idx]`
    // as ChunkVerifier validates a chunk; the helper persists the sidecar after
    // every chunk so a future resume can pick up from any point of progress.
    // On resume we re-seed the bitmap from the sidecar so chunks that already
    // completed in a prior invocation stay marked done (and `missing_ranges()`
    // already excluded their byte ranges from the pending pool).
    let total_chunks = chunks_to_use.chunk_hashes.len();
    let initial_completed = match resume_state.as_ref() {
        Some(state) => state.completed.clone(),
        None => vec![false; total_chunks],
    };
    let completed: Arc<tokio::sync::Mutex<Vec<bool>>> =
        Arc::new(tokio::sync::Mutex::new(initial_completed));

    let pending: Arc<tokio::sync::Mutex<Vec<(u64, u64, u32)>>> =
        Arc::new(tokio::sync::Mutex::new(initial_ranges));

    // Progress accounting through retries: a failed attempt's contribution
    // is rolled back from `total_received` before the retry starts, so the
    // consumer never sees a decrement and the final report equals `size`.
    // On resume we seed `total_received` with the bytes already on disk so
    // the progress callback stays monotonic across invocations (otherwise
    // it would drop from `size` at the end of the first attempt back to
    // ~0 at the start of the second).
    let already_done_bytes: u64 = if let Some(state) = resume_state.as_ref() {
        let cs = state.chunk_size as u64;
        let n_done = state.completed.iter().filter(|c| **c).count() as u64;
        // Last chunk may be partial — cap the final sum at `size`.
        (n_done * cs).min(size)
    } else {
        0
    };
    let total_received = Arc::new(AtomicU64::new(already_done_bytes));
    // std::sync::Mutex (not tokio) so the synchronous per-chunk progress
    // callback can update it without awaiting. It's locked only for a tiny
    // map insert/remove and never held across an await point.
    let range_progress: Arc<std::sync::Mutex<std::collections::HashMap<(u64, u64), u64>>> =
        Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

    let total_size = size;
    let max_retries = streaming::MAX_RANGE_RETRIES;

    let chunks_owned: Arc<ChunkedHash> = Arc::new(chunks_to_use.clone());

    // Wrap the per-range work in a closure the generic retry pool can drive.
    // The closure clones the per-attempt state once and the pool calls it
    // for every attempt against every range.
    let fetcher = {
        let endpoint = endpoint.clone();
        let path = path.clone();
        let write_path = write_path.clone();
        let output_path = output_path.clone();
        let ucan_token = ucan_token.clone();
        let relay_url = relay_url.clone();
        let cancel_token = cancel_token.clone();
        let pause_flag = pause_flag.clone();
        let on_progress = on_progress.clone();
        let total_received = total_received.clone();
        let range_progress = range_progress.clone();
        let completed = completed.clone();
        let chunks_owned = chunks_owned.clone();

        Arc::new(move |start: u64, end: u64| {
            let endpoint = endpoint.clone();
            let path = path.clone();
            let write_path = write_path.clone();
            let output_path = output_path.clone();
            let ucan_token = ucan_token.clone();
            let relay_url = relay_url.clone();
            let cancel_token = cancel_token.clone();
            let pause_flag = pause_flag.clone();
            let on_progress = on_progress.clone();
            let total_received = total_received.clone();
            let range_progress = range_progress.clone();
            let completed = completed.clone();
            let chunks_owned = chunks_owned.clone();
            Box::pin(async move {
                download_range_attempt(
                    &endpoint,
                    remote_id,
                    relay_url,
                    &path,
                    &write_path,
                    &output_path,
                    start,
                    end,
                    &ucan_token,
                    cancel_token,
                    pause_flag,
                    on_progress,
                    total_received,
                    range_progress,
                    total_size,
                    &chunks_owned,
                    completed,
                )
                .await
            })
                as std::pin::Pin<
                    Box<dyn std::future::Future<Output = Result<(), PeerStorageError>> + Send>,
                >
        })
    };

    // Per-range progress accounting bookkeeping needs to roll back failed
    // attempts before retry. Pass that hook into the pool so it can clear
    // partial counters between attempts.
    let total_received_for_rollback = total_received.clone();
    let range_progress_for_rollback = range_progress.clone();
    let on_retry: Arc<
        dyn Fn(
                (u64, u64, u32),
                &PeerStorageError,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + Sync,
    > = Arc::new(move |(start, end, attempt), err| {
        let total_received = total_received_for_rollback.clone();
        let range_progress = range_progress_for_rollback.clone();
        let msg = err.to_string();
        Box::pin(async move {
            let mut rp = range_progress.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(prev) = rp.remove(&(start, end)) {
                total_received.fetch_sub(prev, std::sync::atomic::Ordering::Relaxed);
            }
            drop(rp);
            eprintln!(
                "[PeerStorage] multipart range [{start}, {end}) attempt {attempt} failed, retrying: {msg}",
            );
        })
    });

    let first_err = run_bounded_retry_pool(pending, n, max_retries, fetcher, Some(on_retry)).await;

    if let Some(err) = first_err {
        // Leave partial bytes + sidecar on disk so the next download attempt
        // can resume from the chunks that did complete.
        return Err(err);
    }

    // Defensive: every chunk should be marked complete now. If not, a
    // worker returned Ok despite leaving gaps — that's a bug, not a
    // transport failure.
    let completed_snapshot = completed.lock().await;
    if !completed_snapshot.iter().all(|c| *c) {
        return Err(PeerStorageError::ProtocolError {
            reason: "multipart workers completed without filling chunk bitmap".to_string(),
        });
    }
    drop(completed_snapshot);

    tokio::fs::rename(&write_path, &output_path)
        .await
        .map_err(PeerStorageError::Io)?;
    crate::peer_storage::resume::PartialState::clear(&output_path)
        .await
        .map_err(PeerStorageError::Io)?;

    Ok(StreamReadResult {
        bytes: size,
        hash: Some(chunks_to_use.file_hash.clone()),
    })
}

/// One attempt at downloading a single range. Opens its own QUIC stream,
/// sends `Read { range: Some([start, end]) }`, and pipes the response into
/// `write_path` at offset `start`. When `chunks` is `Some`, the worker uses
/// [`streaming::pipe_recv_to_writer_verified`] so each chunk is BLAKE3-hashed
/// and the shared `completed` bitmap is mutated as chunks verify; when
/// `chunks` is `None`, falls back to the legacy unverified pipeline so the
/// pre-Task-6 callers still work.
#[allow(clippy::too_many_arguments)]
async fn download_range_attempt(
    endpoint: &Arc<tokio::sync::RwLock<PeerEndpoint>>,
    remote_id: EndpointId,
    relay_url: Option<RelayUrl>,
    path: &str,
    write_path: &std::path::Path,
    final_output_path: &std::path::Path,
    start: u64,
    end: u64,
    ucan_token: &str,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    pause_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    on_progress: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    total_received: Arc<std::sync::atomic::AtomicU64>,
    range_progress: Arc<std::sync::Mutex<std::collections::HashMap<(u64, u64), u64>>>,
    total_size: u64,
    chunks: &ChunkedHash,
    completed: Arc<tokio::sync::Mutex<Vec<bool>>>,
) -> Result<(), PeerStorageError> {
    use std::sync::atomic::Ordering;

    let part_size = end - start;

    // Bail before opening a stream if the transfer was already cancelled, so a
    // cancel doesn't cost an extra range round-trip per idle worker.
    if cancel_token.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err(PeerStorageError::Cancelled);
    }

    let (mut send, mut recv) = endpoint
        .read()
        .await
        .open_stream(remote_id, relay_url)
        .await?;

    let req = Request::Read {
        path: path.to_string(),
        range: Some([start, end]),
        ucan_token: ucan_token.to_string(),
    };
    let response = PeerEndpoint::send_request(&mut send, &mut recv, &req).await?;
    let announced = match response {
        Response::ReadHeader { size } => size,
        Response::Error { message } => {
            return Err(PeerStorageError::ProtocolError { reason: message });
        }
        _ => {
            return Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response in multipart read".to_string(),
            });
        }
    };
    if announced != part_size {
        return Err(PeerStorageError::ProtocolError {
            reason: format!(
                "multipart range size mismatch: requested {part_size}, peer announced {announced}"
            ),
        });
    }

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create(false)
        .open(write_path)
        .await
        .map_err(PeerStorageError::Io)?;
    use tokio::io::AsyncSeekExt;
    file.seek(std::io::SeekFrom::Start(start))
        .await
        .map_err(PeerStorageError::Io)?;

    {
        // ChunkVerifier mutates the shared bitmap and the
        // sidecar gets rewritten after every verified chunk. Map this byte
        // range back to its chunk-bitmap slice; both bounds align on a
        // chunk boundary for the initial equal split (and Task 10's
        // resume-aware split will preserve that alignment).
        let chunk_size = chunks.chunk_size as u64;
        if chunk_size == 0 {
            return Err(PeerStorageError::ProtocolError {
                reason: "chunk_size must be > 0".to_string(),
            });
        }
        if start % chunk_size != 0 {
            return Err(PeerStorageError::ProtocolError {
                reason: format!(
                    "multipart range start {start} is not chunk-aligned (chunk_size {chunk_size})"
                ),
            });
        }
        let start_chunk_index = (start / chunk_size) as usize;
        let chunks_in_range = part_size.div_ceil(chunk_size) as usize;
        let end_chunk_index = start_chunk_index + chunks_in_range;
        if end_chunk_index > chunks.chunk_hashes.len() {
            return Err(PeerStorageError::ProtocolError {
                reason: format!(
                    "multipart range [{start}, {end}) covers chunks [{start_chunk_index}, {end_chunk_index}) but manifest only has {} chunks",
                    chunks.chunk_hashes.len()
                ),
            });
        }
        let expected_hashes_for_range = &chunks.chunk_hashes[start_chunk_index..end_chunk_index];

        let mut writer = tokio::io::BufWriter::new(file);

        // Per-chunk progress: each verified chunk reports its delta. We add it
        // to this range's running tally (so a failed attempt can be rolled
        // back by the pool's on_retry hook before the retry) and to the shared
        // cross-worker total, then report (bytes_done, file_size) to the
        // caller. cancel/pause are honoured at chunk boundaries inside the pipe.
        let total_received_pc = total_received.clone();
        let range_progress_pc = range_progress.clone();
        let on_progress_pc = on_progress.clone();
        let range_key = (start, end);
        let on_chunk: Option<Box<dyn FnMut(u64) + Send>> = Some(Box::new(move |delta: u64| {
            {
                let mut rp = range_progress_pc.lock().unwrap_or_else(|e| e.into_inner());
                *rp.entry(range_key).or_insert(0) += delta;
            }
            let new_total = total_received_pc.fetch_add(delta, Ordering::Relaxed) + delta;
            if let Some(cb) = on_progress_pc.as_ref() {
                cb(new_total.min(total_size), total_size);
            }
        }));
        let controls = streaming::VerifiedRecvControls {
            on_chunk,
            cancel_token,
            pause_flag,
        };

        // ChunkVerifier shares the bitmap directly with sibling workers via
        // the Arc — no local clone, no end-of-attempt merge. Each chunk
        // flips its own bit under the mutex and snapshots the bitmap in the
        // same critical section for the sidecar save, so two workers can
        // never race and overwrite each other's bits in the persisted
        // sidecar.
        let verifier = streaming::ChunkVerifier {
            expected_chunk_hashes: expected_hashes_for_range,
            chunk_size: chunks.chunk_size,
            start_chunk_index,
            completed: completed.clone(),
        };

        // pipe_recv_to_writer_verified persists the sidecar against
        // `final_output_path` (the destination the partial file will be
        // renamed to), so resume on the next invocation can find it.
        let recv_result = streaming::pipe_recv_to_writer_verified(
            &mut recv,
            &mut writer,
            part_size,
            verifier,
            Some((final_output_path, &chunks.file_hash)),
            controls,
        )
        .await;

        let _ = tokio::io::AsyncWriteExt::flush(&mut writer).await;
        drop(writer);

        match recv_result {
            Ok(received) => {
                if received != part_size {
                    return Err(PeerStorageError::ConnectionFailed {
                        reason: format!(
                            "multipart range short: expected {part_size}, received {received}"
                        ),
                    });
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}
