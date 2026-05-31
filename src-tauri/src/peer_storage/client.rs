//! Client-side remote operations — outgoing requests to peer endpoints.

use std::path::Path;
use std::sync::Arc;

use iroh::{EndpointId, RelayUrl};

use crate::peer_storage::endpoint::PeerEndpoint;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{FileEntry, Request, Response};
use crate::peer_storage::streaming;

/// Outcome of a streaming peer read into a local file.
///
/// `hash` is the SHA-256 of the bytes that arrived over the wire (and were
/// written to disk). It is `None` when only a partial range was requested,
/// because a partial-content hash is not comparable to a full-file manifest
/// hash.
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

    /// Connect to a remote peer and download a file directly to disk.
    /// Streams chunks from the iroh connection directly into the output file
    /// without buffering the entire file in memory.
    /// Returns the total file size and the SHA-256 of the bytes that landed
    /// on disk so callers can verify integrity against the sender's manifest.
    pub async fn remote_read_to_file(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        output_path: &std::path::Path,
        range: Option<[u64; 2]>,
        on_progress: Option<Box<dyn Fn(u64, u64) + Send>>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
        pause_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
        ucan_token: &str,
    ) -> Result<StreamReadResult, PeerStorageError> {
        let (mut send, mut recv) = self.open_stream(remote_id, relay_url).await?;
        Self::read_open_streams_to_file(
            &mut send, &mut recv, path, output_path,
            range, on_progress, cancel_token, pause_flag, ucan_token,
        ).await
    }

    /// Transfer a file from already-opened QUIC streams to disk.
    /// Callers that hold a lock on `PeerEndpoint` should open the stream under
    /// the lock, drop it, then call this function so the lock is not held during I/O.
    ///
    /// Computes SHA-256 of the streamed bytes inline with the receive loop so
    /// the caller can verify integrity against the sender's manifest hash
    /// without re-reading the file from disk afterwards. When `range` is set
    /// the hash covers only the requested slice — full-file integrity checks
    /// must therefore use full-file reads (range=None).
    pub(crate) async fn read_open_streams_to_file(
        send: &mut iroh::endpoint::SendStream,
        recv: &mut iroh::endpoint::RecvStream,
        path: &str,
        output_path: &std::path::Path,
        range: Option<[u64; 2]>,
        on_progress: Option<Box<dyn Fn(u64, u64) + Send>>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
        pause_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
        ucan_token: &str,
    ) -> Result<StreamReadResult, PeerStorageError> {
        let is_full_file = range.is_none();
        let req = Request::Read {
            path: path.to_string(),
            range,
            ucan_token: ucan_token.to_string(),
        };
        let response = Self::send_request(send, recv, &req).await?;

        match response {
            Response::ReadHeader { size } => {
                let file = tokio::fs::File::create(output_path).await.map_err(|e| {
                    PeerStorageError::ProtocolError {
                        reason: format!("Failed to create output file: {e}"),
                    }
                })?;

                let options = streaming::RecvOptions {
                    on_progress,
                    cancel_token,
                    pause_flag,
                    compute_hash: is_full_file,
                };

                let result = streaming::pipe_recv_to_writer(recv, file, size, options).await;

                let stats = match result {
                    Ok(s) => s,
                    Err(streaming::PipelineError::Cancelled) => {
                        let _ = tokio::fs::remove_file(output_path).await;
                        return Err(PeerStorageError::ProtocolError {
                            reason: "Transfer cancelled".to_string(),
                        });
                    }
                    Err(streaming::PipelineError::Io(e)) => {
                        let _ = tokio::fs::remove_file(output_path).await;
                        return Err(PeerStorageError::ProtocolError {
                            reason: format!("Failed to write to file: {e}"),
                        });
                    }
                    Err(streaming::PipelineError::Stream(reason)) => {
                        let _ = tokio::fs::remove_file(output_path).await;
                        return Err(PeerStorageError::ConnectionFailed { reason });
                    }
                };

                if stats.bytes != size {
                    let _ = tokio::fs::remove_file(output_path).await;
                    return Err(PeerStorageError::ConnectionFailed {
                        reason: format!(
                            "Incomplete download: expected {size} bytes, received {}",
                            stats.bytes
                        ),
                    });
                }

                Ok(StreamReadResult {
                    bytes: size,
                    hash: stats.hash,
                })
            }
            Response::Error { message } => {
                Err(PeerStorageError::ProtocolError { reason: message })
            }
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
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
    /// For large files prefer `remote_read_to_file`; this is for sync-sized reads.
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

    /// Fetch metadata for a single remote path (size, is_dir, modified).
    pub async fn remote_stat(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        ucan_token: &str,
    ) -> Result<FileEntry, PeerStorageError> {
        let (mut send, mut recv) = self.open_stream(remote_id, relay_url).await?;
        let req = Request::Stat {
            path: path.to_string(),
            ucan_token: ucan_token.to_string(),
        };
        let response = Self::send_request(&mut send, &mut recv, &req).await?;
        match response {
            Response::Stat { entry } => Ok(entry),
            Response::Error { message } => {
                Err(PeerStorageError::ProtocolError { reason: message })
            }
            _ => Err(PeerStorageError::ProtocolError {
                reason: "unexpected response (stat)".to_string(),
            }),
        }
    }
}

/// Download a file as `parallelism` parallel range reads, each on its own
/// QUIC stream. Faster than [`PeerEndpoint::read_open_streams_to_file`] for
/// large files because per-stream throughput stops being the bottleneck —
/// the cost is one stat round-trip (paid by the caller) plus a final
/// sequential pass over the on-disk file to compute the SHA-256, since
/// SHA-256 over `N` parallel byte ranges is not composable.
///
/// `size` must be the authoritative file size from the sender (e.g. from
/// the manifest or a stat call) — the function pre-allocates the output
/// file to that exact length and writes each range at its own offset.
pub(crate) async fn read_multipart_to_file(
    endpoint: Arc<tokio::sync::RwLock<PeerEndpoint>>,
    remote_id: EndpointId,
    relay_url: Option<RelayUrl>,
    path: String,
    output_path: std::path::PathBuf,
    size: u64,
    parallelism: usize,
    on_progress: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    pause_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    ucan_token: String,
) -> Result<StreamReadResult, PeerStorageError> {
    use std::sync::atomic::{AtomicU64, Ordering};

    if size == 0 {
        tokio::fs::File::create(&output_path)
            .await
            .map_err(PeerStorageError::Io)?;
        use sha2::Digest;
        return Ok(StreamReadResult {
            bytes: 0,
            hash: Some(hex::encode(sha2::Sha256::digest([]))),
        });
    }

    let n = parallelism
        .max(1)
        .min(streaming::MAX_PARALLEL_STREAMS_PER_FILE);

    // Pre-allocate the target file so every spawned task can seek to its own
    // offset and write its range independently. truncate(true) discards any
    // stale partial file from a previous failed download.
    {
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&output_path)
            .await
            .map_err(PeerStorageError::Io)?;
        file.set_len(size).await.map_err(PeerStorageError::Io)?;
        file.flush().await.map_err(PeerStorageError::Io)?;
    }

    let total_received = Arc::new(AtomicU64::new(0));
    let chunk = size.div_ceil(n as u64);

    let mut join_set = tokio::task::JoinSet::new();
    for i in 0..n {
        let start = (i as u64) * chunk;
        if start >= size {
            break;
        }
        let end = (start + chunk).min(size);
        let part_size = end - start;

        let endpoint = endpoint.clone();
        let path = path.clone();
        let output_path = output_path.clone();
        let ucan_token = ucan_token.clone();
        let relay_url = relay_url.clone();
        let cancel_token = cancel_token.clone();
        let pause_flag = pause_flag.clone();
        let on_progress = on_progress.clone();
        let total_received = total_received.clone();
        let prev_in_stream = Arc::new(AtomicU64::new(0));
        let total_size = size;

        join_set.spawn(async move {
            let (mut send, mut recv) = endpoint
                .read()
                .await
                .open_stream(remote_id, relay_url)
                .await?;

            let req = Request::Read {
                path,
                range: Some([start, end]),
                ucan_token,
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
                .open(&output_path)
                .await
                .map_err(PeerStorageError::Io)?;
            use tokio::io::AsyncSeekExt;
            file.seek(std::io::SeekFrom::Start(start))
                .await
                .map_err(PeerStorageError::Io)?;

            let progress_cb: Option<Box<dyn Fn(u64, u64) + Send>> = on_progress.map(|cb| {
                let total_received = total_received.clone();
                let prev_in_stream = prev_in_stream.clone();
                Box::new(move |done_in_stream: u64, _expected_in_stream: u64| {
                    let prev = prev_in_stream.swap(done_in_stream, Ordering::Relaxed);
                    let delta = done_in_stream.saturating_sub(prev);
                    let new_total =
                        total_received.fetch_add(delta, Ordering::Relaxed) + delta;
                    cb(new_total, total_size);
                }) as Box<dyn Fn(u64, u64) + Send>
            });

            let options = streaming::RecvOptions {
                on_progress: progress_cb,
                cancel_token,
                pause_flag,
                compute_hash: false,
            };

            let stats = streaming::pipe_recv_to_writer(&mut recv, file, part_size, options)
                .await
                .map_err(|e| match e {
                    streaming::PipelineError::Io(e) => PeerStorageError::Io(e),
                    streaming::PipelineError::Stream(reason) => {
                        PeerStorageError::ConnectionFailed { reason }
                    }
                    streaming::PipelineError::Cancelled => PeerStorageError::ProtocolError {
                        reason: "Transfer cancelled".to_string(),
                    },
                })?;
            if stats.bytes != part_size {
                return Err(PeerStorageError::ConnectionFailed {
                    reason: format!(
                        "multipart range short: expected {part_size}, received {}",
                        stats.bytes
                    ),
                });
            }
            Ok::<(), PeerStorageError>(())
        });
    }

    let mut first_err: Option<PeerStorageError> = None;
    while let Some(res) = join_set.join_next().await {
        let task_res = res.map_err(|e| PeerStorageError::ProtocolError {
            reason: format!("multipart join: {e}"),
        });
        match task_res.and_then(|inner| inner) {
            Ok(()) => {}
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e);
                    join_set.abort_all();
                }
            }
        }
    }

    if let Some(err) = first_err {
        let _ = tokio::fs::remove_file(&output_path).await;
        return Err(err);
    }

    let hash = hash_file_sha256(&output_path).await.map_err(|e| {
        PeerStorageError::ProtocolError {
            reason: format!("post-download hash: {e}"),
        }
    })?;

    Ok(StreamReadResult {
        bytes: size,
        hash: Some(hash),
    })
}

/// Sequentially read `path` and return the hex-encoded SHA-256 of its
/// contents. Used after multi-stream downloads (whose per-range layout
/// makes inline hashing impossible) to recover the manifest-comparable
/// hash that single-stream downloads produce for free.
async fn hash_file_sha256(path: &std::path::Path) -> Result<String, std::io::Error> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;

    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; streaming::CHUNK_SIZE];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}
