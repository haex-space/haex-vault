//! Generic filesystem commands
//!
//! These commands provide low-level filesystem access that can be used by
//! extensions and other parts of the application for local file operations.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

mod copy;
mod picker;
mod read;
mod write;

// GLOB re-export so #[tauri::command]'s `__cmd__<name>` companions are
// reachable from the parent `pub use commands::*` in filesystem/mod.rs and
// from `crate::filesystem::filesystem_*` in lib.rs's `generate_handler!`.
pub use copy::*;
pub use picker::*;
pub use read::*;
pub use write::*;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, Error)]
pub enum FsError {
    #[error("File not found: {path}")]
    NotFound { path: String },

    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    #[error("I/O error: {reason}")]
    IoError { reason: String },

    #[error("Invalid path: {reason}")]
    InvalidPath { reason: String },

    #[error("Not a directory: {path}")]
    NotADirectory { path: String },

    #[error("Not a file: {path}")]
    NotAFile { path: String },

    #[allow(dead_code)]
    #[error("Dialog cancelled by user")]
    DialogCancelled,
}

impl From<std::io::Error> for FsError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => FsError::NotFound {
                path: "unknown".to_string(),
            },
            std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied {
                path: "unknown".to_string(),
            },
            _ => FsError::IoError {
                reason: e.to_string(),
            },
        }
    }
}

impl Serialize for FsError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ============================================================================
// Types
// ============================================================================

/// File/directory metadata
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    /// File size in bytes
    pub size: u64,
    /// True if this is a file
    pub is_file: bool,
    /// True if this is a directory
    pub is_directory: bool,
    /// True if this is a symbolic link
    pub is_symlink: bool,
    /// Last modified time (Unix timestamp in milliseconds)
    pub modified: Option<u64>,
    /// Created time (Unix timestamp in milliseconds)
    pub created: Option<u64>,
    /// Whether the file is read-only
    pub readonly: bool,
}

/// Paginated directory listing
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    pub entries: Vec<DirEntry>,
    pub total: usize,
}

/// Directory entry
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    /// Entry name (not full path)
    pub name: String,
    /// Full path
    pub path: String,
    /// True if this is a file
    pub is_file: bool,
    /// True if this is a directory
    pub is_directory: bool,
    /// File size in bytes (0 for directories)
    pub size: u64,
    /// Last modified time (Unix timestamp in milliseconds)
    pub modified: Option<u64>,
}

// ============================================================================
// E2E test escape hatch
// ============================================================================
//
// WebDriver / Playwright cannot drive the OS-native folder/file picker that
// `filesystem_select_folder` / `_file` open — the dialog is rendered by the
// host OS, not the WebView. To keep e2e tests on the real reactive UI path
// (without the SQL/Tauri-command arrange shortcuts that bypass state stores
// and have caused test flakes), the test harness writes the path it wants
// returned into a sentinel file before clicking the "Browse" button. The
// commands below read that file at dialog-open time instead of opening a
// dialog.
//
// File location defaults to /tmp/haex-e2e-pick-{folder,file}.txt; override
// with HAEX_E2E_PICK_FOLDER_FILE / HAEX_E2E_PICK_FILE_FILE at vault spawn.
//
// Gated at runtime by HAEX_E2E_TEST_MODE=1, read each call. Earlier
// `#[cfg(debug_assertions)]` gating compiled the override out of release
// builds entirely — and the e2e Docker rig runs `cargo tauri build
// --no-bundle` (release), so the override was missing where it was needed.
// Production binaries don't set HAEX_E2E_TEST_MODE, so the override stays
// dormant there even though the code path is present.

pub(super) fn e2e_test_mode_enabled() -> bool {
    matches!(
        std::env::var("HAEX_E2E_TEST_MODE").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

/// Resolve the sentinel-file path for an e2e picker override, returning
/// `Some(path)` only if HAEX_E2E_TEST_MODE is enabled AND the file exists
/// right now (so a stale env var alone never suppresses real dialogs, and
/// a planted file alone can't hijack a production user's next folder pick).
pub(super) fn e2e_pick_override_path(env_var: &str, default_suffix: &str) -> Option<String> {
    if !e2e_test_mode_enabled() {
        return None;
    }
    let path = std::env::var(env_var)
        .unwrap_or_else(|_| format!("/tmp/haex-e2e-pick-{}.txt", default_suffix));
    if std::path::Path::new(&path).exists() {
        Some(path)
    } else {
        None
    }
}
