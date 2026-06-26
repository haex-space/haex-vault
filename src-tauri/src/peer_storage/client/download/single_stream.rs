use std::sync::Arc;

use iroh::{EndpointId, RelayUrl};

use crate::file_sync::hashing::ChunkedHash;
use crate::peer_storage::client::StreamReadResult;
use crate::peer_storage::endpoint::PeerEndpoint;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{Request, Response};
use crate::peer_storage::streaming;

/// Single-stream download with resume support.
///
/// First-attempt downloads (no sidecar, or sidecar's file_hash disagrees with
/// the manifest) go through Task 7's full-file verified receive path
/// unchanged. When a sidecar matches, this helper skips the chunks the
/// previous attempt already verified, requesting only the missing ranges and
/// landing their bytes in the surviving `<output_path>.haex-partial` file at
/// the correct offset before atomically renaming to the final path.
///
/// Each missing range is requested via its own `Read { range }` request on a
/// fresh QUIC stream — the server uses one stream per request, so a single
/// recv stream can only carry one range. Range-Read responses are headed by
/// a `ReadHeader { size }` that the helper validates against the requested
/// span before piping into [`streaming::pipe_recv_to_writer_verified`].
pub(super) async fn download_single_stream_with_resume(
    endpoint: Arc<tokio::sync::RwLock<PeerEndpoint>>,
    remote_id: EndpointId,
    relay_url: Option<RelayUrl>,
    path: &str,
    output_path: &std::path::Path,
    file_size: u64,
    chunks_to_use: &ChunkedHash,
    on_progress: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    pause_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    ucan_token: &str,
) -> Result<StreamReadResult, PeerStorageError> {
    // Sidecar lookup runs first so the fresh-path branch below is structurally
    // identical to what Task 7 shipped — anything new lives on the resume side.
    let existing = crate::peer_storage::resume::PartialState::load_if_matches(
        output_path,
        &chunks_to_use.file_hash,
    )
    .await
    .map_err(PeerStorageError::Io)?;

    // Sidecar metadata can survive even when the `.haex-partial` bytes file
    // is gone or truncated. The resume loop below opens that file with
    // `create(false)` and seeks to per-range offsets — both fail on a missing
    // or short file. Validate length matches the expected total before
    // honoring the sidecar; otherwise clear the stale state and fall back to
    // the fresh-download path.
    let existing = match existing {
        Some(state) => {
            let partial_path = crate::peer_storage::resume::PartialState::partial_path(output_path);
            let bytes_ok = tokio::fs::metadata(&partial_path)
                .await
                .map(|m| m.len() == file_size)
                .unwrap_or(false);
            if bytes_ok {
                Some(state)
            } else {
                let _ = crate::peer_storage::resume::PartialState::clear(output_path).await;
                None
            }
        }
        None => None,
    };

    // No surviving partial → delegate to Task 7's fresh-download path verbatim.
    // Likewise if every chunk in the sidecar is already done (which shouldn't
    // happen in normal flow but is cheap to guard) or if no chunk is done yet —
    // in both edge cases there's no benefit to going through the range-Read
    // loop, so we re-use the simpler single-shot path.
    let Some(state) =
        existing.filter(|s| s.completed.iter().any(|c| *c) && !s.completed.iter().all(|c| *c))
    else {
        let (mut send, mut recv) = endpoint
            .read()
            .await
            .open_stream(remote_id, relay_url)
            .await?;
        let on_progress_boxed: Option<Box<dyn Fn(u64, u64) + Send>> = on_progress.map(|cb| {
            Box::new(move |done: u64, total: u64| cb(done, total)) as Box<dyn Fn(u64, u64) + Send>
        });
        return PeerEndpoint::read_open_streams_to_file(
            &mut send,
            &mut recv,
            path,
            output_path,
            on_progress_boxed,
            cancel_token,
            pause_flag,
            ucan_token,
            chunks_to_use,
        )
        .await;
    };

    // Resume path: completed bitmap survives across attempts; only the
    // false-runs need re-requesting. The sidecar was load_if_matches-guarded
    // so we know its file_hash equals the manifest we're verifying against.
    //
    // Sweep orphaned `.meta.tmp.<nonce>` files before the resume loop opens
    // any new streams. Unlike the fresh path (which sweeps inside
    // receive_with_chunk_verification) the resume branch never reaches that
    // function, so prior interrupted saves would otherwise accumulate
    // unbounded across retries. The resume sidecar pair (`.meta` /
    // `.haex-partial`) is preserved by sweep_tmp.
    let _ = crate::peer_storage::resume::PartialState::sweep_tmp(output_path).await;

    let chunk_size = chunks_to_use.chunk_size as u64;
    let total_chunks = chunks_to_use.chunk_hashes.len();

    // Defensive contract checks. PartialState was persisted by Task 7's path
    // so a healthy sidecar will satisfy these — only a tampered or otherwise
    // corrupt sidecar would trip them. Surfacing as a ProtocolError lets the
    // caller see a clean failure instead of silently mis-aligning chunks.
    if state.completed.len() != total_chunks {
        return Err(PeerStorageError::ProtocolError {
            reason: format!(
                "sidecar chunk count {} disagrees with manifest chunk count {}",
                state.completed.len(),
                total_chunks
            ),
        });
    }
    if state.chunk_size != chunks_to_use.chunk_size {
        return Err(PeerStorageError::ProtocolError {
            reason: format!(
                "sidecar chunk_size {} disagrees with manifest chunk_size {}",
                state.chunk_size, chunks_to_use.chunk_size
            ),
        });
    }

    let partial_path = crate::peer_storage::resume::PartialState::partial_path(output_path);

    // Shared bitmap for the resume loop. Seeded from the surviving sidecar
    // so already-verified chunks stay marked done, then mutated by the
    // verifier as each missing range fills in. Wrapping it in a tokio Mutex
    // is the same API the multi-stream path uses — even though only one
    // worker drives it here, consistency wins over micro-optimising for the
    // uncontended case.
    let completed: Arc<tokio::sync::Mutex<Vec<bool>>> =
        Arc::new(tokio::sync::Mutex::new(state.completed.clone()));

    // Bytes already on disk before any missing range fills in. Seed the
    // progress counter with this so the consumer never sees it jump backwards
    // when a resume picks up mid-file. Shared across the per-range pipes via an
    // atomic so each verified chunk can bump it from the pipe's on_chunk hook.
    let already_done_bytes: u64 = {
        let cs = chunks_to_use.chunk_size as u64;
        let guard = completed.lock().await;
        let n_done = guard.iter().filter(|c| **c).count() as u64;
        (n_done * cs).min(file_size)
    };
    let sent = Arc::new(std::sync::atomic::AtomicU64::new(already_done_bytes));
    if let Some(cb) = on_progress.as_ref() {
        cb(already_done_bytes.min(file_size), file_size);
    }

    for (range_start, range_end_raw) in state.missing_ranges() {
        // missing_ranges() rounds end up to a chunk boundary regardless of
        // file size, so the last range can overshoot the actual file end.
        // Clamp it before sending — the server already clamps too, but
        // letting it announce a smaller size than we asked would trip the
        // size-mismatch check below.
        let range_end = range_end_raw.min(file_size);
        if range_end <= range_start {
            continue;
        }
        let range_len = range_end - range_start;

        // Map this byte range back to chunk-bitmap indices. Both bounds are
        // chunk-aligned on the start side; the end may land mid-chunk on the
        // final range, which is fine — div_ceil rounds up so the last chunk
        // is still counted exactly once.
        let start_chunk_index = (range_start / chunk_size) as usize;
        let chunks_in_range = range_len.div_ceil(chunk_size) as usize;
        let end_chunk_index = start_chunk_index + chunks_in_range;
        if end_chunk_index > total_chunks {
            return Err(PeerStorageError::ProtocolError {
                reason: format!(
                    "missing range [{range_start}, {range_end}) covers chunks [{start_chunk_index}, {end_chunk_index}) but manifest only has {total_chunks} chunks"
                ),
            });
        }
        let expected_hashes_for_range =
            &chunks_to_use.chunk_hashes[start_chunk_index..end_chunk_index];

        // Abort promptly if cancelled before spending a stream on this range.
        if cancel_token.as_ref().is_some_and(|t| t.is_cancelled()) {
            return Err(PeerStorageError::Cancelled);
        }

        // Open a fresh stream + send a Read for this range. The server's
        // wire range is half-open [start, end), matching missing_ranges().
        let (mut send, mut recv) = endpoint
            .read()
            .await
            .open_stream(remote_id, relay_url.clone())
            .await?;
        let req = Request::Read {
            path: path.to_string(),
            range: Some([range_start, range_end]),
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
                    reason: "unexpected response on resume range read".to_string(),
                });
            }
        };
        if announced != range_len {
            return Err(PeerStorageError::ProtocolError {
                reason: format!(
                    "resume range size mismatch: requested {range_len} bytes for [{range_start}, {range_end}), peer announced {announced}"
                ),
            });
        }

        // Open the partial bytes file with create(false) — the previous
        // attempt left it on disk with the verified-chunk content. Seek to
        // the start of this missing range so the writer lands bytes at the
        // correct file offset.
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(false)
            .open(&partial_path)
            .await
            .map_err(PeerStorageError::Io)?;
        use tokio::io::AsyncSeekExt;
        file.seek(std::io::SeekFrom::Start(range_start))
            .await
            .map_err(PeerStorageError::Io)?;
        let mut writer = tokio::io::BufWriter::new(file);

        let verifier = streaming::ChunkVerifier {
            expected_chunk_hashes: expected_hashes_for_range,
            chunk_size: chunks_to_use.chunk_size,
            start_chunk_index,
            completed: completed.clone(),
        };

        // Per-chunk progress: bump the shared counter and report a monotonic
        // (bytes_done, file_size) as each verified chunk lands. cancel/pause
        // are honoured at chunk boundaries inside the pipe.
        let on_chunk: Option<Box<dyn FnMut(u64) + Send>> = on_progress.as_ref().map(|cb| {
            let cb = cb.clone();
            let sent = sent.clone();
            Box::new(move |delta: u64| {
                let n = sent.fetch_add(delta, std::sync::atomic::Ordering::Relaxed) + delta;
                cb(n.min(file_size), file_size);
            }) as Box<dyn FnMut(u64) + Send>
        });
        let controls = streaming::VerifiedRecvControls {
            on_chunk,
            cancel_token: cancel_token.clone(),
            pause_flag: pause_flag.clone(),
        };

        let result = streaming::pipe_recv_to_writer_verified(
            &mut recv,
            &mut writer,
            range_len,
            verifier,
            Some((output_path, &chunks_to_use.file_hash)),
            controls,
        )
        .await;

        // Flush whatever made it through so a follow-up resume can re-use
        // the bytes from this attempt. Tokio's BufWriter does NOT flush on
        // drop (no async Drop), so an unflushed buffer would silently lose
        // the tail of the last chunk. Primary recv error still wins (it
        // caused the truncated buffer), but an otherwise-successful recv
        // must not be reported as Ok if flushing failed.
        let flush_result = tokio::io::AsyncWriteExt::flush(&mut writer).await;
        drop(writer);

        // Propagate the failure up so the caller can react. The partial
        // bytes + sidecar are left on disk by design — that's the entire
        // point of the resume contract.
        let received = match (result, flush_result) {
            (Err(e), _) => return Err(e),
            (Ok(_), Err(e)) => return Err(PeerStorageError::Io(e)),
            (Ok(r), Ok(())) => r,
        };
        if received != range_len {
            return Err(PeerStorageError::ConnectionFailed {
                reason: format!(
                    "resume range short: requested {range_len} bytes for [{range_start}, {range_end}), received {received}"
                ),
            });
        }

        // Progress is reported per chunk from inside the pipe (above), so the
        // shared counter already reflects this range's bytes — nothing to add
        // at the range boundary.
    }

    // All missing ranges drained. Sanity-check that every chunk in the
    // bitmap is now true — if not, missing_ranges() returned a set that
    // didn't cover the full file (bug, not transport failure), and we
    // would otherwise rename a still-incomplete file into place.
    if !completed.lock().await.iter().all(|c| *c) {
        return Err(PeerStorageError::ProtocolError {
            reason: "resume completed all missing ranges but bitmap still has gaps".to_string(),
        });
    }

    tokio::fs::rename(&partial_path, output_path)
        .await
        .map_err(PeerStorageError::Io)?;
    crate::peer_storage::resume::PartialState::clear(output_path)
        .await
        .map_err(PeerStorageError::Io)?;

    Ok(StreamReadResult {
        bytes: file_size,
        hash: Some(chunks_to_use.file_hash.clone()),
    })
}
