//! PeerProvider — SyncProvider implementation for P2P sync via iroh/QUIC
//!
//! Supports both read and write operations. Each request carries a UCAN token
//! for authorization — the remote endpoint validates capabilities per-request.

use std::sync::Arc;

use async_trait::async_trait;
use iroh::{EndpointId, RelayUrl};

use crate::peer_storage::endpoint::PeerEndpoint;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{self, Request, Response};

use super::provider::{validate_relative_path, SyncProvider, SyncProviderError};
use super::types::FileState;



pub struct PeerProvider {
    endpoint: Arc<tokio::sync::RwLock<PeerEndpoint>>,
    remote_id: EndpointId,
    relay_url: Option<RelayUrl>,
    remote_base_path: String,
    ucan_token: String,
}

impl PeerProvider {
    pub fn new(
        endpoint: Arc<tokio::sync::RwLock<PeerEndpoint>>,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        remote_base_path: String,
        ucan_token: String,
    ) -> Self {
        Self {
            endpoint,
            remote_id,
            relay_url,
            remote_base_path,
            ucan_token,
        }
    }

    fn full_remote_path(&self, relative_path: &str) -> String {
        if relative_path.is_empty() {
            self.remote_base_path.clone()
        } else {
            format!("{}/{}", self.remote_base_path, relative_path)
        }
    }

    /// Open a QUIC stream while briefly holding the read lock, then drop the lock.
    /// All subsequent I/O happens without holding `peer_storage` read lock,
    /// so vault re-initialization (which needs the write lock) is never blocked.
    async fn open_stream(
        &self,
    ) -> Result<(iroh::endpoint::SendStream, iroh::endpoint::RecvStream), PeerStorageError> {
        self.endpoint
            .read()
            .await
            .open_stream(self.remote_id, self.relay_url.clone())
            .await
    }
}

#[async_trait]
impl SyncProvider for PeerProvider {
    fn display_name(&self) -> String {
        format!("peer:{}", self.remote_id)
    }

