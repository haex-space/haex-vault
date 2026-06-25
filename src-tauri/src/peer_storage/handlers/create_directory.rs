//! `Request::CreateDirectory` handler — mkdir -p across the shared filesystem.

use std::collections::HashSet;
use tokio::sync::RwLock;

#[cfg(target_os = "android")]
use crate::peer_storage::android;
use crate::peer_storage::endpoint::{is_content_uri, PeerState};
use crate::peer_storage::helpers::{find_share_and_subpath, resolve_path_for_write};
use crate::peer_storage::protocol::Response;

pub(super) async fn handle_create_directory(
    state: &RwLock<PeerState>,
    path: &str,
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
                    android::create_directory_content_uri(&app_handle, &root_uri, &_sub)
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
