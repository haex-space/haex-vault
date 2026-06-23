//! `Request::Delete` handler — delete or move-to-trash a file or directory.

use std::collections::HashSet;
use tokio::sync::RwLock;

#[cfg(target_os = "android")]
use crate::peer_storage::android;
use crate::peer_storage::endpoint::{is_content_uri, PeerState};
use crate::peer_storage::helpers::{find_share_and_subpath, resolve_path_filtered};
use crate::peer_storage::protocol::Response;

pub(super) async fn handle_delete(
    state: &RwLock<PeerState>,
    path: &str,
    to_trash: bool,
    allowed_spaces: &HashSet<String>,
) -> Response {
    // Check for Content URI shares (Android)
    if let Ok((share, _sub)) = {
        let s = state.read().await;
        find_share_and_subpath(&s.shares, allowed_spaces, path).map(|(sh, sub)| (sh.clone(), sub))
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
                    android::delete_content_uri(&app_handle, &root_uri, &_sub, to_trash)
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
