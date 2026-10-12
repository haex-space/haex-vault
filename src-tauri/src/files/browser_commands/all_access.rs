//! "Zugriff auf alle Dateien" on Android (spec 044 US7, FR-039, research R11). Without it the file
//! browser explains why it needs it and leads into the system settings; it checks again when the
//! window comes back. Every other platform has no such grant and answers granted.

use serde::Serialize;
use tauri::{AppHandle, Runtime};
use tauri_plugin_holzi_android::HolziAndroidExt;
use ts_rs::TS;

use super::blocking;
use crate::files::{FilesError, FilesErrorCode};

/// Where the answer comes from: only Android asks the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum FilesAccessPlatform {
    Android,
    Other,
}

/// Whether holzi may reach every file of the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FilesAllAccess {
    pub granted: bool,
    pub platform: FilesAccessPlatform,
}

const PLATFORM: FilesAccessPlatform = if cfg!(target_os = "android") {
    FilesAccessPlatform::Android
} else {
    FilesAccessPlatform::Other
};

fn answer(granted: Result<bool, String>) -> Result<FilesAllAccess, FilesError> {
    let granted = granted.map_err(|reason| FilesError::new(FilesErrorCode::Unsupported, reason))?;
    Ok(FilesAllAccess {
        granted,
        platform: PLATFORM,
    })
}

/// The state now, without asking.
#[tauri::command]
pub async fn files_all_access<R: Runtime>(app: AppHandle<R>) -> Result<FilesAllAccess, FilesError> {
    blocking(move || answer(app.holzi_android().all_files_access())).await
}

/// Leads the person to the grant: the system setting from Android 11, the system's question
/// before, which this waits for. The window checks again when it comes back.
#[tauri::command]
pub async fn files_request_all_access<R: Runtime>(
    app: AppHandle<R>,
) -> Result<FilesAllAccess, FilesError> {
    blocking(move || answer(app.holzi_android().request_all_files_access())).await
}

#[cfg(test)]
#[path = "all_access_tests.rs"]
mod tests;
