//! PeerProvider — SyncProvider implementation for P2P sync via iroh/QUIC
//!
//! Supports both read and write operations. Each request carries a UCAN token
//! for authorization — the remote endpoint validates capabilities per-request.

use std::sync::Arc;

use async_trait::async_trait;

use iroh::{EndpointId, RelayUrl};

use crate::peer_storage::endpoint::PeerEndpoint;

use super::provider::{SyncProvider, SyncProviderError};
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
}

#[async_trait]
impl SyncProvider for PeerProvider {
    fn display_name(&self) -> String {
        format!("peer:{}", self.remote_id)
    }

    async fn manifest(&self) -> Result<Vec<FileState>, SyncProviderError> {
        let endpoint = self.endpoint.read().await;
        endpoint
            .remote_manifest(
                self.remote_id,
                self.relay_url.clone(),
                &self.remote_base_path,
                &self.ucan_token,
            )
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed {
                reason: e.to_string(),
            })
    }

    async fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, SyncProviderError> {
        let full_path = self.full_remote_path(relative_path);
        let endpoint = self.endpoint.read().await;
        endpoint
            .remote_read_bytes(
                self.remote_id,
                self.relay_url.clone(),
                &full_path,
                &self.ucan_token,
            )
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed {
                reason: e.to_string(),
            })
    }

    async fn write_file(&self, relative_path: &str, data: &[u8]) -> Result<(), SyncProviderError> {
        let full_path = self.full_remote_path(relative_path);
        let endpoint = self.endpoint.read().await;
        endpoint
            .remote_write_file(
                self.remote_id,
                self.relay_url.clone(),
                &full_path,
                data,
                &self.ucan_token,
            )
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed {
                reason: e.to_string(),
            })
    }

    async fn delete_file(
        &self,
        relative_path: &str,
        to_trash: bool,
    ) -> Result<(), SyncProviderError> {
        let full_path = self.full_remote_path(relative_path);
        let endpoint = self.endpoint.read().await;
        endpoint
            .remote_delete_file(
                self.remote_id,
                self.relay_url.clone(),
                &full_path,
                to_trash,
                &self.ucan_token,
            )
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed {
                reason: e.to_string(),
            })
    }

    async fn create_directory(&self, relative_path: &str) -> Result<(), SyncProviderError> {
        let full_path = self.full_remote_path(relative_path);
        let endpoint = self.endpoint.read().await;
        endpoint
            .remote_create_directory(
                self.remote_id,
                self.relay_url.clone(),
                &full_path,
                &self.ucan_token,
            )
            .await
            .map_err(|e| SyncProviderError::ConnectionFailed {
                reason: e.to_string(),
            })
    }

    async fn read_file_to_path(
        &self,
        relative_path: &str,
        output_path: &std::path::Path,
        on_progress: Arc<dyn Fn(u64, u64) + Send + Sync>,
    ) -> Result<u64, SyncProviderError> {
        let full_path = self.full_remote_path(relative_path);
        let endpoint = self.endpoint.read().await;
        endpoint
            .remote_read_to_file(
                self.remote_id,
                self.relay_url.clone(),
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

    fn supports_trash(&self) -> bool {
        false
    }
}