    async fn manifest(&self) -> Result<Vec<FileState>, SyncProviderError> {
        let (mut send, mut recv) = self.open_stream().await.map_err(|e| {
            SyncProviderError::ConnectionFailed { reason: e.to_string() }
        })?;
        let req = Request::Manifest {
            path: self.remote_base_path.clone(),
            ucan_token: self.ucan_token.clone(),
        };
        let response = PeerEndpoint::send_request(&mut send, &mut recv, &req)
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })?;
        match response {
            Response::Manifest { entries } => Ok(entries),
            Response::Error { message } => Err(SyncProviderError::ConnectionFailed { reason: message }),
            _ => Err(SyncProviderError::ConnectionFailed { reason: "Unexpected response".to_string() }),
        }
    }

    async fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, SyncProviderError> {
        let full_path = self.full_remote_path(relative_path);
        let (mut send, mut recv) = self.open_stream().await.map_err(|e| {
            SyncProviderError::ConnectionFailed { reason: e.to_string() }
        })?;
        let req = Request::Read {
            path: full_path,
            range: None,
            ucan_token: self.ucan_token.clone(),
        };
        let response = PeerEndpoint::send_request(&mut send, &mut recv, &req)
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })?;
        match response {
            Response::ReadHeader { size } => {
                let mut data = Vec::with_capacity(size as usize);
                let mut buf = [0u8; 64 * 1024];
                loop {
                    let chunk = recv.read(&mut buf).await.map_err(|e| {
                        SyncProviderError::ConnectionFailed { reason: e.to_string() }
                    })?;
                    match chunk {
                        Some(n) => data.extend_from_slice(&buf[..n]),
                        None => break,
                    }
                }
                if data.len() != size as usize {
                    return Err(SyncProviderError::ConnectionFailed {
                        reason: format!("short read: expected {} bytes, got {}", size, data.len()),
                    });
                }
                Ok(data)
            }
            Response::Error { message } => Err(SyncProviderError::ConnectionFailed { reason: message }),
            _ => Err(SyncProviderError::ConnectionFailed { reason: "Unexpected response".to_string() }),
        }
    }

    async fn read_file_with_progress(
        &self,
        relative_path: &str,
        on_progress: Arc<dyn Fn(u64, u64) + Send + Sync>,
    ) -> Result<Vec<u8>, SyncProviderError> {
        let full_path = self.full_remote_path(relative_path);
        let (mut send, mut recv) = self.open_stream().await.map_err(|e| {
            SyncProviderError::ConnectionFailed { reason: e.to_string() }
        })?;
        let req = Request::Read {
            path: full_path,
            range: None,
            ucan_token: self.ucan_token.clone(),
        };
        let response = PeerEndpoint::send_request(&mut send, &mut recv, &req)
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })?;
        match response {
            Response::ReadHeader { size } => {
                let mut data = Vec::with_capacity(size as usize);
                let mut buf = [0u8; 64 * 1024];
                let mut bytes_received: u64 = 0;
                loop {
                    let chunk = recv.read(&mut buf).await.map_err(|e| {
                        SyncProviderError::ConnectionFailed { reason: e.to_string() }
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
                if data.len() != size as usize {
                    return Err(SyncProviderError::ConnectionFailed {
                        reason: format!("short read: expected {} bytes, got {}", size, data.len()),
                    });
                }
                Ok(data)
            }
            Response::Error { message } => Err(SyncProviderError::ConnectionFailed { reason: message }),
            _ => Err(SyncProviderError::ConnectionFailed { reason: "Unexpected response".to_string() }),
        }
    }

    async fn write_file(&self, relative_path: &str, data: &[u8]) -> Result<(), SyncProviderError> {
        let full_path = self.full_remote_path(relative_path);
        let (mut send, mut recv) = self.open_stream().await.map_err(|e| {
            SyncProviderError::ConnectionFailed { reason: e.to_string() }
        })?;
        let req = Request::Write {
            path: full_path,
            size: data.len() as u64,
            ucan_token: self.ucan_token.clone(),
        };
        PeerEndpoint::send_request_header(&mut send, &req)
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })?;

        send.write_all(data).await.map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })?;
        send.finish().map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })?;

        let response: Response = protocol::read_response(&mut recv)
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })?;
        match response {
            Response::WriteOk => Ok(()),
            Response::Error { message } => Err(SyncProviderError::ConnectionFailed { reason: message }),
            _ => Err(SyncProviderError::ConnectionFailed { reason: "Unexpected response".to_string() }),
        }
    }

    async fn delete_file(
        &self,
        relative_path: &str,
        to_trash: bool,
    ) -> Result<(), SyncProviderError> {
        let full_path = self.full_remote_path(relative_path);
        let (mut send, mut recv) = self.open_stream().await.map_err(|e| {
            SyncProviderError::ConnectionFailed { reason: e.to_string() }
        })?;
        let req = Request::Delete {
            path: full_path,
            to_trash,
            ucan_token: self.ucan_token.clone(),
        };
        let response = PeerEndpoint::send_request(&mut send, &mut recv, &req)
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })?;
        match response {
            Response::DeleteOk => Ok(()),
            Response::Error { message } => Err(SyncProviderError::ConnectionFailed { reason: message }),
            _ => Err(SyncProviderError::ConnectionFailed { reason: "Unexpected response".to_string() }),
        }
    }

    async fn create_directory(&self, relative_path: &str) -> Result<(), SyncProviderError> {
        let full_path = self.full_remote_path(relative_path);
        let (mut send, mut recv) = self.open_stream().await.map_err(|e| {
            SyncProviderError::ConnectionFailed { reason: e.to_string() }
        })?;
        let req = Request::CreateDirectory {
            path: full_path,
            ucan_token: self.ucan_token.clone(),
        };
        let response = PeerEndpoint::send_request(&mut send, &mut recv, &req)
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })?;
        match response {
            Response::CreateDirectoryOk => Ok(()),
            Response::Error { message } => Err(SyncProviderError::ConnectionFailed { reason: message }),
            _ => Err(SyncProviderError::ConnectionFailed { reason: "Unexpected response".to_string() }),
        }
    }

    async fn read_file_to_path(
        &self,
        relative_path: &str,
        output_path: &std::path::Path,
        on_progress: Arc<dyn Fn(u64, u64) + Send + Sync>,
    ) -> Result<u64, SyncProviderError> {
        validate_relative_path(relative_path)?;
        let full_path = self.full_remote_path(relative_path);
        let (mut send, mut recv) = self.open_stream().await.map_err(|e| {
            SyncProviderError::ConnectionFailed { reason: e.to_string() }
        })?;
        crate::peer_storage::endpoint::PeerEndpoint::read_open_streams_to_file(
            &mut send,
            &mut recv,
            &full_path,
            output_path,
            None,
            Some(Box::new(move |done, total| on_progress(done, total))),
            None,
            None,
            &self.ucan_token,
        )
        .await
        .map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })
    }

    async fn write_file_from_path(
        &self,
        relative_path: &str,
        source_path: &std::path::Path,
    ) -> Result<(), SyncProviderError> {
        validate_relative_path(relative_path)?;
        let full_path = self.full_remote_path(relative_path);
        let size = tokio::fs::metadata(source_path)
            .await
            .map_err(SyncProviderError::Io)?
            .len();
        let (mut send, mut recv) = self.open_stream().await.map_err(|e| {
            SyncProviderError::ConnectionFailed { reason: e.to_string() }
        })?;
        let req = Request::Write {
            path: full_path,
            size,
            ucan_token: self.ucan_token.clone(),
        };
        PeerEndpoint::send_request_header(&mut send, &req)
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })?;
        // Explicit 256 KB chunks: tokio::io::copy uses an 8 KB internal
        // buffer, which produces ~4× more QUIC writes/syscalls than necessary
        // and was the upload-side throughput cap on LAN.
        use tokio::io::AsyncReadExt;
        let mut file = tokio::fs::File::open(source_path).await.map_err(SyncProviderError::Io)?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = file.read(&mut buf).await.map_err(SyncProviderError::Io)?;
            if n == 0 {
                break;
            }
            send.write_all(&buf[..n])
                .await
                .map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })?;
        }
        send.finish()
            .map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })?;
        let response: Response = protocol::read_response(&mut recv)
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed { reason: e.to_string() })?;
        match response {
            Response::WriteOk => Ok(()),
            Response::Error { message } => Err(SyncProviderError::ConnectionFailed { reason: message }),
            _ => Err(SyncProviderError::ConnectionFailed {
                reason: "Unexpected response".to_string(),
            }),
        }
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_trash(&self) -> bool {
        false
    }
}
