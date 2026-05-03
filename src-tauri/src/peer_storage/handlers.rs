//! Server-side request handlers for incoming peer storage connections.

use std::collections::HashSet;
use std::path::Path;
use tokio::sync::RwLock;

use crate::peer_storage::endpoint::{is_content_uri, PeerState};
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::helpers::{
    file_entry_from_path, filter_shares, find_share_and_subpath, find_space_for_path,
    read_dir_entries, resolve_path_filtered, resolve_path_for_write, scan_directory_recursive,
};
use crate::peer_storage::protocol::{self, FileEntry, Request, Response};

// ============================================================================
// DRY helpers
// ============================================================================

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
pub(super) async fn send_response_and_finish(
    send: &mut iroh::endpoint::SendStream,
    response: &Response,
) -> Result<(), PeerStorageError> {
    let bytes = protocol::encode_response(response)
        .map_err(|e| PeerStorageError::ProtocolError {
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

// ============================================================================
// Stream dispatcher
// ============================================================================

pub(super) async fn handle_stream(
    mut send: iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    state: &RwLock<PeerState>,
    allowed_spaces: &HashSet<String>,
) -> Result<(), PeerStorageError> {
    let request = protocol::read_request(recv)
        .await
        .map_err(|e| PeerStorageError::ProtocolError {
            reason: e.to_string(),
        })?;

    // ── Layer 1 (first line of defense): validate UCAN signature + expiry ──
    let validated_ucan = match crate::ucan::validate_token(request.ucan_token()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[PeerStorage] UCAN validation failed: {e}");
            let resp = Response::Error {
                message: format!("UCAN validation failed: {e}"),
            };
            send_response_and_finish(&mut send, &resp).await.ok();
            return Ok(());
        }
    };

    // ── Layer 2 (source of truth): check capability matches operation ──
    let target_space_id = {
        let s = state.read().await;
        let path = match &request {
            Request::List { path, .. }
            | Request::Stat { path, .. }
            | Request::Read { path, .. }
            | Request::Manifest { path, .. }
            | Request::Write { path, .. }
            | Request::Delete { path, .. }
            | Request::CreateDirectory { path, .. } => path.as_str(),
        };
        find_space_for_path(&s.shares, allowed_spaces, path)
    };

    if let Some(space_id) = &target_space_id {
        let required = if request.requires_write() {
            crate::ucan::CapabilityLevel::Write
        } else {
            crate::ucan::CapabilityLevel::Read
        };

        if let Err(e) = crate::ucan::require_capability(&validated_ucan, space_id, required) {
            eprintln!("[PeerStorage] UCAN capability check failed: {e}");
            let resp = Response::Error {
                message: format!("Access denied: {e}"),
            };
            send_response_and_finish(&mut send, &resp).await.ok();
            return Ok(());
        }
    }

    let response = match request {
        Request::List { path, .. } => handle_list(state, &path, allowed_spaces).await,
        Request::Stat { path, .. } => handle_stat(state, &path, allowed_spaces).await,
        Request::Manifest { path, .. } => handle_manifest(state, &path, allowed_spaces).await,
        Request::Read {
            path, range, ..
        } => {
            if let Err(e) = handle_read(&mut send, state, &path, range, allowed_spaces).await {
                eprintln!("[PeerStorage] Read error for '{path}': {e}");
                let error_resp = Response::Error {
                    message: format!("{e}"),
                };
                send_response_and_finish(&mut send, &error_resp).await.ok();
                return Err(e);
            }
            return Ok(());
        }
        Request::Write { path, size, .. } => {
            if let Err(e) =
                handle_write(&mut send, recv, state, &path, size, allowed_spaces).await
            {
                eprintln!("[PeerStorage] Write error for '{path}': {e}");
                let error_resp = Response::Error {
                    message: format!("{e}"),
                };
                send_response_and_finish(&mut send, &error_resp).await.ok();
                return Err(e);
            }
            return Ok(());
        }
        Request::Delete {
            path, to_trash, ..
        } => handle_delete(state, &path, to_trash, allowed_spaces).await,
        Request::CreateDirectory { path, .. } => {
            handle_create_directory(state, &path, allowed_spaces).await
        }
    };

    send_response_and_finish(&mut send, &response).await
}

// ============================================================================
// Request handlers
// ============================================================================

async fn handle_list(
    state: &RwLock<PeerState>,
    path: &str,
    allowed_spaces: &HashSet<String>,
) -> Response {
    let state = state.read().await;

    if path.is_empty() || path == "/" {
        let filtered = filter_shares(&state.shares, allowed_spaces);
        let entries: Vec<FileEntry> = filtered
            .iter()
            .map(|(_id, share)| FileEntry {
                name: share.name.clone(),
                size: 0,
                is_dir: true,
                modified: None,
            })
            .collect();
        return Response::List { entries };
    }

    if let Some(_uri_info) = check_content_uri(&state, allowed_spaces, path) {
        #[cfg(target_os = "android")]
        {
            let app_handle = match _uri_info.app_handle {
                Some(h) => h,
                None => {
                    return Response::Error {
                        message: "AppHandle not available".to_string(),
                    }
                }
            };
            let root_uri = _uri_info.root_uri;
            let sub_path = _uri_info.sub_path;
            drop(state);
            return match tokio::task::spawn_blocking(move || {
                super::android::list_content_uri(&app_handle, &root_uri, &sub_path)
            })
            .await
            {
                Ok(Ok(entries)) => Response::List { entries },
                Ok(Err(e)) => Response::Error { message: e },
                Err(e) => Response::Error {
                    message: format!("Task failed: {e}"),
                },
            };
        }
        #[cfg(not(target_os = "android"))]
        return Response::Error {
            message: "Content URIs are only supported on Android".to_string(),
        };
    }

    let local_path = match resolve_path_filtered(&state.shares, allowed_spaces, path) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    if !local_path.is_dir() {
        return Response::Error {
            message: "Not a directory".to_string(),
        };
    }

    match read_dir_entries(&local_path).await {
        Ok(entries) => Response::List { entries },
        Err(e) => Response::Error {
            message: format!("Failed to list directory: {e}"),
        },
    }
}

async fn handle_stat(
    state: &RwLock<PeerState>,
    path: &str,
    allowed_spaces: &HashSet<String>,
) -> Response {
    let state = state.read().await;

    if let Some(_uri_info) = check_content_uri(&state, allowed_spaces, path) {
        #[cfg(target_os = "android")]
        {
            let app_handle = match _uri_info.app_handle {
                Some(h) => h,
                None => {
                    return Response::Error {
                        message: "AppHandle not available".to_string(),
                    }
                }
            };
            let root_uri = _uri_info.root_uri;
            let sub_path = _uri_info.sub_path;
            drop(state);
            return match tokio::task::spawn_blocking(move || {
                super::android::stat_content_uri(&app_handle, &root_uri, &sub_path)
            })
            .await
            {
                Ok(Ok(entry)) => Response::Stat { entry },
                Ok(Err(e)) => Response::Error { message: e },
                Err(e) => Response::Error {
                    message: format!("Task failed: {e}"),
                },
            };
        }
        #[cfg(not(target_os = "android"))]
        return Response::Error {
            message: "Content URIs are only supported on Android".to_string(),
        };
    }

    let local_path = match resolve_path_filtered(&state.shares, allowed_spaces, path) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    match file_entry_from_path(&local_path) {
        Ok(entry) => Response::Stat { entry },
        Err(e) => Response::Error {
            message: format!("Failed to stat: {e}"),
        },
    }
}

async fn handle_manifest(
    state: &RwLock<PeerState>,
    path: &str,
    allowed_spaces: &HashSet<String>,
) -> Response {
    let state = state.read().await;

    if path.is_empty() || path == "/" {
        return Response::Error {
            message: "Manifest requires a share path".to_string(),
        };
    }

    if let Some(_uri_info) = check_content_uri(&state, allowed_spaces, path) {
        #[cfg(target_os = "android")]
        {
            let app_handle = match _uri_info.app_handle {
                Some(h) => h,
                None => {
                    return Response::Error {
                        message: "AppHandle not available".to_string(),
                    }
                }
            };
            let root_uri = _uri_info.root_uri;
            let sub_path = _uri_info.sub_path;
            drop(state);
            return match tokio::task::spawn_blocking(move || {
                super::android::scan_content_uri_recursive(&app_handle, &root_uri, &sub_path)
            })
            .await
            {
                Ok(Ok(entries)) => Response::Manifest { entries },
                Ok(Err(e)) => Response::Error { message: e },
                Err(e) => Response::Error {
                    message: format!("Task failed: {e}"),
                },
            };
        }
        #[cfg(not(target_os = "android"))]
        return Response::Error {
            message: "Content URIs are only supported on Android".to_string(),
        };
    }

    let local_path = match resolve_path_filtered(&state.shares, allowed_spaces, path) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    if !local_path.is_dir() {
        return Response::Error {
            message: "Not a directory".to_string(),
        };
    }

    match tokio::task::spawn_blocking({
        let base = local_path.clone();
        move || scan_directory_recursive(&local_path, &base)
    })
    .await
    {
        Ok(Ok(entries)) => Response::Manifest { entries },
        Ok(Err(e)) => Response::Error {
            message: format!("Failed to scan directory: {e}"),
        },
        Err(e) => Response::Error {
            message: format!("Task failed: {e}"),
        },
    }
}

async fn handle_read(
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
            let app_handle = uri_info
                .app_handle
                .ok_or_else(|| PeerStorageError::ProtocolError {
                    reason: "AppHandle not available".to_string(),
                })?;
            return super::android::handle_read_content_uri(
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

/// Stream a local file to the QUIC send stream in 256KB chunks.
async fn stream_file_to_send(
    send: &mut iroh::endpoint::SendStream,
    local_path: &Path,
    range: Option<[u64; 2]>,
) -> Result<(), PeerStorageError> {
    use tokio::io::AsyncReadExt;

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

    // Send header
    let header = Response::ReadHeader { size: read_size };
    let header_bytes = protocol::encode_response(&header)
        .map_err(|e| PeerStorageError::ProtocolError {
            reason: e.to_string(),
        })?;
    send.write_all(&header_bytes)
        .await
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: e.to_string(),
        })?;

    if offset > 0 {
        use tokio::io::AsyncSeekExt;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(PeerStorageError::Io)?;
    }

    // Stream file data in chunks (256 KB).
    // Larger chunks reduce per-chunk syscall + QUIC frame overhead, which
    // matters on fast LAN links where 64 KB throughput-caps were observed.
    let mut remaining = read_size;
    let mut buf = vec![0u8; 256 * 1024];

    while remaining > 0 {
        let to_read = (remaining as usize).min(buf.len());
        let n = file
            .read(&mut buf[..to_read])
            .await
            .map_err(PeerStorageError::Io)?;
        if n == 0 {
            break;
        }
        send.write_all(&buf[..n])
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;
        remaining -= n as u64;
    }

    send.finish()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: e.to_string(),
        })?;

    Ok(())
}

