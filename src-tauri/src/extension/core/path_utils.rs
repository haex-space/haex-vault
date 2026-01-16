// src-tauri/src/extension/core/path_utils.rs
//
// Path validation and resolution utilities for extensions.

use crate::extension::error::ExtensionError;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_fs::FsExt;

/// Helper function to validate path and check for path traversal.
/// Returns the cleaned path if valid, or None if invalid/not found.
/// If require_exists is true, returns None if path doesn't exist.
pub fn validate_path_in_directory(
    base_dir: &PathBuf,
    relative_path: &str,
    require_exists: bool,
) -> Result<Option<PathBuf>, ExtensionError> {
    // Check for path traversal patterns
    if relative_path.contains("..") {
        return Err(ExtensionError::SecurityViolation {
            reason: format!("Path traversal attempt: {relative_path}"),
        });
    }

    // Clean the path (same logic as in protocol.rs)
    let clean_path = relative_path
        .replace('\\', "/")
        .trim_start_matches('/')
        .split('/')
        .filter(|&part| !part.is_empty() && part != "." && part != "..")
        .collect::<PathBuf>();

    let full_path = base_dir.join(&clean_path);

    // Check if file/directory exists (if required)
    if require_exists && !full_path.exists() {
        return Ok(None);
    }

    // Verify path is within base directory
    let canonical_base = base_dir
        .canonicalize()
        .map_err(|e| ExtensionError::Filesystem { source: e })?;

    if let Ok(canonical_path) = full_path.canonicalize() {
        if !canonical_path.starts_with(&canonical_base) {
            return Err(ExtensionError::SecurityViolation {
                reason: format!("Path outside base directory: {relative_path}"),
            });
        }
        Ok(Some(canonical_path))
    } else {
        // Path doesn't exist yet - still validate it would be within base
        if full_path.starts_with(&canonical_base) {
            Ok(Some(full_path))
        } else {
            Err(ExtensionError::SecurityViolation {
                reason: format!("Path outside base directory: {relative_path}"),
            })
        }
    }
}

/// Find icon path using FsExt (works better on Android).
/// Returns the relative path if found, None otherwise.
pub fn find_icon(
    app_handle: &AppHandle,
    extension_dir: &PathBuf,
    haextension_dir: &str,
    icon_path: Option<&str>,
) -> Option<String> {
    let fs = app_handle.fs();

    // Helper to check if path contains traversal
    let is_safe_path = |path: &str| -> bool { !path.contains("..") };

    // Helper to clean relative path
    let clean_relative = |path: &str| -> String {
        path.replace('\\', "/")
            .trim_start_matches('/')
            .to_string()
    };

    // Helper to check if file exists using FsExt
    // We try to read a small portion of the file to check existence
    let file_exists = |relative_path: &str| -> bool {
        if !is_safe_path(relative_path) {
            return false;
        }
        let clean = clean_relative(relative_path);
        let full_path = extension_dir.join(&clean);
        // Use FsExt to check if file can be read
        fs.read(&full_path).is_ok()
    };

    // 1. Check manifest icon path
    if let Some(icon) = icon_path {
        if file_exists(icon) {
            return Some(clean_relative(icon));
        }
    }

    // 2. Fallback: Check haextension/favicon.ico
    let haextension_favicon = format!("{haextension_dir}/favicon.ico");
    if file_exists(&haextension_favicon) {
        return Some(clean_relative(&haextension_favicon));
    }

    // 3. Fallback: Check favicon.ico in root
    if file_exists("favicon.ico") {
        return Some("favicon.ico".to_string());
    }

    None
}
