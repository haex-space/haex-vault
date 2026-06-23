//! DRY helpers shared by every per-request handler in this module.

use std::collections::HashSet;

use crate::peer_storage::endpoint::{is_content_uri, PeerState};
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::helpers::find_share_and_subpath;
use crate::peer_storage::protocol::{self, Response};

/// Information about a Content URI share, extracted from PeerState.
#[allow(dead_code)] // Fields are read on Android only
pub(super) struct ContentUriInfo {
    pub root_uri: String,
    pub sub_path: String,
    pub app_handle: Option<tauri::AppHandle>,
}

/// Check if a request path targets a Content URI share. Returns `Some` with
/// the URI info when the share uses Android Content URIs, `None` otherwise.
pub(super) fn check_content_uri(
    state: &PeerState,
    allowed_spaces: &HashSet<String>,
    path: &str,
) -> Option<ContentUriInfo> {
    let (share, sub_path) = find_share_and_subpath(&state.shares, allowed_spaces, path).ok()?;
    if !is_content_uri(&share.local_path) {
        return None;
    }
    Some(ContentUriInfo {
        root_uri: share.local_path.clone(),
        sub_path,
        app_handle: state.app_handle.clone(),
    })
}

/// Encode a response, write it to the QUIC send stream, and signal finish.
pub(in crate::peer_storage) async fn send_response_and_finish(
    send: &mut iroh::endpoint::SendStream,
    response: &Response,
) -> Result<(), PeerStorageError> {
    let bytes =
        protocol::encode_response(response).map_err(|e| PeerStorageError::ProtocolError {
            reason: e.to_string(),
        })?;
    send.write_all(&bytes)
        .await
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: e.to_string(),
        })?;
    send.finish()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: e.to_string(),
        })?;
    Ok(())
}
