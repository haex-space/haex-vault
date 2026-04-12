//! Android Content URI helpers for peer storage.
//!
//! Uses `tauri_plugin_android_fs` to read/write files via the Storage Access
//! Framework. These functions are only compiled on Android.

#[cfg(target_os = "android")]
use crate::peer_storage::error::PeerStorageError;
#[cfg(target_os = "android")]
use crate::peer_storage::protocol::{self, FileEntry, Response};

/// Resolve a relative sub-path within a Content URI tree by navigating directory
/// by directory. Returns the target Content URI JSON string and whether it's a dir.
#[cfg(target_os = "android")]
pub(super) fn resolve_content_uri_subpath(
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
) -> Result<(tauri_plugin_android_fs::FileUri, bool), String> {
    use tauri_plugin_android_fs::{AndroidFsExt, FileUri};

    crate::filesystem::reject_path_traversal(sub_path)?;

    let api = app_handle.android_fs();
    let root = FileUri::from_json_str(root_uri_json)
        .map_err(|e| format!("Invalid Content URI: {e:?}"))?;

    let segments: Vec<&str> = sub_path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    if segments.is_empty() {
        let is_dir = api
            .get_type(&root)
            .map(|t| t == tauri_plugin_android_fs::EntryType::Dir)
            .unwrap_or(true);
        return Ok((root, is_dir));
    }

    let mut current = root;

    for (i, segment) in segments.iter().enumerate() {
        let entries = api
            .read_dir(&current)
            .map_err(|e| format!("Failed to read dir: {e:?}"))?;

        let found = entries.into_iter().find(|entry| entry.name() == *segment);

        match found {
            Some(entry) => {
                let is_dir = entry.is_dir();
                let is_last = i == segments.len() - 1;
                current = entry.uri().clone();

                if !is_last && !is_dir {
                    return Err(format!("Path segment '{}' is not a directory", segment));
                }

                if is_last {
                    return Ok((current, is_dir));
                }
            }
            None => return Err(format!("Not found: {}", segment)),
        }
    }

    unreachable!()
}

/// List directory contents via Content URI.
#[cfg(target_os = "android")]
pub(super) fn list_content_uri(
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
) -> Result<Vec<FileEntry>, String> {
    use tauri_plugin_android_fs::AndroidFsExt;

    let (target_uri, is_dir) = resolve_content_uri_subpath(app_handle, root_uri_json, sub_path)?;

    if !is_dir {
        return Err("Not a directory".to_string());
    }

    let api = app_handle.android_fs();
    let dir_entries = api
        .read_dir(&target_uri)
        .map_err(|e| format!("Failed to read dir: {e:?}"))?;

    let mut entries: Vec<FileEntry> = dir_entries
        .into_iter()
        .map(|entry| {
            let modified = entry
                .last_modified()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs());

            FileEntry {
                name: entry.name().to_string(),
                size: entry.file_len().unwrap_or(0),
                is_dir: entry.is_dir(),
                modified,
            }
        })
        .collect();

    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

/// Get file/dir metadata via Content URI.
#[cfg(target_os = "android")]
pub(super) fn stat_content_uri(
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
) -> Result<FileEntry, String> {
    use tauri_plugin_android_fs::AndroidFsExt;

    let (target_uri, is_dir) = resolve_content_uri_subpath(app_handle, root_uri_json, sub_path)?;
    let api = app_handle.android_fs();

    let info = api
        .get_info(&target_uri)
        .map_err(|e| format!("Failed to get info: {e:?}"))?;

    let modified = info
        .last_modified()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs());

    Ok(FileEntry {
        name: info.name().to_string(),
        size: info.file_len().unwrap_or(0),
        is_dir,
        modified,
    })
}

