//! A folder of this device as a place (research R1, R10): the file system without following links,
//! the system's trash on desktops and the rule trash elsewhere. Every call runs on a blocking
//! thread.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::place::{split, trash_path, unreachable, Facts, Listing, Place, SkipReason, Trashed};
use crate::files::local::ops::io_error;
use crate::files::transfer::local::CHUNK;
use crate::files::transfer::remote::Side;
use crate::files::{FilesError, FilesErrorCode};

/// A folder of this device; `root` is absolute.
#[derive(Debug, Clone)]
pub struct DevicePlace {
    root: PathBuf,
    /// Mirrored deletions go to the system's trash (desktops); otherwise to the rule trash.
    system_trash: bool,
}

impl DevicePlace {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            system_trash: cfg!(not(any(target_os = "android", target_os = "ios"))),
        }
    }

    /// A place that uses the rule trash also on desktops (tests, which must not fill the user's
    /// trash).
    pub fn with_rule_trash(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            system_trash: false,
        }
    }

    fn at(&self, path: &str) -> PathBuf {
        if path.is_empty() {
            self.root.clone()
        } else {
            self.root.join(path)
        }
    }
}

/// Runs `work` on a blocking thread.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, FilesError> + Send + 'static,
) -> Result<T, FilesError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| FilesError::new(FilesErrorCode::Unsupported, "the work stopped"))?
}

fn facts_of(meta: &std::fs::Metadata) -> Facts {
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_millis()).ok())
        .unwrap_or(0);
    Facts {
        size: meta.len(),
        mtime_ms,
        hash: None,
    }
}

/// The relative path of `path` below `root`, `/` between names; `None` when a name is no text.
fn relative(root: &Path, path: &Path) -> Option<String> {
    let rest = path.strip_prefix(root).ok()?;
    let names: Option<Vec<&str>> = rest.iter().map(|part| part.to_str()).collect();
    Some(names?.join("/"))
}

fn list(root: &Path, cancel: &CancellationToken) -> Result<Listing, FilesError> {
    match std::fs::metadata(root) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Err(unreachable("the place is not a folder")),
        Err(_) => return Err(unreachable("the place's folder is missing or unreadable")),
    }
    std::fs::read_dir(root).map_err(|_| unreachable("the place's folder cannot be read"))?;
    let mut listing = Listing::default();
    for entry in walkdir::WalkDir::new(root).min_depth(1).follow_links(false) {
        if cancel.is_cancelled() {
            return Err(FilesError::new(FilesErrorCode::Unsupported, "cancelled"));
        }
        // A folder that cannot be read would look like its files were deleted, so it fails the
        // listing.
        let entry = entry.map_err(|error| {
            let rel = error
                .path()
                .and_then(|path| relative(root, path))
                .unwrap_or_default();
            FilesError::new(
                FilesErrorCode::NoAccess,
                format!("the folder {rel} of the place cannot be read"),
            )
        })?;
        let Some(rel) = relative(root, entry.path()) else {
            listing.skipped.push((
                entry.path().to_string_lossy().into_owned(),
                SkipReason::BadName,
            ));
            continue;
        };
        let kind = entry.file_type();
        if kind.is_symlink() {
            listing.skipped.push((rel, SkipReason::Link));
        } else if kind.is_dir() {
            listing.folders.insert(rel);
        } else if kind.is_file() {
            match entry.metadata() {
                Ok(meta) => {
                    listing.files.insert(rel, facts_of(&meta));
                }
                Err(_) => listing.skipped.push((rel, SkipReason::NoAccess)),
            }
        }
    }
    Ok(listing)
}

fn hash_file(path: &Path, cancel: &CancellationToken) -> Result<String, FilesError> {
    let mut file = std::fs::File::open(path).map_err(|error| io_error(error, path))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        if cancel.is_cancelled() {
            return Err(FilesError::new(FilesErrorCode::Unsupported, "cancelled"));
        }
        let read = file.read(&mut buf).map_err(|error| io_error(error, path))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Lowercase hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn move_file(from: &Path, to: &Path) -> Result<(), FilesError> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|error| io_error(error, parent))?;
    }
    std::fs::rename(from, to).map_err(|error| io_error(error, from))
}

#[async_trait]
impl Place for DevicePlace {
    async fn list_tree(&self, cancel: &CancellationToken) -> Result<Listing, FilesError> {
        let (root, cancel) = (self.root.clone(), cancel.clone());
        blocking(move || list(&root, &cancel)).await
    }

    async fn stat(&self, path: &str) -> Result<Option<Facts>, FilesError> {
        let at = self.at(path);
        blocking(move || match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.is_file() => Ok(Some(facts_of(&meta))),
            Ok(_) => Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_error(error, &at)),
        })
        .await
    }

    async fn hash(&self, path: &str, cancel: &CancellationToken) -> Result<String, FilesError> {
        let (at, cancel) = (self.at(path), cancel.clone());
        blocking(move || hash_file(&at, &cancel)).await
    }

    async fn create_folder(&self, path: &str) -> Result<(), FilesError> {
        let at = self.at(path);
        blocking(move || std::fs::create_dir_all(&at).map_err(|error| io_error(error, &at))).await
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), FilesError> {
        let (from, to) = (self.at(from), self.at(to));
        blocking(move || move_file(&from, &to)).await
    }

    async fn delete(&self, path: &str) -> Result<(), FilesError> {
        let at = self.at(path);
        blocking(move || std::fs::remove_file(&at).map_err(|error| io_error(error, &at))).await
    }

    async fn trash(&self, path: &str, run: &str) -> Result<Trashed, FilesError> {
        let at = self.at(path);
        let rule = trash_path(run, path);
        let target = self.at(&rule);
        let system_trash = self.system_trash;
        blocking(move || {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            if system_trash && trash::delete(&at).is_ok() {
                return Ok(Trashed::System);
            }
            #[cfg(any(target_os = "android", target_os = "ios"))]
            let _ = system_trash;
            move_file(&at, &target)?;
            Ok(Trashed::Rule(rule))
        })
        .await
    }

    async fn read_text(&self, path: &str, max: usize) -> Result<Option<String>, FilesError> {
        let at = self.at(path);
        blocking(move || {
            let meta = match std::fs::metadata(&at) {
                Ok(meta) => meta,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(io_error(error, &at)),
            };
            if meta.len() > max as u64 {
                return Err(FilesError::new(
                    FilesErrorCode::TooLarge,
                    "the file is too large",
                ));
            }
            let bytes = std::fs::read(&at).map_err(|error| io_error(error, &at))?;
            String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| FilesError::new(FilesErrorCode::Binary, "the file is no text"))
        })
        .await
    }

    async fn write_text(&self, path: &str, text: &str) -> Result<(), FilesError> {
        let at = self.at(path);
        let text = text.to_owned();
        let (folder, name) = split(path);
        let part = self
            .at(folder)
            .join(format!(".{name}.holzi-part-{}", uuid::Uuid::new_v4()));
        blocking(move || {
            std::fs::write(&part, text).map_err(|error| io_error(error, &part))?;
            std::fs::rename(&part, &at).map_err(|error| {
                let _ = std::fs::remove_file(&part);
                io_error(error, &at)
            })
        })
        .await
    }

    fn side(&self) -> Side {
        Side::Device
    }

    fn full_path(&self, path: &str) -> String {
        self.at(path).to_string_lossy().into_owned()
    }
}

#[cfg(test)]
#[path = "place_device_tests.rs"]
mod tests;
