//! `Request::List` handler — directory listings, including Android Content URI.

use std::collections::HashSet;
use tokio::sync::RwLock;

#[cfg(target_os = "android")]
use crate::peer_storage::android;
use crate::peer_storage::endpoint::PeerState;
use crate::peer_storage::helpers::{filter_shares, read_dir_entries, resolve_path_filtered};
use crate::peer_storage::protocol::{FileEntry, Response};

use super::common::check_content_uri;

pub(super) async fn handle_list(
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
                android::list_content_uri(&app_handle, &root_uri, &sub_path)
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
