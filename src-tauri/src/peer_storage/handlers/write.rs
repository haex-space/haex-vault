//! `Request::Write` handler — accept an inbound file stream and persist atomically.

use std::collections::HashSet;
use tokio::sync::RwLock;

#[cfg(target_os = "android")]
use crate::peer_storage::android;
use crate::peer_storage::endpoint::PeerState;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::helpers::resolve_path_for_write;
use crate::peer_storage::protocol::Response;
use crate::peer_storage::streaming;

use super::common::{check_content_uri, send_response_and_finish};

pub(super) async fn handle_write(
    send: &mut iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    state: &RwLock<PeerState>,
    path: &str,
    size: u64,
    allowed_spaces: &HashSet<String>,
) -> Result<(), PeerStorageError> {
    let content_uri_info = {
        let state = state.read().await;
        check_content_uri(&state, allowed_spaces, path)
    };

    if let Some(uri_info) = content_uri_info {
        #[cfg(target_os = "android")]
        {
            return android::handle_write_content_uri(
                send,
                recv,
                &uri_info
                    .app_handle
                    .ok_or_else(|| PeerStorageError::ProtocolError {
                        reason: "AppHandle not available".to_string(),
                    })?,
                &uri_info.root_uri,
                &uri_info.sub_path,
                size,
            )
            .await;
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (uri_info, recv, size);
            let resp = Response::Error {
                message: "Content URIs are only supported on Android".to_string(),
            };
            send_response_and_finish(send, &resp).await.ok();
            return Ok(());
        }
    }

    let local_path = {
        let state = state.read().await;
        match resolve_path_for_write(&state.shares, allowed_spaces, path) {
            Ok(p) => p,
            Err(resp) => {
                send_response_and_finish(send, &resp).await.ok();
                return Ok(());
            }
        }
    };

    // Create parent directories
    if let Some(parent) = local_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(PeerStorageError::Io)?;
    }

    // Stage to a sibling `.part` file and rename atomically once the
    // advertised byte count has fully arrived. This prevents truncated
    // streams (dropped connections, early EOF) from clobbering an existing
    // file at `local_path` with partial data.
    let temp_path = {
        let mut name = local_path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();
        name.push(".part");
        local_path.with_file_name(name)
    };
    let file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(PeerStorageError::Io)?;

    let write_result =
        streaming::pipe_recv_to_writer(recv, file, size, streaming::RecvOptions::default())
            .await
            .map_err(|e| match e {
                streaming::PipelineError::Io(e) => PeerStorageError::Io(e),
                streaming::PipelineError::Stream(reason) => {
                    PeerStorageError::ConnectionFailed { reason }
                }
                streaming::PipelineError::Cancelled => PeerStorageError::ProtocolError {
                    reason: "Transfer cancelled".to_string(),
                },
            })
            .and_then(|stats| {
                if stats.bytes != size {
                    Err(PeerStorageError::ConnectionFailed {
                        reason: format!(
                            "stream ended early during write: expected {size} bytes, received {}",
                            stats.bytes
                        ),
                    })
                } else {
                    Ok(())
                }
            });

    if let Err(e) = write_result {
        let _ = tokio::fs::remove_file(&temp_path).await;
        let resp = Response::Error {
            message: e.to_string(),
        };
        send_response_and_finish(send, &resp).await.ok();
        return Err(e);
    }

    if let Err(e) = tokio::fs::rename(&temp_path, &local_path).await {
        let _ = tokio::fs::remove_file(&temp_path).await;
        let resp = Response::Error {
            message: format!("Failed to finalize file: {e}"),
        };
        send_response_and_finish(send, &resp).await.ok();
        return Err(PeerStorageError::Io(e));
    }

    send_response_and_finish(send, &Response::WriteOk).await
}