/// Stream a file via Content URI to the QUIC send stream.
///
/// The JNI file descriptor stays entirely within a `spawn_blocking` thread that
/// reads chunks and sends them over an `mpsc` channel.  The async side receives
/// chunks and writes them to QUIC.  This avoids fd-lifetime issues that arise
/// when converting a JNI `std::fs::File` to `tokio::fs::File` (the
/// `ParcelFileDescriptor` on the Java side can be GC'd while the async read is
/// still in progress, invalidating the fd).
#[cfg(target_os = "android")]
pub(super) async fn handle_read_content_uri(
    send: &mut iroh::endpoint::SendStream,
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
    range: Option<[u64; 2]>,
) -> Result<(), PeerStorageError> {
    use std::io::Read;
    use tauri_plugin_android_fs::AndroidFsExt;

    let app = app_handle.clone();
    let root = root_uri_json.to_string();
    let sub = sub_path.to_string();

    // Step 1: Resolve Content URI and get file size (blocking JNI)
    let (file_size, target_root, target_sub) = tokio::task::spawn_blocking({
        let app = app.clone();
        let root = root.clone();
        let sub = sub.clone();
        move || -> Result<(u64, String, String), PeerStorageError> {
            let api = app.android_fs();
            let (target_uri, is_dir) = resolve_content_uri_subpath(&app, &root, &sub)
                .map_err(|e| PeerStorageError::ProtocolError { reason: e })?;

            if is_dir {
                return Err(PeerStorageError::ProtocolError {
                    reason: "Not a file".to_string(),
                });
            }

            let size = api.get_len(&target_uri).unwrap_or(0);
            eprintln!("[PeerStorage] Content URI read: size={size}, path={sub}");
            Ok((size, root, sub))
        }
    })
    .await
    .map_err(|e| PeerStorageError::ProtocolError {
        reason: format!("Task failed: {e}"),
    })??;

    let (offset, read_size) = match range {
        Some([start, end]) => {
            let end = end.min(file_size);
            (start, end - start)
        }
        None => (0, file_size),
    };

    // Step 2: Send header
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

    if read_size == 0 {
        send.finish()
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;
        return Ok(());
    }

    // Step 3: Channel-based streaming — fd stays in the blocking thread
    // 4 chunks in-flight ≈ 256KB buffer, provides backpressure
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, String>>(4);

    let reader_handle = tokio::task::spawn_blocking(move || {
        let api = app.android_fs();

        let (target_uri, _) = match resolve_content_uri_subpath(&app, &target_root, &target_sub) {
            Ok(v) => v,
            Err(e) => {
                let _ = tx.blocking_send(Err(format!("Failed to resolve URI: {e}")));
                return;
            }
        };

        let mut file = match api.open_file_readable(&target_uri) {
            Ok(f) => f,
            Err(e) => {
                let _ = tx.blocking_send(Err(format!("Failed to open file: {e:?}")));
                return;
            }
        };

        // Seek to offset if needed
        if offset > 0 {
            use std::io::Seek;
            if let Err(e) = file.seek(std::io::SeekFrom::Start(offset)) {
                let _ = tx.blocking_send(Err(format!("Failed to seek: {e}")));
                return;
            }
        }

        let mut remaining = read_size;
        let mut buf = vec![0u8; 64 * 1024];
        let mut chunks_sent: u64 = 0;

        while remaining > 0 {
            let to_read = (remaining as usize).min(buf.len());
            match file.read(&mut buf[..to_read]) {
                Ok(0) => {
                    eprintln!(
                        "[PeerStorage] Content URI read: EOF after {chunks_sent} chunks, \
                         {remaining} bytes remaining"
                    );
                    break;
                }
                Ok(n) => {
                    remaining -= n as u64;
                    chunks_sent += 1;
                    if tx.blocking_send(Ok(buf[..n].to_vec())).is_err() {
                        eprintln!(
                            "[PeerStorage] Content URI read: receiver dropped after \
                             {chunks_sent} chunks"
                        );
                        return;
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[PeerStorage] Content URI read: IO error after {chunks_sent} chunks: {e}"
                    );
                    let _ = tx.blocking_send(Err(format!("Read error: {e}")));
                    return;
                }
            }
        }

        eprintln!("[PeerStorage] Content URI read: complete, {chunks_sent} chunks sent");
    });

    // Step 4: Receive chunks and write to QUIC stream
    while let Some(result) = rx.recv().await {
        match result {
            Ok(chunk) => {
                send.write_all(&chunk)
                    .await
                    .map_err(|e| PeerStorageError::ConnectionFailed {
                        reason: e.to_string(),
                    })?;
            }
            Err(e) => {
                eprintln!("[PeerStorage] Content URI streaming error: {e}");
                return Err(PeerStorageError::ProtocolError { reason: e });
            }
        }
    }

    let _ = reader_handle.await;

    send.finish()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: e.to_string(),
        })?;

    Ok(())
}