async fn handle_write(
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
            return super::android::handle_write_content_uri(
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
    use tokio::io::AsyncWriteExt;
    let temp_path = {
        let mut name = local_path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();
        name.push(".part");
        local_path.with_file_name(name)
    };
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(PeerStorageError::Io)?;

    let mut remaining = size;
    let mut buf = vec![0u8; 256 * 1024];

    let write_result: Result<(), PeerStorageError> = async {
        while remaining > 0 {
            let to_read = (remaining as usize).min(buf.len());
            let chunk = recv
                .read(&mut buf[..to_read])
                .await
                .map_err(|e| PeerStorageError::ConnectionFailed {
                    reason: format!("Failed to read file data: {e}"),
                })?;
            match chunk {
                Some(n) => {
                    file.write_all(&buf[..n])
                        .await
                        .map_err(PeerStorageError::Io)?;
                    remaining -= n as u64;
                }
                None => {
                    return Err(PeerStorageError::ConnectionFailed {
                        reason: format!(
                            "stream ended early during write: {remaining} bytes still expected"
                        ),
                    });
                }
            }
        }
        file.flush().await.map_err(PeerStorageError::Io)?;
        Ok(())
    }
    .await;

    if let Err(e) = write_result {
        drop(file);
        let _ = tokio::fs::remove_file(&temp_path).await;
        let resp = Response::Error {
            message: e.to_string(),
        };
        send_response_and_finish(send, &resp).await.ok();
        return Err(e);
    }

    drop(file);
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

async fn handle_delete(
    state: &RwLock<PeerState>,
    path: &str,
    to_trash: bool,
    allowed_spaces: &HashSet<String>,
) -> Response {
    // Check for Content URI shares (Android)
    if let Ok((share, _sub)) = {
        let s = state.read().await;
        find_share_and_subpath(&s.shares, allowed_spaces, path)
            .map(|(sh, sub)| (sh.clone(), sub))
    } {
        if is_content_uri(&share.local_path) {
            #[cfg(target_os = "android")]
            {
                let app_handle = {
                    let s = state.read().await;
                    match &s.app_handle {
                        Some(h) => h.clone(),
                        None => {
                            return Response::Error {
                                message: "AppHandle not available".to_string(),
                            }
                        }
                    }
                };
                let root_uri = share.local_path.clone();
                return match tokio::task::spawn_blocking(move || {
                    super::android::delete_content_uri(&app_handle, &root_uri, &_sub, to_trash)
                })
                .await
                {
                    Ok(Ok(())) => Response::DeleteOk,
                    Ok(Err(e)) => Response::Error { message: e },
                    Err(e) => Response::Error {
                        message: format!("Task failed: {e}"),
                    },
                };
            }
            #[cfg(not(target_os = "android"))]
            return Response::Error {
                message: "Content URIs are only supported on Android".to_string(),
            };
        }
    }

    let local_path = {
        let state = state.read().await;
        match resolve_path_filtered(&state.shares, allowed_spaces, path) {
            Ok(p) => p,
            Err(resp) => return resp,
        }
    };

    if !local_path.exists() {
        return Response::Error {
            message: "File not found".to_string(),
        };
    }

    if to_trash {
        #[cfg(not(target_os = "android"))]
        {
            if let Err(e) = trash::delete(&local_path) {
                return Response::Error {
                    message: format!("Failed to trash: {e}"),
                };
            }
        }
        #[cfg(target_os = "android")]
        {
            if let Err(e) = tokio::fs::remove_file(&local_path).await {
                return Response::Error {
                    message: format!("Failed to delete: {e}"),
                };
            }
        }
    } else if local_path.is_dir() {
        if let Err(e) = tokio::fs::remove_dir_all(&local_path).await {
            return Response::Error {
                message: format!("Failed to delete directory: {e}"),
            };
        }
    } else if let Err(e) = tokio::fs::remove_file(&local_path).await {
        return Response::Error {
            message: format!("Failed to delete file: {e}"),
        };
    }

    Response::DeleteOk
}

async fn handle_create_directory(
    state: &RwLock<PeerState>,
    path: &str,
    allowed_spaces: &HashSet<String>,
) -> Response {
    // Check for Content URI shares (Android)
    if let Ok((share, _sub)) = {
        let s = state.read().await;
        find_share_and_subpath(&s.shares, allowed_spaces, path)
            .map(|(sh, sub)| (sh.clone(), sub))
    } {
        if is_content_uri(&share.local_path) {
            #[cfg(target_os = "android")]
            {
                let app_handle = {
                    let s = state.read().await;
                    match &s.app_handle {
                        Some(h) => h.clone(),
                        None => {
                            return Response::Error {
                                message: "AppHandle not available".to_string(),
                            }
                        }
                    }
                };
                let root_uri = share.local_path.clone();
                return match tokio::task::spawn_blocking(move || {
                    super::android::create_directory_content_uri(&app_handle, &root_uri, &_sub)
                })
                .await
                {
                    Ok(Ok(())) => Response::CreateDirectoryOk,
                    Ok(Err(e)) => Response::Error { message: e },
                    Err(e) => Response::Error {
                        message: format!("Task failed: {e}"),
                    },
                };
            }
            #[cfg(not(target_os = "android"))]
            return Response::Error {
                message: "Content URIs are only supported on Android".to_string(),
            };
        }
    }

    let local_path = {
        let state = state.read().await;
        match resolve_path_for_write(&state.shares, allowed_spaces, path) {
            Ok(p) => p,
            Err(resp) => return resp,
        }
    };

    match tokio::fs::create_dir_all(&local_path).await {
        Ok(()) => Response::CreateDirectoryOk,
        Err(e) => Response::Error {
            message: format!("Failed to create directory: {e}"),
        },
    }
}
