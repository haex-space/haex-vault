//! The viewer, thumbnails and opening with the system's app (spec 044 US1, US2).

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, State};
use ts_rs::TS;

use super::{blocking, device_path, storage_files, storage_of};
use crate::files::access::Want;
use crate::files::kind::{viewer_kind, ViewerKind};
use crate::files::local::ops;
use crate::files::local::text::{read_text, text_from, TextContent, TEXT_LIMIT};
use crate::files::media::MediaServer;
use crate::files::state::FilesState;
use crate::files::storage_source::{object_key, storage_error};
use crate::files::streaming::{LocalFileSource, StorageFileSource};
use crate::files::{thumbnails, Entry, EntryKind, FilesError, FilesErrorCode, SourceRef};
use crate::state::AppState;

/// The text of a file for the viewer, at most 5 MB (FR-015).
#[tauri::command]
pub async fn files_read_text(
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
) -> Result<TextContent, FilesError> {
    if let Some(id) = storage_of(&source) {
        let storage = storage_files(&state, id).await?;
        let entry = storage.stat(&path).await?;
        if entry.kind != EntryKind::File {
            return Err(FilesError::invalid_path("not a file"));
        }
        let bytes = storage
            .read_start(&path, entry.size.unwrap_or(0), TEXT_LIMIT)
            .await?;
        return text_from(bytes, TEXT_LIMIT);
    }
    let real = device_path(&files, &source, &path, Want::Read)?;
    blocking(move || read_text(&real, TEXT_LIMIT)).await
}

/// What the viewer gets for a file (contracts/tauri-commands.md, `files_open`).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct Opened {
    pub kind: ViewerKind,
    /// The media server's URL for image, video, audio and PDF; released with the tab.
    pub url: Option<String>,
    pub entry: Entry,
}

/// Opens a file for the viewer of `tab_id`: images, video, audio and PDF get a URL of the media
/// server (FR-012, FR-016), text is read with [`files_read_text`], the rest is the info view.
#[tauri::command]
pub async fn files_open(
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    media: State<'_, MediaServer>,
    source: SourceRef,
    path: String,
    tab_id: String,
) -> Result<Opened, FilesError> {
    if let Some(id) = storage_of(&source) {
        let storage = storage_files(&state, id).await?;
        let entry = storage.stat(&path).await?;
        if entry.kind != EntryKind::File {
            return Err(FilesError::invalid_path("not a file"));
        }
        let kind = viewer_kind(&entry.name);
        let url = if streams(kind) {
            let mime = entry
                .mime
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_owned());
            let file = StorageFileSource::new(
                storage.store().clone(),
                storage.access().clone(),
                object_key(&path)?,
                entry.size.unwrap_or(0),
                mime,
            );
            Some(media.register(&tab_id, Arc::new(file)))
        } else {
            None
        };
        return Ok(Opened { kind, url, entry });
    }
    let real = device_path(&files, &source, &path, Want::Read)?;
    let own = files.own.clone();
    let stat_path = real.clone();
    let entry = blocking(move || ops::stat(&stat_path, &own)).await?;
    if entry.kind != EntryKind::File {
        return Err(FilesError::invalid_path("not a file"));
    }
    let kind = viewer_kind(&entry.name);
    let url = streams(kind).then(|| {
        let mime = entry
            .mime
            .clone()
            .unwrap_or_else(|| "application/octet-stream".to_owned());
        media.register(&tab_id, Arc::new(LocalFileSource::new(real, mime)))
    });
    Ok(Opened { kind, url, entry })
}

/// Whether the viewer shows `kind` from the media server.
fn streams(kind: ViewerKind) -> bool {
    matches!(
        kind,
        ViewerKind::Image | ViewerKind::Video | ViewerKind::Audio | ViewerKind::Pdf
    )
}

/// The largest image of a storage holzi downloads for a thumbnail.
const STORAGE_THUMBNAIL_LIMIT: u64 = 50 * 1024 * 1024;

/// Ends one URL of [`files_open`].
#[tauri::command]
pub fn files_release(media: State<'_, MediaServer>, url: String) {
    media.release(&url);
}

/// Ends every URL of a tab; the window calls it when the tab's file browser goes away.
#[tauri::command]
pub fn files_release_tab(media: State<'_, MediaServer>, tab_id: String) {
    media.release_tab(&tab_id);
}

