//! A file the person chose in the system's dialog (spec 043 FR-015, contract `picked-file.md`,
//! research R3). On a desktop it is a path, on Android a `content://` address of a document
//! provider (local storage, downloads, a cloud). Both open through the file plugin, so every flow
//! reads and writes a chosen file the same way and none derives a path from it.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};
use tauri_plugin_holzi_android::HolziAndroidExt;
use ts_rs::TS;

use crate::error::{HaexError, Result};

/// What `open` and `save` of `@tauri-apps/plugin-dialog` return, passed on unchanged (a newtype,
/// so it travels as the bare string).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct PickedFile(pub String);

impl From<&str> for PickedFile {
    fn from(value: &str) -> Self {
        PickedFile(value.to_string())
    }
}

/// How a chosen file is reached: the file plugin in the app, plain paths in tests.
pub trait Opener {
    /// Opens for reading, or for writing from the start (created if missing).
    fn open(&self, file: FilePath, write: bool) -> io::Result<File>;
    /// The folders a chosen *path* may lie in; `None` where any path is fine.
    fn path_roots(&self) -> Option<Vec<PathBuf>>;
    /// The name the document provider shows for an address.
    fn provider_name(&self, uri: &str) -> Option<String>;
}

impl<R: Runtime> Opener for AppHandle<R> {
    fn open(&self, file: FilePath, write: bool) -> io::Result<File> {
        let mut options = OpenOptions::new();
        if write {
            options.read(false).write(true).create(true).truncate(true);
        } else {
            options.read(true);
        }
        self.fs().open(file, options)
    }

    /// Without free paths (FR-016) a path must lie in the app's own storage: its private folders
    /// or its own download folder (on Android in the shared storage, closed to other apps since
    /// Android 11). The e2e suite puts its test files in the download folder, everything else
    /// arrives as an address from the dialog.
    fn path_roots(&self) -> Option<Vec<PathBuf>> {
        if crate::platform::capabilities().free_paths {
            return None;
        }
        let path = self.path();
        Some(
            [
                path.app_data_dir(),
                path.app_local_data_dir(),
                path.app_cache_dir(),
                path.download_dir(),
            ]
            .into_iter()
            .flatten()
            .collect(),
        )
    }

    fn provider_name(&self, uri: &str) -> Option<String> {
        self.holzi_android().display_name(uri)
    }
}

/// Whether `path` lies inside one of `roots`, after resolving symbolic links. A path with `..`
/// or a relative one is refused before anything is resolved.
pub fn inside(path: &Path, roots: &[PathBuf]) -> bool {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return false;
    }
    let Some(resolved) = resolve_existing(path) else {
        return false;
    };
    roots.iter().any(|root| {
        root.canonicalize()
            .is_ok_and(|root| resolved.starts_with(root))
    })
}

/// The real place of `path`; for a file that does not exist yet (a save), that of its folder.
fn resolve_existing(path: &Path) -> Option<PathBuf> {
    if let Ok(resolved) = path.canonicalize() {
        return Some(resolved);
    }
    let parent = path.parent()?.canonicalize().ok()?;
    Some(parent.join(path.file_name()?))
}

/// The chosen file as the file plugin takes it. Only provider addresses (`content:`) and paths
/// are accepted; a `file:` address counts as its path, so it cannot pass the path check.
pub fn resolve(opener: &(impl Opener + ?Sized), file: &PickedFile) -> Result<FilePath> {
    let parsed = match FilePath::from_str(&file.0) {
        Ok(parsed) => parsed,
        Err(never) => match never {},
    };
    let parsed = match parsed {
        FilePath::Url(url) if url.scheme() == "file" => {
            FilePath::Path(url.to_file_path().map_err(|()| refused())?)
        }
        FilePath::Url(url) if url.scheme() == "content" => FilePath::Url(url),
        FilePath::Url(_) => return Err(refused()),
        FilePath::Path(path) => FilePath::Path(path),
    };
    if let (FilePath::Path(path), Some(roots)) = (&parsed, opener.path_roots()) {
        if !inside(path, &roots) {
            return Err(refused());
        }
    }
    Ok(parsed)
}

