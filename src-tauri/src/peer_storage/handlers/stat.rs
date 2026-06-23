//! `Request::Stat` handler — file/directory metadata plus chunk hashes.

use std::collections::HashSet;
use std::path::Path;
use tokio::sync::RwLock;

#[cfg(target_os = "android")]
use crate::peer_storage::android;
use crate::peer_storage::endpoint::PeerState;
use crate::peer_storage::helpers::{file_entry_from_path, resolve_path_filtered};
use crate::peer_storage::protocol::Response;

use super::common::check_content_uri;

pub(super) async fn handle_stat(
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
                android::stat_content_uri(&app_handle, &root_uri, &sub_path)
            })
            .await
            {
                // Android Content URI files now carry chunks too — hashed via
                // the SAF reader, the same path the manifest scan uses
                // (`collect_content_uri_entries`). Directories carry `None`.
                Ok(Ok((entry, chunks))) => Response::Stat { entry, chunks },
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

    stat_local_path(&local_path).await
}

/// Stat a local path and (for files) compute / fetch the cached
/// BLAKE3 chunked hash. Split out so tests can exercise the file-vs-directory
/// hash-population branch without spinning up a PeerState/UCAN harness.
pub(super) async fn stat_local_path(local_path: &Path) -> Response {
    let entry = match file_entry_from_path(local_path) {
        Ok(e) => e,
        Err(e) => {
            return Response::Error {
                message: format!("Failed to stat: {e}"),
            };
        }
    };

    if !entry.is_dir {
        let path_for_hash = local_path.to_path_buf();
        let size = entry.size;
        let mtime_nanos = match std::fs::metadata(local_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
        {
            Some(n) => n,
            None => {
                // No usable mtime (rare — e.g. some special filesystems). A
                // zero-nanos cache key would alias unrelated files, so hash
                // uncached this once — the server must still report chunks for
                // every file (see client::download_file_to_path) so the
                // receiver can verify.
                let path = local_path.to_path_buf();
                return match tokio::task::spawn_blocking(move || {
                    crate::file_sync::hashing::hash_file_chunked(&path)
                })
                .await
                {
                    Ok(Ok(chunks)) => Response::Stat {
                        entry,
                        chunks: Some(chunks),
                    },
                    Ok(Err(e)) => Response::Error {
                        message: format!("Failed to hash file: {e}"),
                    },
                    Err(e) => Response::Error {
                        message: format!("Hash task failed: {e}"),
                    },
                };
            }
        };

        // `cached_hash_chunked` is synchronous and may hash a multi-GB file;
        // spawn_blocking keeps the async runtime responsive.
        match tokio::task::spawn_blocking(move || {
            crate::file_sync::hashing::cached_hash_chunked(&path_for_hash, size, mtime_nanos)
        })
        .await
        {
            Ok(Ok(chunks)) => Response::Stat {
                entry,
                chunks: Some(chunks),
            },
            Ok(Err(e)) => Response::Error {
                message: format!("Failed to hash file: {e}"),
            },
            Err(e) => Response::Error {
                message: format!("Hash task failed: {e}"),
            },
        }
    } else {
        Response::Stat {
            entry,
            chunks: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handle_stat_returns_chunks_for_files() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("hello.bin");
        // 2 MiB + 5 bytes ⇒ exactly 3 chunks (chunk_size = 1 MiB).
        let data: Vec<u8> = (0..(2 * 1024 * 1024 + 5))
            .map(|i| (i % 251) as u8)
            .collect();
        tokio::fs::write(&file, &data).await.unwrap();

        let response = stat_local_path(&file).await;

        let Response::Stat { entry, chunks } = response else {
            panic!("expected Stat response, got {response:?}");
        };
        assert_eq!(entry.size, data.len() as u64);
        assert!(!entry.is_dir);
        let chunks = chunks.expect("file stat must include chunks");
        assert_eq!(
            chunks.chunk_size,
            crate::file_sync::hashing::CHUNK_HASH_SIZE
        );
        assert_eq!(chunks.chunk_hashes.len(), 3);
        let expected_file = blake3::hash(&data).to_hex().to_string();
        assert_eq!(chunks.file_hash, expected_file);
    }

    #[tokio::test]
    async fn handle_stat_returns_no_chunks_for_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let response = stat_local_path(tmp.path()).await;
        let Response::Stat { entry, chunks } = response else {
            panic!("expected Stat response, got {response:?}");
        };
        assert!(entry.is_dir);
        assert_eq!(chunks, None);
    }
}
