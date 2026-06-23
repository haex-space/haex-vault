use iroh::{EndpointId, RelayUrl};

use crate::peer_storage::client::RemoteStat;
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
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
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
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
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
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
            _ => Err(PeerStorageError::ProtocolError {
                reason: "unexpected response (stat)".to_string(),
            }),
        }
    }
}