/// Write a file via Content URI. Receives file data from QUIC stream.
#[cfg(target_os = "android")]
pub(super) async fn handle_write_content_uri(
    send: &mut iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
    size: u64,
) -> Result<(), PeerStorageError> {
    use std::io::Write;
    use tauri_plugin_android_fs::AndroidFsExt;

    // Read all data from the stream first
    let mut data = Vec::with_capacity(size as usize);
    let mut buf = [0u8; 64 * 1024];
    let mut remaining = size;
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
                data.extend_from_slice(&buf[..n]);
                remaining -= n as u64;
            }
            None => break,
        }
    }

    // Write via android_fs (blocking JNI)
    let app = app_handle.clone();
    let root = root_uri_json.to_string();
    let sub = sub_path.to_string();

    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let api = app.android_fs();

        let path = std::path::Path::new(&sub);
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid file name")?;

        let (parent_uri, _) = resolve_content_uri_subpath(
            &app,
            &root,
            path.parent().and_then(|p| p.to_str()).unwrap_or(""),
        )
        .map_err(|e| format!("Failed to resolve parent: {e}"))?;

        let file_uri = api
            .create_new_file(&parent_uri, file_name, None)
            .map_err(|e| format!("Failed to create file: {e:?}"))?;

        let mut file = api
            .open_file_writable(&file_uri)
            .map_err(|e| format!("Failed to open for writing: {e:?}"))?;
        file.write_all(&data)
            .map_err(|e| format!("Failed to write: {e}"))?;

        Ok(())
    })
    .await
    .map_err(|e| PeerStorageError::ProtocolError {
        reason: format!("Task failed: {e}"),
    })?;

    let resp = match result {
        Ok(()) => Response::WriteOk,
        Err(e) => Response::Error { message: e },
    };

    super::handlers::send_response_and_finish(send, &resp).await
}

/// Delete a file/directory via Content URI.
#[cfg(target_os = "android")]
pub(super) fn delete_content_uri(
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
    _to_trash: bool,
) -> Result<(), String> {
    use tauri_plugin_android_fs::AndroidFsExt;

    let (target_uri, is_dir) = resolve_content_uri_subpath(app_handle, root_uri_json, sub_path)?;
    let api = app_handle.android_fs();
    if is_dir {
        api.remove_dir_all(&target_uri)
    } else {
        api.remove_file(&target_uri)
    }
    .map_err(|e| format!("Failed to delete: {e:?}"))
}

/// Create a directory via Content URI.
#[cfg(target_os = "android")]
pub(super) fn create_directory_content_uri(
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
) -> Result<(), String> {
    use tauri_plugin_android_fs::AndroidFsExt;

    crate::filesystem::reject_path_traversal(sub_path)?;

    let trimmed = sub_path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Ok(()); // Root already exists
    }

    let api = app_handle.android_fs();
    let root = tauri_plugin_android_fs::FileUri::from_json_str(root_uri_json)
        .map_err(|e| format!("Invalid Content URI: {e:?}"))?;

    api.create_dir_all(&root, trimmed)
        .map_err(|e| format!("Failed to create directory '{}': {e:?}", trimmed))?;

    Ok(())
}