fn refused() -> HaexError {
    HaexError::InvalidInput {
        reason: "a file must be chosen in the system's file dialog".to_string(),
    }
}

/// An error while reading the chosen file: the person learns that it gives nothing back; the
/// cause goes to the log.
fn unreadable(error: io::Error) -> HaexError {
    log::warn!("chosen file: reading failed: {error}");
    HaexError::Unreadable
}

/// An error while writing: a full device is named, anything else is an I/O error.
fn write_failed(error: io::Error) -> HaexError {
    if error.kind() == io::ErrorKind::StorageFull {
        HaexError::NotEnoughSpace
    } else {
        HaexError::from(error)
    }
}

/// Opens the chosen file for reading; large files (a vault, a model) are read from this.
pub fn open_read(opener: &(impl Opener + ?Sized), file: &PickedFile) -> Result<File> {
    let path = resolve(opener, file)?;
    opener.open(path, false).map_err(unreadable)
}

/// Reads at most `limit + 1` bytes, so the caller sees whether the file is larger than its limit
/// without reading all of it. The size a provider reports is not trusted.
pub fn read(opener: &(impl Opener + ?Sized), file: &PickedFile, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    open_read(opener, file)?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    Ok(bytes)
}

/// Copies the chosen file to `target` through a temporary file beside it, renamed over `target`
/// at the end; a failure leaves nothing behind and an existing `target` untouched. Returns the
/// bytes copied.
pub fn copy_into(opener: &(impl Opener + ?Sized), file: &PickedFile, target: &Path) -> Result<u64> {
    let mut source = open_read(opener, file)?;
    let folder = target.parent().ok_or_else(|| HaexError::Io {
        reason: "copy target has no folder".to_string(),
    })?;
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut temporary = tempfile::Builder::new()
        .prefix(&format!("{name}."))
        .suffix(".tmp")
        .tempfile_in(folder)
        .map_err(write_failed)?;
    let mut buffer = vec![0u8; 1 << 16];
    let mut copied = 0u64;
    loop {
        let read = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(unreadable(error)),
        };
        temporary.write_all(&buffer[..read]).map_err(write_failed)?;
        copied += read as u64;
    }
    temporary.as_file().sync_all().map_err(write_failed)?;
    temporary
        .persist(target)
        .map_err(|error| write_failed(error.error))?;
    Ok(copied)
}

/// Writes `bytes` to the file chosen in the save dialog, replacing what was there.
pub fn write(opener: &(impl Opener + ?Sized), file: &PickedFile, bytes: &[u8]) -> Result<()> {
    let path = resolve(opener, file)?;
    let mut target = opener.open(path, true).map_err(write_failed)?;
    target.write_all(bytes).map_err(write_failed)?;
    target.sync_all().map_err(write_failed)
}

/// The name to show for the chosen file: its file name, or for an address the name its provider
/// shows (the last part of the address if the provider names none).
pub fn display_name(opener: &(impl Opener + ?Sized), file: &PickedFile) -> String {
    match FilePath::from_str(&file.0) {
        Ok(FilePath::Url(url)) if url.scheme() == "content" => opener
            .provider_name(url.as_str())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| last_segment(&url)),
        Ok(FilePath::Url(url)) => url
            .to_file_path()
            .ok()
            .and_then(|path| file_name(&path))
            .unwrap_or_else(|| last_segment(&url)),
        Ok(FilePath::Path(path)) => file_name(&path).unwrap_or_default(),
        Err(never) => match never {},
    }
}

fn file_name(path: &Path) -> Option<String> {
    Some(path.file_name()?.to_string_lossy().into_owned())
}

/// The last part of an address, decoded: `…/document/primary%3ADownload%2Fa.txt` → `a.txt`.
fn last_segment(url: &url::Url) -> String {
    let segment = url.path_segments().and_then(Iterator::last).unwrap_or("");
    let decoded = percent_encoding::percent_decode_str(segment).decode_utf8_lossy();
    decoded
        .rsplit(['/', ':'])
        .next()
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
#[path = "picked_tests.rs"]
mod tests;