/// The JPEG thumbnail of an image (FR-005).
#[tauri::command]
pub async fn files_thumbnail(
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
    size: u64,
    modified_ms: i64,
) -> Result<Response, FilesError> {
    if let Some(id) = storage_of(&source) {
        let cache = files.thumbnails.clone();
        let label = format!("storage:{id}");
        if let Some(hit) = thumbnails::cached(&cache, &label, &path, size, modified_ms) {
            return hit.map(Response::new);
        }
        if size > STORAGE_THUMBNAIL_LIMIT {
            return Err(FilesError::new(
                FilesErrorCode::TooLarge,
                "too large for a thumbnail",
            ));
        }
        let storage = storage_files(&state, id).await?;
        let key = object_key(&path)?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut reader = storage
            .store()
            .get_range(storage.access(), &key, 0, size, deadline)
            .await
            .map_err(storage_error)?;
        let temp = cache.join(format!(".download-{}", uuid::Uuid::new_v4()));
        let downloaded = async {
            tokio::fs::create_dir_all(&cache).await?;
            let mut file = tokio::fs::File::create(&temp).await?;
            tokio::io::copy(&mut reader, &mut file).await?;
            Ok::<(), std::io::Error>(())
        }
        .await;
        if downloaded.is_err() {
            let _ = tokio::fs::remove_file(&temp).await;
            return Err(storage_error(crate::remote_storage::StorageError::Network));
        }
        let bytes = blocking(move || {
            let made =
                thumbnails::render_into_cache(&cache, &label, &path, size, modified_ms, &temp);
            let _ = std::fs::remove_file(&temp);
            made
        })
        .await?;
        return Ok(Response::new(bytes));
    }
    let real = device_path(&files, &source, &path, Want::Read)?;
    let cache = files.thumbnails.clone();
    let bytes =
        blocking(move || thumbnails::thumbnail(&cache, "device", &real, size, modified_ms)).await?;
    Ok(Response::new(bytes))
}

/// Opens a file with the system's app for it (FR-014).
#[tauri::command]
pub async fn files_open_system(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
) -> Result<(), FilesError> {
    let real = match storage_of(&source) {
        Some(id) => download_to_open(&state, &files, id, &path).await?,
        None => device_path(&files, &source, &path, Want::Read)?,
    };
    #[cfg(desktop)]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_path(real.to_string_lossy(), None::<&str>)
            .map_err(|error| {
                log::warn!(
                    "files: the system could not open {}: {error}",
                    real.display()
                );
                FilesError::new(FilesErrorCode::Unsupported, "no app for this file")
            })
    }
    #[cfg(mobile)]
    {
        let _ = (app, real);
        Err(FilesError::new(
            FilesErrorCode::Unsupported,
            "not on this platform yet",
        ))
    }
}

/// A file of a storage, downloaded into holzi's cache so the system's app can open it (FR-014;
/// the copy stays until the cache is cleared).
async fn download_to_open(
    state: &State<'_, AppState>,
    files: &FilesState,
    storage_id: &str,
    path: &str,
) -> Result<PathBuf, FilesError> {
    let storage = storage_files(state, storage_id).await?;
    let entry = storage.stat(path).await?;
    if entry.kind != EntryKind::File {
        return Err(FilesError::invalid_path("not a file"));
    }
    let folder = files
        .thumbnails
        .parent()
        .unwrap_or(&files.thumbnails)
        .join("files-opened")
        .join(uuid::Uuid::new_v4().to_string());
    let target = folder.join(&entry.name);
    let mut reader = storage
        .store()
        .get_range(
            storage.access(),
            &object_key(path)?,
            0,
            entry.size.unwrap_or(0),
            tokio::time::Instant::now() + std::time::Duration::from_secs(60),
        )
        .await
        .map_err(storage_error)?;
    let written = async {
        tokio::fs::create_dir_all(&folder).await?;
        let mut file = tokio::fs::File::create(&target).await?;
        tokio::io::copy(&mut reader, &mut file).await?;
        Ok::<(), std::io::Error>(())
    }
    .await;
    if written.is_err() {
        let _ = tokio::fs::remove_dir_all(&folder).await;
        return Err(storage_error(crate::remote_storage::StorageError::Network));
    }
    Ok(target)
}
