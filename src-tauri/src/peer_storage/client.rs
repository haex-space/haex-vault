//! Client-side remote operations — outgoing requests to peer endpoints.

use std::sync::Arc;

use iroh::{EndpointId, RelayUrl};

use crate::peer_storage::endpoint::PeerEndpoint;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{FileEntry, Request, Response};

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
        use sha2::{Digest, Sha256};
        use tokio::io::AsyncWriteExt;

        let is_full_file = range.is_none();
        let req = Request::Read {
            path: path.to_string(),
            range,
            ucan_token: ucan_token.to_string(),
        };
        let response = Self::send_request(send, recv, &req).await?;

        match response {
            Response::ReadHeader { size } => {
                // Pipeline: QUIC reader (this task) → bounded channel →
                // disk-writer + hasher (spawned task). The previous serial
                // loop alternated `recv.read().await` and `file.write_all().await`,
                // which paired disk and net latency in series and choked
                // per-stream throughput on a fast LAN. Decoupling them lets
                // QUIC keep pulling ACKs while the disk is still flushing
                // the previous chunk.
                const CHUNK: usize = 1024 * 1024;
                const CHANNEL_DEPTH: usize = 8;
                let (tx, mut rx) =
                    tokio::sync::mpsc::channel::<Vec<u8>>(CHANNEL_DEPTH);

                let writer_path = output_path.to_path_buf();
                let writer_task = tokio::spawn(async move {
                    let mut file = match tokio::fs::File::create(&writer_path).await {
                        Ok(f) => f,
                        Err(e) => {
                            return Err(PeerStorageError::ProtocolError {
                                reason: format!("Failed to create output file: {e}"),
                            });
                        }
                    };
                    let mut hasher = is_full_file.then(Sha256::new);
                    let mut bytes_written: u64 = 0;
                    while let Some(chunk) = rx.recv().await {
                        if let Err(e) = file.write_all(&chunk).await {
                            return Err(PeerStorageError::ProtocolError {
                                reason: format!("Failed to write to file: {e}"),
                            });
                        }
                        if let Some(h) = hasher.as_mut() {
                            h.update(&chunk);
                        }
                        bytes_written += chunk.len() as u64;
                    }
                    if let Err(e) = file.flush().await {
                        return Err(PeerStorageError::ProtocolError {
                            reason: format!("Failed to flush file: {e}"),
                        });
                    }
                    Ok((bytes_written, hasher.map(|h| hex::encode(h.finalize()))))
                });

                let mut bytes_received: u64 = 0;
                let mut buf = vec![0u8; CHUNK];
                let mut io_err: Option<PeerStorageError> = None;

                while bytes_received < size {
                    // Check cancellation before each chunk
                    if let Some(ref token) = cancel_token {
                        if token.is_cancelled() {
                            io_err = Some(PeerStorageError::ProtocolError {
                                reason: "Transfer cancelled".to_string(),
                            });
                            break;
                        }
                    }

                    // Wait while paused
                    if let Some(ref flag) = pause_flag {
                        while flag.load(std::sync::atomic::Ordering::Relaxed) {
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            if let Some(ref token) = cancel_token {
                                if token.is_cancelled() {
                                    break;
                                }
                            }
                        }
                        if let Some(ref token) = cancel_token {
                            if token.is_cancelled() {
                                io_err = Some(PeerStorageError::ProtocolError {
                                    reason: "Transfer cancelled".to_string(),
                                });
                                break;
                            }
                        }
                    }

                    match recv.read(&mut buf).await {
                        Ok(Some(n)) => {
                            let chunk = buf[..n].to_vec();
                            if tx.send(chunk).await.is_err() {
                                // Writer task aborted — surface its error below.
                                break;
                            }
                            bytes_received += n as u64;
                            if let Some(ref cb) = on_progress {
                                cb(bytes_received, size);
                            }
                        }
                        Ok(None) => {
                            io_err = Some(PeerStorageError::ConnectionFailed {
                                reason: format!(
                                    "Stream ended early: expected {size} bytes, received {bytes_received}"
                                ),
                            });
                            break;
                        }
                        Err(e) => {
                            io_err = Some(PeerStorageError::ConnectionFailed {
                                reason: format!("Failed to read from stream: {e}"),
                            });
                            break;
                        }
                    }
                }

                drop(tx);
                let writer_result = writer_task.await.map_err(|e| {
                    PeerStorageError::ProtocolError {
                        reason: format!("Writer task panicked: {e}"),
                    }
                })?;

                if let Some(err) = io_err {
                    let _ = tokio::fs::remove_file(output_path).await;
                    return Err(err);
                }

                let (bytes_written, hash) = writer_result?;

                if bytes_written != size {
                    let _ = tokio::fs::remove_file(output_path).await;
                    return Err(PeerStorageError::ConnectionFailed {
                        reason: format!(
                            "Incomplete download: expected {size} bytes, received {bytes_written}"
                        ),
                    });
                }

                Ok(StreamReadResult { bytes: size, hash })
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

    /// Connect to a remote peer and write a file.
    /// Sends the Write request header, then streams the file data.
    pub async fn remote_write_file(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        data: &[u8],
        ucan_token: &str,
    ) -> Result<(), PeerStorageError> {
        let (mut send, mut recv) = self.open_stream(remote_id, relay_url).await?;

        let req = Request::Write {
            path: path.to_string(),
            size: data.len() as u64,
            ucan_token: ucan_token.to_string(),
        };
        Self::send_request_header(&mut send, &req).await?;

        // Stream file data
        send.write_all(data)
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
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
            Response::WriteOk => Ok(()),
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

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};

    use crate::peer_storage::endpoint::PeerEndpoint;

    // ------------------------------------------------------------------
    // Test harness — two local PeerEndpoints over RelayMode::Disabled,
    // sharing a temp directory that contains a 1 MiB ramp file
    // (byte i == (i % 256) as u8).
    // ------------------------------------------------------------------

    const BASE64URL: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
        &base64::alphabet::URL_SAFE,
        base64::engine::general_purpose::NO_PAD,
    );
    const ED25519_MULTICODEC: [u8; 2] = [0xed, 0x01];

    fn did_from_signing_key(key: &SigningKey) -> String {
        let mut bytes = Vec::with_capacity(34);
        bytes.extend_from_slice(&ED25519_MULTICODEC);
        bytes.extend_from_slice(key.verifying_key().as_bytes());
        format!("did:key:z{}", bs58::encode(bytes).into_string())
    }

    /// Mint a read-capable UCAN for `space_id`, signed by the audience key.
    /// Mirrors the test helper used by `ucan::verify::tests::make_test_token`,
    /// kept inline here so the peer_storage tests have no cross-module test
    /// dependency.
    fn read_ucan(signer: &SigningKey, space_id: &str) -> String {
        let issuer_did = did_from_signing_key(signer);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
        let payload = serde_json::json!({
            "ucv": "1.0",
            "iss": issuer_did,
            "aud": "did:key:z6MkAudience",
            "cap": { format!("space:{}", space_id): "space/read" },
            "exp": now + 3600,
            "iat": now,
            "prf": [],
            "nnc": "test-nonce"
        });
        let header_b64 = BASE64URL.encode(serde_json::to_string(&header).unwrap().as_bytes());
        let payload_b64 = BASE64URL.encode(serde_json::to_string(&payload).unwrap().as_bytes());
        let signing_input = format!("{}.{}", header_b64, payload_b64);
        let signature = signer.sign(signing_input.as_bytes());
        format!(
            "{}.{}.{}",
            header_b64,
            payload_b64,
            BASE64URL.encode(signature.to_bytes())
        )
    }

    struct Harness {
        // Kept alive so the bound iroh endpoint + accept loop keep running
        // for the duration of the test, even though we never call methods
        // on `server` directly after setup.
        _server: PeerEndpoint,
        client: PeerEndpoint,
        server_remote_id: iroh::EndpointId,
        share_name: String,
        space_id: String,
        ucan: String,
        _tmp: tempfile::TempDir,
    }

    /// Spin up two local PeerEndpoints. Server hosts a 1 MiB ramp file under
    /// share "media" / space "test-space". Client is registered as an allowed
    /// peer for that space and has a fresh QUIC connection cached so
    /// `open_stream` will reuse it without needing relay/address lookup.
    async fn setup_harness() -> Harness {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("ramp.bin");
        let mut ramp = vec![0u8; 1024 * 1024];
        for (i, b) in ramp.iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }
        tokio::fs::write(&file_path, &ramp).await.unwrap();

        let share_name = "media".to_string();
        let space_id = "test-space".to_string();

        // --- Server side ---
        let mut server = PeerEndpoint::new_ephemeral();
        let server_id = server.start_for_test().await.expect("server bind");
        server
            .add_share(
                "share-1".to_string(),
                share_name.clone(),
                tmp.path().to_string_lossy().to_string(),
                space_id.clone(),
            )
            .await;

        // --- Client side ---
        let mut client = PeerEndpoint::new_ephemeral();
        client.start_for_test().await.expect("client bind");
        let client_id = client.endpoint_id();

        // Grant the client read access to the space on the server.
        let mut allowed = HashMap::new();
        let mut spaces = HashSet::new();
        spaces.insert(space_id.clone());
        allowed.insert(client_id.to_string(), spaces);
        server.set_allowed_peers(allowed).await;

        // Server endpoint addr (full, with direct addrs since RelayMode::Disabled).
        let server_addr = server.endpoint_ref().unwrap().addr();
        client
            .connect_for_test(server_addr)
            .await
            .expect("client → server connect");

        // Sign the UCAN with the same key as the client device — the server's
        // capability check verifies the token signature but does not require
        // iss == client EndpointId, only that the token grants read on the
        // target space.
        let mut seed = [0u8; 32];
        rand::fill(&mut seed);
        let ucan_signer = SigningKey::from_bytes(&seed);
        let ucan = read_ucan(&ucan_signer, &space_id);

        Harness {
            _server: server,
            client,
            server_remote_id: server_id,
            share_name,
            space_id,
            ucan,
            _tmp: tmp,
        }
    }

    #[tokio::test]
    async fn remote_read_range_returns_only_requested_bytes() {
        let h = setup_harness().await;
        let path = format!("/{}/ramp.bin", h.share_name);

        let bytes = h
            .client
            .remote_read_range_bytes(h.server_remote_id, None, &path, [100, 199], &h.ucan)
            .await
            .expect("remote_read_range_bytes");

        assert_eq!(bytes.len(), 100, "range [100, 199] should yield 100 bytes");
        assert_eq!(bytes[0], 100, "first byte of the range");
        assert_eq!(bytes[99], 199, "last byte of the range");
        let _ = h.space_id;
    }

    #[tokio::test]
    async fn remote_stat_returns_file_size() {
        let h = setup_harness().await;
        let path = format!("/{}/ramp.bin", h.share_name);

        let entry = h
            .client
            .remote_stat(h.server_remote_id, None, &path, &h.ucan)
            .await
            .expect("remote_stat");

        assert_eq!(entry.size, 1024 * 1024, "ramp file is 1 MiB");
        assert!(!entry.is_dir, "ramp.bin is a regular file");
        let _ = h.space_id;
    }
}
