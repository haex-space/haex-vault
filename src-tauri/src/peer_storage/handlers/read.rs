//! `Request::Read` handler — stream a file (or Content URI) over QUIC.

use std::collections::HashSet;
use std::path::Path;
use tokio::sync::RwLock;

#[cfg(target_os = "android")]
use crate::peer_storage::android;
use crate::peer_storage::endpoint::PeerState;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::helpers::resolve_path_filtered;
use crate::peer_storage::protocol::{self, Response};
use crate::peer_storage::streaming;

use super::common::{check_content_uri, send_response_and_finish};

pub(super) async fn handle_read(
    send: &mut iroh::endpoint::SendStream,
    state: &RwLock<PeerState>,
    path: &str,
    range: Option<[u64; 2]>,
    allowed_spaces: &HashSet<String>,
) -> Result<(), PeerStorageError> {
    let content_uri_info = {
        let state = state.read().await;
        check_content_uri(&state, allowed_spaces, path)
    };

    if let Some(uri_info) = content_uri_info {
        #[cfg(target_os = "android")]
        {
            let app_handle =
                uri_info
                    .app_handle
                    .ok_or_else(|| PeerStorageError::ProtocolError {
                        reason: "AppHandle not available".to_string(),
                    })?;
            return android::handle_read_content_uri(
                send,
                &app_handle,
                &uri_info.root_uri,
                &uri_info.sub_path,
                range,
            )
            .await;
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = uri_info;
            let resp = Response::Error {
                message: "Content URIs are only supported on Android".to_string(),
            };
            send_response_and_finish(send, &resp).await.ok();
            return Ok(());
        }
    }

    let local_path = {
        let state = state.read().await;
        match resolve_path_filtered(&state.shares, allowed_spaces, path) {
            Ok(p) => p,
            Err(resp) => {
                send_response_and_finish(send, &resp).await.ok();
                return Ok(());
            }
        }
    };

    if !local_path.is_file() {
        let resp = Response::Error {
            message: "Not a file".to_string(),
        };
        send_response_and_finish(send, &resp).await.ok();
        return Ok(());
    }

    stream_file_to_send(send, &local_path, range).await
}

/// Stream a local file to the QUIC send stream via the shared
/// disk → network pipeline in [`crate::peer_storage::streaming`].
async fn stream_file_to_send(
    send: &mut iroh::endpoint::SendStream,
    local_path: &Path,
    range: Option<[u64; 2]>,
) -> Result<(), PeerStorageError> {
    use tokio::io::AsyncSeekExt;

    let mut file = tokio::fs::File::open(local_path)
        .await
        .map_err(PeerStorageError::Io)?;

    let metadata = file.metadata().await.map_err(PeerStorageError::Io)?;
    let file_size = metadata.len();

    let (offset, read_size) = match range {
        Some([start, end]) => {
            let end = end.min(file_size);
            (start, end - start)
        }
        None => (0, file_size),
    };

    let header = Response::ReadHeader { size: read_size };
    let header_bytes =
        protocol::encode_response(&header).map_err(|e| PeerStorageError::ProtocolError {
            reason: e.to_string(),
        })?;
    send.write_all(&header_bytes)
        .await
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: e.to_string(),
        })?;

    if offset > 0 {
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(PeerStorageError::Io)?;
    }

    streaming::pipe_reader_to_send(send, file, read_size, streaming::SendOptions::default())
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

    send.finish()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: e.to_string(),
        })?;

    Ok(())
}
