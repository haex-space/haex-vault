use std::sync::Arc;

use iroh::{EndpointId, RelayUrl};

use crate::file_sync::hashing::ChunkedHash;
use crate::peer_storage::client::StreamReadResult;
use crate::peer_storage::endpoint::PeerEndpoint;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{Request, Response};
use crate::peer_storage::streaming;

impl PeerEndpoint {
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
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
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
        // Sweep orphaned `.haex-partial.meta.tmp.<nonce>` files left by a prior
        // interrupted attempt before starting — nothing is mid-save here, so
        // every remaining temp file is garbage. Best-effort; the resume pair
        // (`.meta`/`.haex-partial`) is preserved.
        let _ = crate::peer_storage::resume::PartialState::sweep_tmp(output_path).await;

        let partial_path = crate::peer_storage::resume::PartialState::partial_path(output_path);

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
            Arc::new(tokio::sync::Mutex::new(vec![
                false;
                chunks.chunk_hashes.len()
            ]));
        let verifier = streaming::ChunkVerifier {
            expected_chunk_hashes: &chunks.chunk_hashes,
            chunk_size: chunks.chunk_size,
            start_chunk_index: 0,
            completed: completed.clone(),
        };

        // Per-chunk progress: accumulate each chunk's delta into the running
        // total and report (bytes_done, total) to the caller. cancel/pause are
        // honoured at chunk boundaries inside the verified pipe.
        let mut sent: u64 = 0;
        let on_chunk: Option<Box<dyn FnMut(u64) + Send>> = on_progress.map(|cb| {
            Box::new(move |delta: u64| {
                sent += delta;
                cb(sent.min(size), size);
            }) as Box<dyn FnMut(u64) + Send>
        });
        let controls = streaming::VerifiedRecvControls {
            on_chunk,
            cancel_token,
            pause_flag,
        };

        let result = streaming::pipe_recv_to_writer_verified(
            &mut *recv,
            &mut writer,
            size,
            verifier,
            Some((output_path, &chunks.file_hash)),
            controls,
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
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
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
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
            _ => Err(PeerStorageError::ProtocolError {
                reason: "unexpected response (read range)".to_string(),
            }),
        }
    }
}
