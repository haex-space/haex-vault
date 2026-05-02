//! Client-side remote operations — outgoing requests to peer endpoints.

use std::sync::Arc;

use iroh::{EndpointId, RelayUrl};

use crate::peer_storage::endpoint::PeerEndpoint;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{FileEntry, Request, Response};

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
    /// Returns the total file size on success.
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
    ) -> Result<u64, PeerStorageError> {
        let (mut send, mut recv) = self.open_stream(remote_id, relay_url).await?;

        let req = Request::Read {
            path: path.to_string(),
            range,
            ucan_token: ucan_token.to_string(),
        };
        let response = Self::send_request(&mut send, &mut recv, &req).await?;

        match response {
            Response::ReadHeader { size } => {
                // Stream chunks directly to file — no full-file RAM buffering
                use tokio::io::AsyncWriteExt;

                let mut file = tokio::fs::File::create(output_path)
                    .await
                    .map_err(|e| PeerStorageError::ProtocolError {
                        reason: format!("Failed to create output file: {e}"),
                    })?;

                let mut bytes_written: u64 = 0;
                let mut buf = [0u8; 64 * 1024]; // 64KB chunks

                loop {
                    // Check cancellation before each chunk
                    if let Some(ref token) = cancel_token {
                        if token.is_cancelled() {
                            let _ = tokio::fs::remove_file(output_path).await;
                            return Err(PeerStorageError::ProtocolError {
                                reason: "Transfer cancelled".to_string(),
                            });
                        }
                    }

                    // Wait while paused
                    if let Some(ref flag) = pause_flag {
                        while flag.load(std::sync::atomic::Ordering::Relaxed) {
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            if let Some(ref token) = cancel_token {
                                if token.is_cancelled() {
                                    let _ = tokio::fs::remove_file(output_path).await;
                                    return Err(PeerStorageError::ProtocolError {
                                        reason: "Transfer cancelled".to_string(),
                                    });
                                }
                            }
                        }
                    }

                    let chunk = recv.read(&mut buf).await.map_err(|e| {
                        PeerStorageError::ConnectionFailed {
                            reason: format!("Failed to read from stream: {e}"),
                        }
                    })?;
                    match chunk {
                        Some(n) => {
                            file.write_all(&buf[..n]).await.map_err(|e| {
                                PeerStorageError::ProtocolError {
                                    reason: format!("Failed to write to file: {e}"),
                                }
                            })?;
                            bytes_written += n as u64;
                            if let Some(ref cb) = on_progress {
                                cb(bytes_written, size);
                            }
                        }
                        None => break,
                    }
                }

                file.flush().await.map_err(|e| PeerStorageError::ProtocolError {
                    reason: format!("Failed to flush file: {e}"),
                })?;

                if bytes_written != size {
                    let _ = tokio::fs::remove_file(output_path).await;
                    return Err(PeerStorageError::ProtocolError {
                        reason: format!(
                            "Truncated transfer: expected {size} bytes, received {bytes_written}"
                        ),
                    });
                }

                Ok(size)
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

                loop {
                    let chunk = recv.read(&mut buf).await.map_err(|e| {
                        PeerStorageError::ConnectionFailed {
                            reason: format!("Failed to read from stream: {e}"),
                        }
                    })?;
                    match chunk {
                        Some(n) => data.extend_from_slice(&buf[..n]),
                        None => break,
                    }
                }

                if data.len() as u64 != size {
                    return Err(PeerStorageError::ProtocolError {
                        reason: format!(
                            "Truncated transfer: expected {size} bytes, received {}",
                            data.len()
                        ),
                    });
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
}
