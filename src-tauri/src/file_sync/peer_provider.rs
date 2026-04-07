//! PeerProvider — SyncProvider implementation for reading from a remote P2P peer
//!
//! Read-only provider that fetches files from a remote peer via iroh/QUIC.
//! Write, delete, and create_directory operations return AccessDenied.

use std::sync::Arc;

use async_trait::async_trait;
use iroh::{EndpointId, RelayUrl};

use crate::peer_storage::endpoint::PeerEndpoint;

use super::provider::{SyncProvider, SyncProviderError};
use super::types::FileState;

pub struct PeerProvider {
    /// The shared PeerEndpoint (behind AppState mutex)
    endpoint: Arc<tokio::sync::Mutex<PeerEndpoint>>,
    /// Remote peer's endpoint ID
    remote_id: EndpointId,
    /// Relay URL for NAT traversal
    relay_url: Option<RelayUrl>,
    /// Base path on the remote share (e.g. "MyShare" or "MyShare/subfolder")
    remote_base_path: String,
}

impl PeerProvider {
    pub fn new(
        endpoint: Arc<tokio::sync::Mutex<PeerEndpoint>>,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        remote_base_path: String,
    ) -> Self {
        Self {
            endpoint,
            remote_id,
            relay_url,
            remote_base_path,
        }
    }

    /// Build the full remote path by joining the base path with a relative path.
    fn full_remote_path(&self, relative_path: &str) -> String {
        if relative_path.is_empty() {
            self.remote_base_path.clone()
        } else {
            format!("{}/{}", self.remote_base_path, relative_path)
        }
    }
}

#[async_trait]
impl SyncProvider for PeerProvider {
    fn display_name(&self) -> String {
        format!("peer:{}", self.remote_id)
    }

    async fn manifest(&self) -> Result<Vec<FileState>, SyncProviderError> {
        let endpoint = self.endpoint.lock().await;
        endpoint
            .remote_manifest(
                self.remote_id,
                self.relay_url.clone(),
                &self.remote_base_path,
            )
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed {
                reason: e.to_string(),
            })
    }

    async fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, SyncProviderError> {
        let full_path = self.full_remote_path(relative_path);
        let endpoint = self.endpoint.lock().await;
        endpoint
            .remote_read_bytes(
                self.remote_id,
                self.relay_url.clone(),
                &full_path,
            )
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed {
                reason: e.to_string(),
            })
    }

    async fn write_file(&self, _relative_path: &str, _data: &[u8]) -> Result<(), SyncProviderError> {
        Err(SyncProviderError::AccessDenied {
            reason: "PeerProvider is read-only".to_string(),
        })
    }

    async fn delete_file(
        &self,
        _relative_path: &str,
        _to_trash: bool,
    ) -> Result<(), SyncProviderError> {
        Err(SyncProviderError::AccessDenied {
            reason: "PeerProvider is read-only".to_string(),
        })
    }

    async fn create_directory(&self, _relative_path: &str) -> Result<(), SyncProviderError> {
        Err(SyncProviderError::AccessDenied {
            reason: "PeerProvider is read-only".to_string(),
        })
    }

    fn supports_trash(&self) -> bool {
        false
    }
}
