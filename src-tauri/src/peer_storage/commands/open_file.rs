//! Open-file commands + Android MediaStore copy helpers.

use crate::peer_storage::error::PeerStorageError;

// ============================================================================
// Open file with system app (cross-platform)
// ============================================================================

/// Open a file with the system's default app.
/// On Android, uses android_fs FileOpener (Intent-based).
/// On Desktop, uses tauri-plugin-opener.
pub fn open_file_with_system(
    #[allow(unused_variables)] app: &tauri::AppHandle,
    path: &str,
) -> Result<(), PeerStorageError> {
    #[cfg(target_os = "android")]
    {
        use tauri_plugin_android_fs::{AndroidFsExt, FsUri};

        let api = app.android_fs();
        let uri = if path.starts_with('{') {
            FsUri::from_json_str(path).map_err(|e| PeerStorageError::ProtocolError {
                reason: format!("Invalid Content URI: {e:?}"),
            })?
        } else {
            FsUri::from_path(path)
        };
        api.opener()
            .open_file(&uri)
            .map_err(|e| PeerStorageError::ProtocolError {
                reason: format!("Failed to open file: {e:?}"),
            })?;
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_path(path, None::<String>).map_err(|e| {
            PeerStorageError::ProtocolError {
                reason: format!("Failed to open file: {e}"),
            }
        })?;
    }
    Ok(())
}

/// Tauri command wrapper for open_file_with_system.
#[tauri::command(rename_all = "camelCase")]
pub async fn open_file_system(app: tauri::AppHandle, path: String) -> Result<(), PeerStorageError> {
    open_file_with_system(&app, &path)
}

// ============================================================================
// Helpers
// ============================================================================

/// On Android, copy a downloaded file from the app-private directory to the
/// public Downloads folder via MediaStore so it becomes visible in the system
/// file manager. The `sub_path` parameter places the file under a relative
/// directory inside Downloads (e.g. `HaexVault/My Space`) — MediaStore creates
/// the directory chain on demand. Returns the FsUri JSON string of the
/// public file on Android, or the original path string on other platforms.
pub(super) fn move_to_public_downloads(
    #[allow(unused_variables)] app_handle: &tauri::AppHandle,
    output_path: &std::path::Path,
    #[allow(unused_variables)] sub_path: Option<&str>,
) -> String {
    #[cfg(target_os = "android")]
    {
        use tauri_plugin_android_fs::{AndroidFsExt, PublicGeneralPurposeDir};

        let file_name = output_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        // MediaStore takes a single relative_path that includes the file name.
        // Build `HaexVault/<space>/<file>` here so the dir chain materialises.
        let relative_path = match sub_path {
            Some(s) if !s.is_empty() => format!("{s}/{file_name}"),
            _ => file_name.clone(),
        };

        let result: Result<String, String> = (|| {
            let api = app_handle.android_fs();
            let ps = api.public_storage();

            let dest_uri = ps
                .create_new_file(
                    None,
                    PublicGeneralPurposeDir::Download,
                    &relative_path,
                    None,
                )
                .map_err(|e| format!("create_new_file: {e:?}"))?;

            // Stream-copy from app-private temp file to public Downloads
            let mut src = std::fs::File::open(output_path).map_err(|e| format!("open src: {e}"))?;
            let mut dest = api
                .open_file_writable(&dest_uri)
                .map_err(|e| format!("open dest: {e:?}"))?;
            std::io::copy(&mut src, &mut dest).map_err(|e| format!("copy: {e}"))?;
            drop(dest);

            // Clean up temp file
            let _ = std::fs::remove_file(output_path);

            Ok(dest_uri
                .to_json_string()
                .map_err(|e| format!("to_json: {e:?}"))?)
        })();

        match result {
            Ok(uri_json) => uri_json,
            Err(e) => {
                eprintln!("[peer_storage] Failed to move to public Downloads: {e}");
                // Fallback: return original path
                output_path.to_string_lossy().to_string()
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        output_path.to_string_lossy().to_string()
    }
}

/// Verify that a previously-recorded local path still references a file
/// with the expected size. Returns `true` only on an exact match — any I/O
/// error, missing target, or size mismatch is treated as a cache miss and
/// triggers a fresh download.
///
/// On desktop, `local_path` is a filesystem path. On Android, it is a
/// JSON-encoded `FsUri` pointing at a MediaStore entry; we call
/// `android_fs.get_len` which returns an error if the user has deleted
/// the file via the system file manager.
pub(super) fn verify_local_target_intact(
    #[allow(unused_variables)] app_handle: &tauri::AppHandle,
    local_path: &str,
    expected_size: u64,
) -> bool {
    #[cfg(target_os = "android")]
    {
        if local_path.starts_with('{') {
            use tauri_plugin_android_fs::{AndroidFsExt, FsUri};
            let Ok(uri) = FsUri::from_json_str(local_path) else {
                return false;
            };
            return app_handle
                .android_fs()
                .get_len(&uri)
                .map(|len| len == expected_size)
                .unwrap_or(false);
        }
        // Fall through to filesystem check for non-URI paths (legacy rows).
    }

    match std::fs::metadata(local_path) {
        Ok(m) => m.is_file() && m.len() == expected_size,
        Err(_) => false,
    }
}
