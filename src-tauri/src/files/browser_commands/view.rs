//! The viewer, thumbnails and opening with the system's app (spec 044 US1, US2).

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, State};
use ts_rs::TS;

use super::{blocking, device_path, storage_of, window_storage};
use crate::files::access::Want;
use crate::files::encrypted::ops::{file_at, later};
use crate::files::encrypted::redacted::Redacted;
use crate::files::encrypted::route::{plain_by_memory, route, Located, Route};
use crate::files::encrypted::source::{EncryptedFile, EncryptedFileSource};
use crate::files::kind::{viewer_kind, ViewerKind};
use crate::files::local::ops;
use crate::files::local::text::{read_text, text_from, TextContent, TEXT_LIMIT};
use crate::files::media::MediaServer;
use crate::files::state::FilesState;
use crate::files::storage_source::StorageFiles;
use crate::files::storage_source::{object_key, storage_error};
use crate::files::streaming::{LocalFileSource, StorageFileSource, StreamingSource};
use crate::files::{thumbnails, Entry, EntryKind, FilesError, FilesErrorCode, SourceRef};
use crate::state::AppState;

/// The text of a file for the viewer, at most 5 MB (FR-015).
#[tauri::command]
pub async fn files_read_text(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
) -> Result<TextContent, FilesError> {
    if let Some(id) = storage_of(&source) {
        let storage = window_storage(&app, &state, &files, id).await?;
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
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    media: State<'_, MediaServer>,
    source: SourceRef,
    path: String,
    tab_id: String,
) -> Result<Opened, FilesError> {
    if let Some(id) = storage_of(&source) {
        let storage = window_storage(&app, &state, &files, id).await?;
        let entry = storage.stat(&path).await?;
        if entry.kind != EntryKind::File {
            return Err(FilesError::invalid_path("not a file"));
        }
        let kind = viewer_kind(&entry.name);
        let mime = entry
            .mime
            .clone()
            .unwrap_or_else(|| "application/octet-stream".to_owned());
        let source: Option<Arc<dyn StreamingSource>> = match route(&storage, &path).await? {
            // Video and audio from encrypted folders come with spec 048 US4.
            Route::Encrypted(at) if matches!(kind, ViewerKind::Image | ViewerKind::Pdf) => {
                let (folder, version) = file_at(&storage, &at).await?;
                let file = EncryptedFile::open(&storage, folder, &version).await?;
                Some(Arc::new(EncryptedFileSource::new(file, mime)))
            }
            Route::Encrypted(at) => {
                file_at(&storage, &at).await?;
                None
            }
            Route::Plain if streams(kind) => Some(Arc::new(StorageFileSource::new(
                storage.store().clone(),
                storage.access().clone(),
                object_key(&path)?,
                entry.size.unwrap_or(0),
                mime,
            ))),
            Route::Plain => None,
        };
        let url = source.map(|source| media.register(&tab_id, source));
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
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    source: SourceRef,
    path: String,
    size: u64,
    modified_ms: i64,
) -> Result<Response, FilesError> {
    if let Some(id) = storage_of(&source) {
        let open = || window_storage(&app, &state, &files, id);
        return storage_thumbnail(&files, id, path, size, modified_ms, open)
            .await
            .map(Response::new);
    }
    let real = device_path(&files, &source, &path, Want::Read)?;
    let cache = files.thumbnails.clone();
    let bytes =
        blocking(move || thumbnails::thumbnail(&cache, "device", &real, size, modified_ms)).await?;
    Ok(Response::new(bytes))
}

/// The thumbnail of `path` in the storage `id`: an image in an encrypted folder is made in memory
/// and kept in memory only (spec 048 FR-026); any other is downloaded beside the disk cache as in
/// spec 044. `open` opens the storage when a thumbnail has to be made.
pub(crate) async fn storage_thumbnail<F, Fut>(
    files: &FilesState,
    id: &str,
    path: String,
    size: u64,
    modified_ms: i64,
    open: F,
) -> Result<Vec<u8>, FilesError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<StorageFiles, FilesError>>,
{
    let cache = files.thumbnails.clone();
    let label = format!("storage:{id}");
    let key = thumbnails::cache_key(&label, &path, size, modified_ms);
    if let Some(hit) = files.memory_thumbnails.get(&key) {
        return hit;
    }
    // The disk cache only for a path known to be outside encrypted folders (FR-026).
    let plain = plain_by_memory(&files.encrypted, id, &path);
    if plain {
        if let Some(hit) = thumbnails::cached(&cache, &label, &path, size, modified_ms) {
            return hit;
        }
    }
    if size > STORAGE_THUMBNAIL_LIMIT {
        return Err(FilesError::new(
            FilesErrorCode::TooLarge,
            "too large for a thumbnail",
        ));
    }
    let storage = open().await?;
    if let Route::Encrypted(at) = route(&storage, &path).await? {
        let made = memory_thumbnail(&storage, &at).await;
        // A made thumbnail or an image that does not decode; never a damaged or unreachable file.
        if made
            .as_ref()
            .map_or_else(|e| e.code == FilesErrorCode::Broken, |_| true)
        {
            files.memory_thumbnails.put(key, &made);
        }
        return made;
    }
    if !plain {
        if let Some(hit) = thumbnails::cached(&cache, &label, &path, size, modified_ms) {
            return hit;
        }
    }
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
        let made = thumbnails::render_into_cache(&cache, &label, &path, size, modified_ms, &temp);
        let _ = std::fs::remove_file(&temp);
        made
    })
    .await?;
    Ok(bytes)
}

/// The thumbnail of an image in an encrypted folder, made in memory: nothing under the thumbnail
/// cache, no download file (spec 048 FR-026).
async fn memory_thumbnail(storage: &StorageFiles, at: &Located) -> Result<Vec<u8>, FilesError> {
    let (folder, version) = file_at(storage, at).await?;
    let file = EncryptedFile::open(storage, folder, &version).await?;
    if file.size() > STORAGE_THUMBNAIL_LIMIT {
        return Err(FilesError::new(
            FilesErrorCode::TooLarge,
            "too large for a thumbnail",
        ));
    }
    let mut bytes = Vec::new();
    let mut at = 0;
    while at < file.size() {
        let end = file
            .size()
            .min(at + crate::files::encrypted::source::WINDOW);
        bytes.extend(file.read(at, end).await?);
        at = end;
    }
    blocking(move || thumbnails::render_bytes(&bytes)).await
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
        Some(id) => download_to_open(&app, &state, &files, id, &path).await?,
        None => device_path(&files, &source, &path, Want::Read)?,
    };
    #[cfg(desktop)]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_path(real.to_string_lossy(), None::<&str>)
            .map_err(|error| {
                // The opener's error can quote the path (FR-027).
                let _ = error;
                log::warn!(
                    "files: the system could not open {}",
                    Redacted(&real.to_string_lossy())
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
    app: &AppHandle,
    state: &State<'_, AppState>,
    files: &FilesState,
    storage_id: &str,
    path: &str,
) -> Result<PathBuf, FilesError> {
    let storage = window_storage(app, state, files, storage_id).await?;
    // Encrypted folders need the notice and their own folder first (spec 048 US5).
    if let Route::Encrypted(_) = route(&storage, path).await? {
        return Err(later());
    }
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
