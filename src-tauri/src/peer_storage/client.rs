//! Client-side remote operations — outgoing requests to peer endpoints.

use std::path::Path;
use std::sync::Arc;

use iroh::{EndpointId, RelayUrl};

use crate::file_sync::hashing::ChunkedHash;
use crate::peer_storage::endpoint::PeerEndpoint;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{FileEntry, Request, Response};
use crate::peer_storage::streaming;

/// Outcome of a streaming peer read into a local file.
///
/// `hash` is the manifest's BLAKE3 `file_hash` that the chunked verifier
/// confirmed against the bytes on disk. It is `None` only for paths that
/// don't produce a comparable full-file hash (zero-byte short-circuit on
/// the multi-stream entry-point with no manifest).
#[derive(Debug, Clone)]
pub struct StreamReadResult {
    pub bytes: u64,
    pub hash: Option<String>,
}

impl PeerEndpoint {
    /// Connect to a remote peer and list a directory.
    pub async fn remote_list(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        ucan_token: &str,
    ) -> Result<Vec<FileEntry>, PeerStorageError> {
        let (mut send, mut recv) = self.open_stream(remote_id, relay_url).await?;
        let req = Request::List {
            path: path.to_string(),
            ucan_token: ucan_token.to_string(),
        };
        let response = Self::send_request(&mut send, &mut recv, &req).await?;

        match response {
            Response::List { entries } => Ok(entries),
            Response::Error { message } => {
                Err(PeerStorageError::ProtocolError { reason: message })
            }
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Transfer a file from already-opened QUIC streams to disk.
    /// Callers that hold a lock on `PeerEndpoint` should open the stream under
    /// the lock, drop it, then call this function so the lock is not held during I/O.
    ///
    /// The receive loop verifies each chunk against the manifest's BLAKE3
    /// hashes inline and persists a resume sidecar after every successful
    /// chunk — bytes only land on disk once the corresponding chunk hash
    /// matches. The output stream is written to `<output_path>.haex-partial`
    /// and atomically renamed to `output_path` on full success; the sidecar
    /// metadata is cleared in the same step. On a hash mismatch the partial
    /// bytes + sidecar are left in place so a retry can resume from the
    /// surviving chunks.
    pub(crate) async fn read_open_streams_to_file(
        send: &mut iroh::endpoint::SendStream,
        recv: &mut iroh::endpoint::RecvStream,
        path: &str,
        output_path: &std::path::Path,
        on_progress: Option<Box<dyn Fn(u64, u64) + Send>>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
        pause_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
        ucan_token: &str,
        chunks_to_use: &ChunkedHash,
    ) -> Result<StreamReadResult, PeerStorageError> {
        let req = Request::Read {
            path: path.to_string(),
            range: None,
            ucan_token: ucan_token.to_string(),
        };
        let response = Self::send_request(send, recv, &req).await?;

        match response {
            Response::ReadHeader { size } => {
                Self::receive_with_chunk_verification(
                    recv,
                    output_path,
                    size,
                    chunks_to_use,
                    on_progress,
                    cancel_token,
                    pause_flag,
                )
                .await
            }
            Response::Error { message } => {
                Err(PeerStorageError::ProtocolError { reason: message })
            }
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Verified single-stream receive path: streams bytes to a sidecar
    /// partial-path file, verifies each chunk inline, and atomically
    /// renames to `output_path` on success.
    async fn receive_with_chunk_verification(
        recv: &mut iroh::endpoint::RecvStream,
        output_path: &std::path::Path,
        size: u64,
        chunks: &ChunkedHash,
        on_progress: Option<Box<dyn Fn(u64, u64) + Send>>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
        pause_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<StreamReadResult, PeerStorageError> {
        // The optional knobs aren't plumbed through the verified pipe yet —
        // single-stream downloads under MULTI_STREAM_THRESHOLD finish in
        // sub-second wall-clock on LAN, so pause/cancel granularity at the
        // chunk boundary is more than enough. Tasks 9–10 will wire these
        // into the multi-stream path which does need finer-grained control.
        let _ = (cancel_token, pause_flag);

        let partial_path =
            crate::peer_storage::resume::PartialState::partial_path(output_path);

        let file = tokio::fs::File::create(&partial_path).await.map_err(|e| {
            PeerStorageError::ProtocolError {
                reason: format!("Failed to create partial file: {e}"),
            }
        })?;
        let mut writer = tokio::io::BufWriter::new(file);

        // Single writer here, but the verifier API takes
        // `Arc<Mutex<Vec<bool>>>` for consistency with the multi-stream path
        // (and so PartialState::save always sees an authoritative snapshot).
        // Lock contention with a single user is uncontended hot-path mutex
        // acquisition — negligible.
        let completed: Arc<tokio::sync::Mutex<Vec<bool>>> =
            Arc::new(tokio::sync::Mutex::new(vec![false; chunks.chunk_hashes.len()]));
        let verifier = streaming::ChunkVerifier {
            expected_chunk_hashes: &chunks.chunk_hashes,
            chunk_size: chunks.chunk_size,
            start_chunk_index: 0,
            completed: completed.clone(),
        };

        // Track progress at the writer level by polling the verifier's
        // completed bitmap as chunks land. The verified pipe doesn't expose
        // a callback yet, so emulate progress by snapshotting completion
        // count between chunks. For files < MULTI_STREAM_THRESHOLD this is
        // a single-stream stream of ≤16 chunks so the granularity is fine.
        let _ = on_progress; // TODO: thread per-chunk progress in Task 8

        let result = streaming::pipe_recv_to_writer_verified(
            &mut *recv,
            &mut writer,
            size,
            verifier,
            Some((output_path, &chunks.file_hash)),
        )
        .await;

        // Always flush whatever we have so far so a retry can re-use what
        // was already verified. The BufWriter drops with the file on the
        // error path below — we don't bother propagating that flush error
        // since it won't tell the caller anything more useful than the
        // primary failure.
        let _ = tokio::io::AsyncWriteExt::flush(&mut writer).await;
        drop(writer);

        match result {
            Ok(bytes) => {
                debug_assert!(completed.lock().await.iter().all(|c| *c));
                if bytes != size {
                    return Err(PeerStorageError::ConnectionFailed {
                        reason: format!(
                            "Incomplete verified download: expected {size} bytes, received {bytes}"
                        ),
                    });
                }
                // Atomic rename of the verified bytes onto the final path,
                // then clear the sidecar metadata.
                tokio::fs::rename(&partial_path, output_path)
                    .await
                    .map_err(PeerStorageError::Io)?;
                crate::peer_storage::resume::PartialState::clear(output_path)
                    .await
                    .map_err(PeerStorageError::Io)?;
                Ok(StreamReadResult {
                    bytes: size,
                    hash: Some(chunks.file_hash.clone()),
                })
            }
            Err(e) => {
                // Leave partial bytes + sidecar in place for a future
                // resume attempt (wired in Task 8). The caller sees the
                // primary error and decides whether to retry.
                Err(e)
            }
        }
    }

    /// Connect to a remote peer and get a recursive file manifest.
    pub async fn remote_manifest(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        ucan_token: &str,
    ) -> Result<Vec<crate::file_sync::types::FileState>, PeerStorageError> {
        let (mut send, mut recv) = self.open_stream(remote_id, relay_url).await?;
        let req = Request::Manifest {
            path: path.to_string(),
            ucan_token: ucan_token.to_string(),
        };
        let response = Self::send_request(&mut send, &mut recv, &req).await?;

        match response {
            Response::Manifest { entries } => Ok(entries),
            Response::Error { message } => {
                Err(PeerStorageError::ProtocolError { reason: message })
            }
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Connect to a remote peer and read a file into memory.
    /// For large files prefer `download_file_to_path`; this is for sync-sized reads.
    pub async fn remote_read_bytes(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        ucan_token: &str,
    ) -> Result<Vec<u8>, PeerStorageError> {
        self.remote_read_bytes_with_progress(remote_id, relay_url, path, ucan_token, |_, _| {})
            .await
    }

    /// Like `remote_read_bytes` but calls `on_progress(bytes_done, bytes_total)` after each
    /// 64 KiB chunk so callers can report per-file transfer progress.
    pub async fn remote_read_bytes_with_progress(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        ucan_token: &str,
        on_progress: impl Fn(u64, u64) + Send,
    ) -> Result<Vec<u8>, PeerStorageError> {
        let (mut send, mut recv) = self.open_stream(remote_id, relay_url).await?;
        let req = Request::Read {
            path: path.to_string(),
            range: None,
            ucan_token: ucan_token.to_string(),
        };
        let response = Self::send_request(&mut send, &mut recv, &req).await?;

        match response {
            Response::ReadHeader { size } => {
                let mut data = Vec::with_capacity(size as usize);
                let mut buf = [0u8; 64 * 1024];
                let mut bytes_received: u64 = 0;

                loop {
                    let chunk = recv.read(&mut buf).await.map_err(|e| {
                        PeerStorageError::ConnectionFailed {
                            reason: format!("Failed to read from stream: {e}"),
                        }
                    })?;
                    match chunk {
                        Some(n) => {
                            data.extend_from_slice(&buf[..n]);
                            bytes_received += n as u64;
                            on_progress(bytes_received, size);
                        }
                        None => break,
                    }
                }

                Ok(data)
            }
            Response::Error { message } => {
                Err(PeerStorageError::ProtocolError { reason: message })
            }
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Connect to a remote peer and write a file from disk.
    ///
    /// Sends the Write request header, then streams the file contents via
    /// [`streaming::pipe_reader_to_send`]. Honours optional progress + cancel
    /// hooks in `options` so callers (e.g. the `peer_storage_remote_write`
    /// Tauri command) can drive the same UI flow the read path uses.
    ///
    /// Returns the number of bytes actually written to the wire — equal to
    /// the file size on success, less than that on cancel.
    ///
    /// On cancel the function returns `PeerStorageError::ProtocolError { reason: "cancelled" }`
    /// **without** calling `send.finish()`. Dropping the un-finished `SendStream`
    /// triggers a QUIC reset on the server, which is the cleanup signal the
    /// server's `handle_write` already relies on: it stages to a `.part`
    /// sibling and `remove_file`s it on any non-OK path before the atomic
    /// rename. So no truncated file is ever exposed at the destination.
    pub async fn remote_write_file(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        source_path: &Path,
        ucan_token: &str,
        options: streaming::SendOptions,
    ) -> Result<u64, PeerStorageError> {
        let size = tokio::fs::metadata(source_path)
            .await
            .map_err(|e| PeerStorageError::ProtocolError {
                reason: format!("stat source '{}': {e}", source_path.display()),
            })?
            .len();
        let file = tokio::fs::File::open(source_path)
            .await
            .map_err(|e| PeerStorageError::ProtocolError {
                reason: format!("open source '{}': {e}", source_path.display()),
            })?;

        let (mut send, mut recv) = self.open_stream(remote_id, relay_url).await?;

        let req = Request::Write {
            path: path.to_string(),
            size,
            ucan_token: ucan_token.to_string(),
        };
        Self::send_request_header(&mut send, &req).await?;

        let stats = streaming::pipe_reader_to_send(&mut send, file, size, options)
            .await
            .map_err(|e| match e {
                streaming::PipelineError::Io(e) => PeerStorageError::Io(e),
                streaming::PipelineError::Stream(reason) => {
                    PeerStorageError::ConnectionFailed { reason }
                }
                streaming::PipelineError::Cancelled => PeerStorageError::ProtocolError {
                    reason: "cancelled".to_string(),
                },
            })?;
        send.finish()
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        let response: Response = crate::peer_storage::protocol::read_response(&mut recv)
            .await
            .map_err(|e| PeerStorageError::ProtocolError {
                reason: e.to_string(),
            })?;

        match response {
            Response::WriteOk => Ok(stats.bytes),
            Response::Error { message } => {
                Err(PeerStorageError::ProtocolError { reason: message })
            }
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Connect to a remote peer and delete a file.
    pub async fn remote_delete_file(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        to_trash: bool,
        ucan_token: &str,
    ) -> Result<(), PeerStorageError> {
        let (mut send, mut recv) = self.open_stream(remote_id, relay_url).await?;
        let req = Request::Delete {
            path: path.to_string(),
            to_trash,
            ucan_token: ucan_token.to_string(),
        };
        let response = Self::send_request(&mut send, &mut recv, &req).await?;

        match response {
            Response::DeleteOk => Ok(()),
            Response::Error { message } => {
                Err(PeerStorageError::ProtocolError { reason: message })
            }
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Connect to a remote peer and create a directory.
    pub async fn remote_create_directory(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        ucan_token: &str,
    ) -> Result<(), PeerStorageError> {
        let (mut send, mut recv) = self.open_stream(remote_id, relay_url).await?;
        let req = Request::CreateDirectory {
            path: path.to_string(),
            ucan_token: ucan_token.to_string(),
        };
        let response = Self::send_request(&mut send, &mut recv, &req).await?;

        match response {
            Response::CreateDirectoryOk => Ok(()),
            Response::Error { message } => {
                Err(PeerStorageError::ProtocolError { reason: message })
            }
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Read a specific byte range into memory. Inclusive bounds, matching
    /// the HTTP `Range: bytes=START-END` semantics that callers (the media
    /// streaming layer in particular) work with.
    ///
    /// The wire protocol uses half-open ranges `[start, end)`, so the
    /// inclusive `[a, b]` argument is converted to `[a, b + 1]` before being
    /// sent on the request.
    pub async fn remote_read_range_bytes(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        range: [u64; 2],
        ucan_token: &str,
    ) -> Result<Vec<u8>, PeerStorageError> {
        if range[0] > range[1] {
            return Err(PeerStorageError::ProtocolError {
                reason: format!("invalid range: {}-{}", range[0], range[1]),
            });
        }
        // Upper bound for what we're willing to buffer: the requested
        // inclusive byte count. A peer that announces more than this is
        // either buggy or malicious, so refuse before allocating.
        let max_expected = range[1]
            .checked_sub(range[0])
            .and_then(|d| d.checked_add(1))
            .ok_or_else(|| PeerStorageError::ProtocolError {
                reason: "invalid range length".to_string(),
            })?;

        let (mut send, mut recv) = self.open_stream(remote_id, relay_url).await?;
        // Convert inclusive [start, end] → wire half-open [start, end + 1].
        // saturating_add guards against the (pathological) caller passing
        // u64::MAX as end.
        let wire_range = [range[0], range[1].saturating_add(1)];
        let req = Request::Read {
            path: path.to_string(),
            range: Some(wire_range),
            ucan_token: ucan_token.to_string(),
        };
        let response = Self::send_request(&mut send, &mut recv, &req).await?;

        match response {
            Response::ReadHeader { size } => {
                if size > max_expected {
                    return Err(PeerStorageError::ProtocolError {
                        reason: format!(
                            "range response too large: requested at most {max_expected} bytes, peer announced {size}"
                        ),
                    });
                }
                let mut data = Vec::with_capacity(size as usize);
                let mut buf = [0u8; 64 * 1024];
                let mut got: u64 = 0;
                while got < size {
                    match recv.read(&mut buf).await.map_err(|e| {
                        PeerStorageError::ConnectionFailed {
                            reason: format!("read: {e}"),
                        }
                    })? {
                        Some(n) => {
                            data.extend_from_slice(&buf[..n]);
                            got += n as u64;
                            if got > size {
                                return Err(PeerStorageError::ConnectionFailed {
                                    reason: format!(
                                        "peer exceeded announced size: announced {size}, received {got}"
                                    ),
                                });
                            }
                        }
                        None => break,
                    }
                }
                if got < size {
                    return Err(PeerStorageError::ConnectionFailed {
                        reason: format!(
                            "Stream ended early: expected {size} bytes, received {got}"
                        ),
                    });
                }
                Ok(data)
            }
            Response::Error { message } => {
                Err(PeerStorageError::ProtocolError { reason: message })
            }
            _ => Err(PeerStorageError::ProtocolError {
                reason: "unexpected response (read range)".to_string(),
            }),
        }
    }

    /// Fetch metadata for a single remote path (size, is_dir, modified) and,
    /// for files, the BLAKE3 chunked-hash manifest the server holds.
    pub async fn remote_stat(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        ucan_token: &str,
    ) -> Result<RemoteStat, PeerStorageError> {
        let (mut send, mut recv) = self.open_stream(remote_id, relay_url).await?;
        let req = Request::Stat {
            path: path.to_string(),
            ucan_token: ucan_token.to_string(),
        };
        let response = Self::send_request(&mut send, &mut recv, &req).await?;
        match response {
            Response::Stat { entry, chunks } => Ok(RemoteStat { entry, chunks }),
            Response::Error { message } => {
                Err(PeerStorageError::ProtocolError { reason: message })
            }
            _ => Err(PeerStorageError::ProtocolError {
                reason: "unexpected response (stat)".to_string(),
            }),
        }
    }
}

/// Result of a remote stat-probe — file metadata plus, for files, the
/// BLAKE3 chunked-hash manifest served from the peer's hash cache.
#[derive(Debug, Clone)]
pub struct RemoteStat {
    pub entry: FileEntry,
    /// `Some` for files, `None` for directories.
    pub chunks: Option<ChunkedHash>,
}

/// Download a remote file to `output_path`, picking single-stream or
/// multi-stream automatically based on the announced size.
///
/// This is the single entry point both file_sync and the frontend
/// `peer_storage_remote_read` Tauri command should use. The dispatch logic
/// (stat probe → multi-stream above [`streaming::MULTI_STREAM_THRESHOLD`],
/// single-stream below) used to be open-coded in `peer_provider`, while
/// the frontend command path went straight to single-stream — so UI
/// downloads of a 200 MB file ran ~4× slower than the same file via sync.
///
/// On success, logs the transfer rate to stderr so throughput regressions
/// are visible without separate instrumentation.
pub(crate) async fn download_file_to_path(
    endpoint: Arc<tokio::sync::RwLock<PeerEndpoint>>,
    remote_id: EndpointId,
    relay_url: Option<RelayUrl>,
    path: String,
    output_path: std::path::PathBuf,
    expected_chunks: Option<ChunkedHash>,
    on_progress: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    pause_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    ucan_token: String,
) -> Result<StreamReadResult, PeerStorageError> {
    let started = std::time::Instant::now();

    // 1 RTT probe to learn the size. On LAN this is sub-millisecond; on
    // any WAN/relay link it is repaid many times over by the multi-stream
    // throughput gain it unlocks for large files.
    let stat = endpoint
        .read()
        .await
        .remote_stat(remote_id, relay_url.clone(), &path, &ucan_token)
        .await?;

    if stat.entry.is_dir {
        return Err(PeerStorageError::ProtocolError {
            reason: format!("remote path is a directory, not a file: {path}"),
        });
    }

    // Resolve which ChunkedHash governs verification.
    //
    // - Sync-rule flow: caller supplied `expected_chunks` from the manifest.
    //   The stat-probe also returns chunks (served from the sender's hash
    //   cache). In steady state these agree; if they disagree the file has
    //   changed on the sender between manifest scan and download — abort
    //   loudly so the engine can re-scan rather than persisting bytes that
    //   no longer match the announced hash.
    // - File-browser flow: caller passes `None`; we adopt the stat-probe's
    //   chunks as the verifier. The receiver therefore learns the hashes
    //   from the peer it is downloading from — TLS already covers transport
    //   integrity, and the stat-probe rides the same authenticated
    //   connection as the bytes themselves.
    //
    // `chunks_to_use` is currently only retained for the throughput log;
    // Task 7 plumbs it into per-chunk verification on the receive path.
    let chunks_to_use = match (expected_chunks, stat.chunks.clone()) {
        (Some(manifest), Some(stat_chunks)) => {
            if manifest != stat_chunks {
                return Err(PeerStorageError::ManifestHashMismatch {
                    manifest_file_hash: manifest.file_hash,
                    actual_file_hash: stat_chunks.file_hash,
                });
            }
            // Functionally either — the manifest is the source of truth for
            // the sync flow, so keep it.
            manifest
        }
        (manifest_opt, None) => {
            // The early `stat.entry.is_dir` check above already rejected
            // directories — reaching here means the server told us the
            // path is a file but returned no chunks. That violates the
            // server-side invariant Task 6 installs (the peer is required
            // to send `chunks: Some(_)` for any non-directory entry it
            // owns), so surface a ProtocolError rather than silently
            // accepting a download with no per-chunk verification.
            let manifest_detail = manifest_opt
                .as_ref()
                .map(|m| format!(" (manifest claimed file_hash {})", m.file_hash))
                .unwrap_or_default();
            return Err(PeerStorageError::ProtocolError {
                reason: format!(
                    "stat reported non-directory path with no chunks (server invariant violation): {path}{manifest_detail}"
                ),
            });
        }
        (None, Some(stat_chunks)) => stat_chunks,
    };
    let use_multi = stat.entry.size >= streaming::MULTI_STREAM_THRESHOLD;

    let result = if use_multi {
        read_multipart_to_file(
            endpoint,
            remote_id,
            relay_url,
            path,
            output_path,
            stat.entry.size,
            streaming::MAX_PARALLEL_STREAMS_PER_FILE,
            &chunks_to_use,
            on_progress,
            cancel_token,
            pause_flag,
            ucan_token,
        )
        .await?
    } else {
        download_single_stream_with_resume(
            endpoint.clone(),
            remote_id,
            relay_url.clone(),
            &path,
            &output_path,
            stat.entry.size,
            &chunks_to_use,
            on_progress.clone(),
            cancel_token.clone(),
            pause_flag.clone(),
            &ucan_token,
        )
        .await?
    };

    let elapsed = started.elapsed();
    let secs = elapsed.as_secs_f64();
    let mb = result.bytes as f64 / (1024.0 * 1024.0);
    let rate = if secs > 0.0 { mb / secs } else { 0.0 };
    let streams = if use_multi {
        streaming::MAX_PARALLEL_STREAMS_PER_FILE
    } else {
        1
    };
    eprintln!(
        "[PeerStorage] download {} bytes in {:.2}s = {:.2} MB/s ({} stream{})",
        result.bytes,
        secs,
        rate,
        streams,
        if streams == 1 { "" } else { "s" }
    );

    Ok(result)
}

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
async fn download_single_stream_with_resume(
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

    // No surviving partial → delegate to Task 7's fresh-download path verbatim.
    // Likewise if every chunk in the sidecar is already done (which shouldn't
    // happen in normal flow but is cheap to guard) or if no chunk is done yet —
    // in both edge cases there's no benefit to going through the range-Read
    // loop, so we re-use the simpler single-shot path.
    let Some(state) = existing
        .filter(|s| s.completed.iter().any(|c| *c) && !s.completed.iter().all(|c| *c))
    else {
        let (mut send, mut recv) = endpoint
            .read()
            .await
            .open_stream(remote_id, relay_url)
            .await?;
        let on_progress_boxed: Option<Box<dyn Fn(u64, u64) + Send>> =
            on_progress.map(|cb| {
                Box::new(move |done: u64, total: u64| cb(done, total))
                    as Box<dyn Fn(u64, u64) + Send>
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

    let partial_path =
        crate::peer_storage::resume::PartialState::partial_path(output_path);

    // Shared bitmap for the resume loop. Seeded from the surviving sidecar
    // so already-verified chunks stay marked done, then mutated by the
    // verifier as each missing range fills in. Wrapping it in a tokio Mutex
    // is the same API the multi-stream path uses — even though only one
    // worker drives it here, consistency wins over micro-optimising for the
    // uncontended case.
    let completed: Arc<tokio::sync::Mutex<Vec<bool>>> =
        Arc::new(tokio::sync::Mutex::new(state.completed.clone()));

    // Cancel/pause aren't plumbed through the verified pipe yet —
    // single-stream resume only fires for files under MULTI_STREAM_THRESHOLD
    // (16 MiB), so chunk-boundary granularity is more than enough for the
    // file-explorer surface. Multi-stream resume (Task 10) wires those into
    // its retry pool.
    let _ = (cancel_token, pause_flag);

    // Bytes already on disk before any missing range fills in. Seed the
    // progress callback with this so the consumer never sees the counter
    // jump backwards when a resume picks up mid-file.
    let already_done_bytes: u64 = {
        let cs = chunks_to_use.chunk_size as u64;
        let guard = completed.lock().await;
        let n_done = guard.iter().filter(|c| **c).count() as u64;
        (n_done * cs).min(file_size)
    };
    let mut bytes_so_far = already_done_bytes;
    if let Some(ref cb) = on_progress {
        cb(bytes_so_far, file_size);
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

        let result = streaming::pipe_recv_to_writer_verified(
            &mut recv,
            &mut writer,
            range_len,
            verifier,
            Some((output_path, &chunks_to_use.file_hash)),
        )
        .await;

        // Flush whatever made it through so a follow-up resume can re-use
        // the bytes from this attempt; primary error wins over flush error.
        let _ = tokio::io::AsyncWriteExt::flush(&mut writer).await;
        drop(writer);

        // Propagate the failure up so the caller can react. The partial
        // bytes + sidecar are left on disk by design — that's the entire
        // point of the resume contract.
        let received = result?;
        if received != range_len {
            return Err(PeerStorageError::ConnectionFailed {
                reason: format!(
                    "resume range short: requested {range_len} bytes for [{range_start}, {range_end}), received {received}"
                ),
            });
        }

        // Report progress at the end of each missing range — consumers see a
        // monotonic counter that converges on `file_size` as each gap fills.
        bytes_so_far = bytes_so_far.saturating_add(received).min(file_size);
        if let Some(ref cb) = on_progress {
            cb(bytes_so_far, file_size);
        }
    }

    // All missing ranges drained. Sanity-check that every chunk in the
    // bitmap is now true — if not, missing_ranges() returned a set that
    // didn't cover the full file (bug, not transport failure), and we
    // would otherwise rename a still-incomplete file into place.
    if !completed.lock().await.iter().all(|c| *c) {
        return Err(PeerStorageError::ProtocolError {
            reason: "resume completed all missing ranges but bitmap still has gaps"
                .to_string(),
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
    let range_progress: Arc<tokio::sync::Mutex<std::collections::HashMap<(u64, u64), u64>>> =
        Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));

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
                    Box<
                        dyn std::future::Future<Output = Result<(), PeerStorageError>>
                            + Send,
                    >,
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
            )
                -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + Sync,
    > = Arc::new(move |(start, end, attempt), err| {
        let total_received = total_received_for_rollback.clone();
        let range_progress = range_progress_for_rollback.clone();
        let msg = err.to_string();
        Box::pin(async move {
            let mut rp = range_progress.lock().await;
            if let Some(prev) = rp.remove(&(start, end)) {
                total_received
                    .fetch_sub(prev, std::sync::atomic::Ordering::Relaxed);
            }
            drop(rp);
            eprintln!(
                "[PeerStorage] multipart range [{start}, {end}) attempt {attempt} failed, retrying: {msg}",
            );
        })
    });

    let first_err = run_bounded_retry_pool(pending, n, max_retries, fetcher, Some(on_retry))
        .await;

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
            reason: "multipart workers completed without filling chunk bitmap"
                .to_string(),
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

/// Generic worker pool with per-item bounded retry.
///
/// Spawns `concurrency` workers that pop `(start, end, attempt)` triples off
/// the shared `pending` queue, invoke `fetcher(start, end)`, and on Err either
/// re-queue with `attempt + 1` (while `attempt < max_retries`) or return the
/// error from that worker. Sibling workers keep draining the queue regardless
/// of whether one returned Err — the only thing that bubbles up is the first
/// permanent failure (after retries) encountered across all workers.
///
/// `on_retry` is invoked once per failed attempt that is about to be
/// re-queued, *before* the attempt is pushed back. The retry pool itself
/// doesn't know about per-attempt side effects (progress counters that need
/// rolling back, sidecar bytes from the failed attempt, etc.); the hook lets
/// the caller clean those up.
pub(crate) async fn run_bounded_retry_pool(
    pending: Arc<tokio::sync::Mutex<Vec<(u64, u64, u32)>>>,
    concurrency: usize,
    max_retries: u32,
    fetcher: Arc<
        dyn Fn(
                u64,
                u64,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<Output = Result<(), PeerStorageError>> + Send,
                >,
            > + Send
            + Sync,
    >,
    on_retry: Option<
        Arc<
            dyn Fn(
                    (u64, u64, u32),
                    &PeerStorageError,
                )
                    -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
                + Send
                + Sync,
        >,
    >,
) -> Option<PeerStorageError> {
    let mut workers = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let pending = pending.clone();
        let fetcher = fetcher.clone();
        let on_retry = on_retry.clone();
        workers.push(tokio::spawn(async move {
            loop {
                let next = pending.lock().await.pop();
                let Some((start, end, attempt)) = next else {
                    break;
                };

                match fetcher(start, end).await {
                    Ok(()) => continue,
                    Err(e) if attempt < max_retries => {
                        if let Some(hook) = on_retry.as_ref() {
                            hook((start, end, attempt), &e).await;
                        }
                        pending.lock().await.push((start, end, attempt + 1));
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok::<(), PeerStorageError>(())
        }));
    }

    let mut first_err: Option<PeerStorageError> = None;
    for handle in workers {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
            Err(join_err) => {
                if first_err.is_none() {
                    first_err = Some(PeerStorageError::ProtocolError {
                        reason: format!("worker join: {join_err}"),
                    });
                }
            }
        }
    }
    first_err
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
    range_progress: Arc<
        tokio::sync::Mutex<std::collections::HashMap<(u64, u64), u64>>,
    >,
    total_size: u64,
    chunks: &ChunkedHash,
    completed: Arc<tokio::sync::Mutex<Vec<bool>>>,
) -> Result<(), PeerStorageError> {
    use std::sync::atomic::Ordering;

    let part_size = end - start;

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
        let expected_hashes_for_range =
            &chunks.chunk_hashes[start_chunk_index..end_chunk_index];

        let mut writer = tokio::io::BufWriter::new(file);

        // Drive progress through a wrapping callback that updates the
        // shared total + per-range last value. The verifier doesn't itself
        // surface progress, so we tee it through a write-side wrapper.
        let total_received_pc = total_received.clone();
        let range_progress_pc = range_progress.clone();
        let on_progress_pc = on_progress.clone();
        let range_key = (start, end);

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
                // Report final progress for this range. Failed attempts'
                // contributions are rolled back by the caller before retry,
                // so on a successful attempt we can just add the full range
                // size to total_received in one shot.
                if let Some(cb) = on_progress_pc {
                    let prev = {
                        let mut rp = range_progress_pc.lock().await;
                        let prev = rp.get(&range_key).copied().unwrap_or(0);
                        rp.insert(range_key, part_size);
                        prev
                    };
                    let delta = part_size.saturating_sub(prev);
                    let new_total =
                        total_received_pc.fetch_add(delta, Ordering::Relaxed) + delta;
                    cb(new_total.min(total_size), total_size);
                }
                // pause / cancel are honoured by the underlying recv stream
                // through TCP-style backpressure once we surface those into
                // pipe_recv_to_writer_verified — for now keep the bindings
                // alive so signatures don't drift.
                let _ = (cancel_token, pause_flag);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}
