use super::{e2e_pick_override_path, FsError};

/// Open a folder selection dialog
#[tauri::command]
pub async fn filesystem_select_folder(
    #[allow(unused_variables)] window: tauri::WebviewWindow,
    #[allow(unused_variables)] title: Option<String>,
    #[allow(unused_variables)] default_path: Option<String>,
    #[allow(unused_variables)] app_handle: tauri::AppHandle,
) -> Result<Option<String>, FsError> {
    // E2E test escape hatch — see top-of-file comment on e2e_pick_override_path.
    if let Some(path) = e2e_pick_override_path("HAEX_E2E_PICK_FOLDER_FILE", "folder") {
        let trimmed = std::fs::read_to_string(&path)
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        return Ok(if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        });
    }

    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_dialog::DialogExt;

        let mut dialog = window.dialog().file();

        if let Some(t) = title {
            dialog = dialog.set_title(&t);
        }

        if let Some(path) = default_path {
            dialog = dialog.set_directory(&path);
        }

        let selected = dialog.blocking_pick_folder();

        Ok(selected.and_then(|p| p.as_path().map(|path| path.to_string_lossy().to_string())))
    }

    #[cfg(target_os = "android")]
    {
        use tauri_plugin_android_fs::AndroidFsExt;

        let api = app_handle.android_fs();
        let picker = api.picker();

        let selected = picker.pick_dir(None, true).map_err(|e| FsError::IoError {
            reason: format!("Android folder picker error: {:?}", e),
        })?;

        match selected {
            Some(uri) => {
                // Persist URI permission so it survives app restarts.
                // Without this, Android revokes access when the app is terminated.
                picker
                    .persist_uri_permission(&uri)
                    .map_err(|e| FsError::IoError {
                        reason: format!("Failed to persist URI permission: {:?}", e),
                    })?;

                let uri_json = uri.to_json_string().map_err(|e| FsError::IoError {
                    reason: format!("Failed to serialize URI: {:?}", e),
                })?;
                Ok(Some(uri_json))
            }
            None => Ok(None),
        }
    }
}

/// Open a file selection dialog
#[tauri::command]
pub async fn filesystem_select_file(
    #[allow(unused_variables)] window: tauri::WebviewWindow,
    #[allow(unused_variables)] title: Option<String>,
    #[allow(unused_variables)] default_path: Option<String>,
    #[allow(unused_variables)] filters: Option<Vec<(String, Vec<String>)>>,
    #[allow(unused_variables)] multiple: Option<bool>,
    #[allow(unused_variables)] app_handle: tauri::AppHandle,
) -> Result<Option<Vec<String>>, FsError> {
    // E2E test escape hatch — see top-of-file comment on e2e_pick_override_path.
    // File contains one path per line (multi-select). Honors the `multiple`
    // flag so behavior matches the production single-pick path.
    if let Some(path) = e2e_pick_override_path("HAEX_E2E_PICK_FILE_FILE", "file") {
        let mut paths: Vec<String> = std::fs::read_to_string(&path)
            .ok()
            .map(|s| {
                s.lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if !multiple.unwrap_or(false) && paths.len() > 1 {
            paths.truncate(1);
        }
        return Ok(if paths.is_empty() { None } else { Some(paths) });
    }

    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_dialog::DialogExt;

        let mut dialog = window.dialog().file();

        if let Some(t) = title {
            dialog = dialog.set_title(&t);
        }

        if let Some(path) = default_path {
            dialog = dialog.set_directory(&path);
        }

        if let Some(f) = filters {
            for (name, extensions) in f {
                let ext_refs: Vec<&str> = extensions.iter().map(|s| s.as_str()).collect();
                dialog = dialog.add_filter(&name, &ext_refs);
            }
        }

        if multiple.unwrap_or(false) {
            let selected = dialog.blocking_pick_files();
            Ok(selected.map(|paths| {
                paths
                    .into_iter()
                    .filter_map(|p| p.as_path().map(|path| path.to_string_lossy().to_string()))
                    .collect()
            }))
        } else {
            let selected = dialog.blocking_pick_file();
            Ok(selected.and_then(|p| {
                p.as_path()
                    .map(|path| vec![path.to_string_lossy().to_string()])
            }))
        }
    }

    #[cfg(target_os = "android")]
    {
        use tauri_plugin_android_fs::AndroidFsExt;

        let api = app_handle.android_fs();
        let picker = api.picker();

        // Convert extension filters to MIME types for Android
        let mime_types: Vec<String> = filters
            .as_ref()
            .map(|f| {
                f.iter()
                    .flat_map(|(_, extensions)| {
                        extensions.iter().map(|ext| {
                            match ext.to_lowercase().as_str() {
                                "jpg" | "jpeg" => "image/jpeg".to_string(),
                                "png" => "image/png".to_string(),
                                "gif" => "image/gif".to_string(),
                                "webp" => "image/webp".to_string(),
                                "svg" => "image/svg+xml".to_string(),
                                "pdf" => "application/pdf".to_string(),
                                "txt" => "text/plain".to_string(),
                                "json" => "application/json".to_string(),
                                "xml" => "application/xml".to_string(),
                                "zip" => "application/zip".to_string(),
                                "mp3" => "audio/mpeg".to_string(),
                                "mp4" => "video/mp4".to_string(),
                                "doc" => "application/msword".to_string(),
                                "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string(),
                                "xls" => "application/vnd.ms-excel".to_string(),
                                "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
                                _ => "*/*".to_string(),
                            }
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mime_refs: Vec<&str> = if mime_types.is_empty() {
            vec!["*/*"]
        } else {
            mime_types.iter().map(|s| s.as_str()).collect()
        };

        if multiple.unwrap_or(false) {
            let selected =
                picker
                    .pick_files(None, &mime_refs, false)
                    .map_err(|e| FsError::IoError {
                        reason: format!("Android file picker error: {:?}", e),
                    })?;

            if selected.is_empty() {
                Ok(None)
            } else {
                // Persist URI permissions so they survive app restarts.
                // Without this, Android revokes access when the app is terminated.
                for uri in &selected {
                    picker
                        .persist_uri_permission(uri)
                        .map_err(|e| FsError::IoError {
                            reason: format!("Failed to persist URI permission: {:?}", e),
                        })?;
                }
                let uris: Result<Vec<String>, FsError> = selected
                    .into_iter()
                    .map(|uri| {
                        uri.to_json_string().map_err(|e| FsError::IoError {
                            reason: format!("Failed to serialize URI: {:?}", e),
                        })
                    })
                    .collect();
                Ok(Some(uris?))
            }
        } else {
            let selected =
                picker
                    .pick_file(None, &mime_refs, false)
                    .map_err(|e| FsError::IoError {
                        reason: format!("Android file picker error: {:?}", e),
                    })?;

            match selected {
                Some(uri) => {
                    // Persist URI permission so it survives app restarts.
                    // Without this, Android revokes access when the app is terminated.
                    picker
                        .persist_uri_permission(&uri)
                        .map_err(|e| FsError::IoError {
                            reason: format!("Failed to persist URI permission: {:?}", e),
                        })?;
                    let uri_json = uri.to_json_string().map_err(|e| FsError::IoError {
                        reason: format!("Failed to serialize URI: {:?}", e),
                    })?;
                    Ok(Some(vec![uri_json]))
                }
                None => Ok(None),
            }
        }
    }
}
