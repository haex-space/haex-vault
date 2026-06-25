//! `Request::Manifest` handler — recursive directory scan with chunk hashes.

use std::collections::HashSet;
use tokio::sync::RwLock;

#[cfg(target_os = "android")]
use crate::peer_storage::android;
use crate::peer_storage::endpoint::PeerState;
use crate::peer_storage::helpers::{resolve_path_filtered, scan_directory_recursive};
use crate::peer_storage::protocol::Response;

use super::common::check_content_uri;

pub(super) async fn handle_manifest(
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
                android::scan_content_uri_recursive(&app_handle, &root_uri, &sub_path)
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
