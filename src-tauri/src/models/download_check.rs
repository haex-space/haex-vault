//! Size and free space before a model download (spec 043 FR-028, data-model.md "Bestätigung vor
//! Modelldownloads"). Every download asks this first, on every platform: the confirmation names the
//! size, and when the free space of the models folder is short it warns before anything is loaded.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use ts_rs::TS;

use crate::error::{HolziError, Result};

/// The model a download would load.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DownloadTarget {
    /// A chat model of the catalog.
    Catalog { id: String },
    /// A speech recognition model of its catalog.
    Stt { id: String },
    /// A speech model of the assistant's catalog (spec 066).
    Tts { id: String },
    /// A file whose size its source names (the HuggingFace file picker); absent when it names none.
    #[serde(rename_all = "camelCase")]
    File {
        #[ts(type = "number | null")]
        size_bytes: Option<u64>,
    },
}

/// What the confirmation shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct DownloadCheck {
    /// The size of the download; absent when no source names it.
    #[ts(type = "number | null")]
    pub size_bytes: Option<u64>,
    /// The free space where models are stored; absent when the system does not tell.
    #[ts(type = "number | null")]
    pub free_bytes: Option<u64>,
    /// Whether the download fits with a reserve of a tenth of its size. Without a size or without
    /// the free space there is nothing to warn about.
    pub fits: bool,
}

/// Whether `size` fits into `free` with a reserve of 10 % of `size`.
pub fn fits(size: Option<u64>, free: Option<u64>) -> bool {
    match (size, free) {
        (Some(size), Some(free)) => free >= size.saturating_add(size / 10),
        _ => true,
    }
}

/// The free space of the disk `dir` lies on: the mounted disk with the longest mount point that
/// holds `dir`. On Windows `std::fs::canonicalize` names the path `\\?\C:\…`, which never starts
/// with a mount point `C:\`; `dunce` keeps the plain form.
pub fn free_bytes(dir: &Path) -> Option<u64> {
    let dir = dunce::canonicalize(dir).ok()?;
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|disk| dir.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(sysinfo::Disk::available_space)
}

fn size_of(target: &DownloadTarget) -> Result<Option<u64>> {
    match target {
        DownloadTarget::Catalog { id } => crate::catalog::get(id)
            .map(|entry| Some(entry.approx_size_bytes))
            .ok_or_else(|| HolziError::CatalogEntryNotFound { id: id.clone() }),
        DownloadTarget::Stt { id } => crate::stt::catalog::get(id)?
            .map(|entry| Some(entry.approx_size_bytes))
            .ok_or_else(|| HolziError::CatalogEntryNotFound { id: id.clone() }),
        DownloadTarget::Tts { id } => Ok(Some(crate::tts::catalog::get(id)?.approx_size_bytes)),
        DownloadTarget::File { size_bytes } => Ok(*size_bytes),
    }
}

/// Size, free space and whether it fits, for the confirmation before a download.
#[tauri::command]
pub async fn model_download_check(app: AppHandle, target: DownloadTarget) -> Result<DownloadCheck> {
    let size_bytes = size_of(&target)?;
    let dir = super::paths::models_root(&app)?;
    let free_bytes = tauri::async_runtime::spawn_blocking(move || free_bytes(&dir))
        .await
        .ok()
        .flatten();
    Ok(DownloadCheck {
        size_bytes,
        free_bytes,
        fits: fits(size_bytes, free_bytes),
    })
}

#[cfg(test)]
#[path = "download_check_tests.rs"]
mod download_check_tests;
