use std::path::Path;

use iroh::{EndpointId, RelayUrl};

use crate::peer_storage::endpoint::PeerEndpoint;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{Request, Response};
use crate::peer_storage::streaming;

impl PeerEndpoint {
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
        let file = tokio::fs::File::open(source_path).await.map_err(|e| {
            PeerStorageError::ProtocolError {
                reason: format!("open source '{}': {e}", source_path.display()),
            }
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
                streaming::PipelineError::Cancelled => PeerStorageError::Cancelled,
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
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
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
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
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
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }
}
