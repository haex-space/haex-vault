//! One file as a place (spec 074, research R1 and R5): a file rule hands the engine two places that each hold
//! one entry. [`OneFilePlace`] wraps the folder a file lies in and shows only the file of that name, so
//! planning, copying, conflicts, the per-file state and the run log work on it as on any folder.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::ignore::IGNORE_FILE;
use super::model::{SyncRule, SyncRuleSide};
use super::place::{unreachable, Facts, Listing, Place, Trashed};
use super::scan::ScanError;
use crate::files::transfer::remote::Side;
use crate::files::{FilesError, FilesErrorCode, SourceRef};

/// The folder of a place and the name of the one file in it that the rule is about.
pub struct OneFilePlace {
    inner: Box<dyn Place>,
    name: String,
}

impl OneFilePlace {
    pub fn new(inner: Box<dyn Place>, name: impl Into<String>) -> Self {
        Self {
            inner,
            name: name.into(),
        }
    }
}

#[async_trait]
impl Place for OneFilePlace {
    /// The file alone, or nothing when there is none. A folder that is gone is an error, not an empty
    /// listing, so it never looks like a deletion.
    async fn list_tree(&self, _cancel: &CancellationToken) -> Result<Listing, FilesError> {
        self.inner.check_root().await?;
        let mut listing = Listing::default();
        if let Some(facts) = self.inner.stat(&self.name).await? {
            listing.files.insert(self.name.clone(), facts);
        }
        Ok(listing)
    }

    async fn check_root(&self) -> Result<(), FilesError> {
        self.inner.check_root().await
    }

    async fn stat(&self, path: &str) -> Result<Option<Facts>, FilesError> {
        self.inner.stat(path).await
    }

    async fn hash(&self, path: &str, cancel: &CancellationToken) -> Result<String, FilesError> {
        self.inner.hash(path, cancel).await
    }

    async fn create_folder(&self, path: &str) -> Result<(), FilesError> {
        self.inner.create_folder(path).await
    }

    /// A rename of a file rule is a file moving aside as a conflict copy, never an overwrite. The listing of
    /// this place shows the one file only, so the planner cannot see a copy of that name that lies beside
    /// it (a second conflict on the same day); the name is checked here, and nothing is overwritten.
    async fn rename(&self, from: &str, to: &str) -> Result<(), FilesError> {
        if from != to && self.inner.stat(to).await?.is_some() {
            return Err(FilesError::new(
                FilesErrorCode::Exists,
                format!("{to} is taken beside the file of the rule"),
            ));
        }
        self.inner.rename(from, to).await
    }

    async fn delete(&self, path: &str) -> Result<(), FilesError> {
        self.inner.delete(path).await
    }

    async fn trash(&self, path: &str, run: &str) -> Result<Trashed, FilesError> {
        self.inner.trash(path, run).await
    }

    /// The exclusion file does not apply to a file rule (FR-010): it reads as absent.
    async fn read_text(&self, path: &str, max: usize) -> Result<Option<String>, FilesError> {
        if path == IGNORE_FILE {
            return Ok(None);
        }
        self.inner.read_text(path, max).await
    }

    async fn write_text(&self, path: &str, text: &str) -> Result<(), FilesError> {
        self.inner.write_text(path, text).await
    }

    fn implies_folders(&self) -> bool {
        self.inner.implies_folders()
    }

    fn side(&self) -> Side {
        self.inner.side()
    }

    fn full_path(&self, path: &str) -> String {
        self.inner.full_path(path)
    }
}

/// The folder a file lies in and its name, from the file's path as the rule stores it: a device path
/// (separators of the platform) or a window path of a storage (`/Fotos/a.jpg`). `None` for a path
/// without a file name (a root, or one that ends in a separator).
pub fn split_file_path(location: &SourceRef, path: &str) -> Option<(String, String)> {
    match location {
        SourceRef::Device => {
            let path = std::path::Path::new(path);
            let name = path.file_name()?.to_str()?.to_owned();
            let parent = path.parent()?.to_string_lossy().into_owned();
            Some((parent, name))
        }
        SourceRef::Storage { .. } => {
            let (parent, name) = path.rsplit_once('/')?;
            if name.is_empty() {
                return None;
            }
            let parent = if parent.is_empty() { "/" } else { parent };
            Some((parent.to_owned(), name.to_owned()))
        }
    }
}

/// Why no file is at the source path of a file rule; the window translates the key (`noFile`,
/// `folderGone`). A folder at the path counts as `noFile`: a place answers only for files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidReason {
    /// The folder is there, and no file of that name is in it.
    NoFile,
    /// A folder above the file is missing: renamed, moved or deleted.
    FolderGone,
}

impl InvalidReason {
    pub fn key(self) -> &'static str {
        match self {
            Self::NoFile => "noFile",
            Self::FolderGone => "folderGone",
        }
    }
}

/// What a file rule's source looks like at a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceCheck {
    Valid,
    Invalid(InvalidReason),
}

/// The directory where an external medium appears for `path`: `/media/<user>/<label>`,
/// `/run/media/<user>/<label>`, `/mnt/<name>`, `/Volumes/<name>`, `/storage/<id>`, or a Windows drive
/// (`X:\`). `None` for a path on the system's own disk. When it is missing, the medium is not there
/// (research R3). Works on the text, so it is the same on every platform.
pub fn mount_root(path: &str) -> Option<PathBuf> {
    let bytes = path.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        return Some(PathBuf::from(&path[..3]));
    }
    if !path.starts_with('/') {
        return None;
    }
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let depth = match parts.as_slice() {
        ["media", _, _, ..] => 3,
        ["run", "media", _, _, ..] => 4,
        ["mnt", _, ..] | ["Volumes", _, ..] | ["storage", _, ..] => 2,
        _ => return None,
    };
    Some(PathBuf::from(format!("/{}", parts[..depth].join("/"))))
}

/// Whether the source file of a file rule is there (spec 074 research R3). `place` is the source side
/// as a run opens it, `name` the file's name, `device_path` its path on this device (for the mount root)
/// and `root_exists` tells whether a directory is there. A place that cannot be reached is an error,
/// never an invalid source: the rule waits.
pub async fn source_check(
    place: &dyn Place,
    name: &str,
    device_path: Option<&str>,
    root_exists: &(dyn Fn(&Path) -> bool + Sync),
) -> Result<SourceCheck, FilesError> {
    // A device place whose folder is gone answers `NotFound`: that is "no file", and the root check below
    // tells the folder from the file.
    match place.stat(name).await {
        Ok(Some(_)) => return Ok(SourceCheck::Valid),
        Ok(None) => {}
        Err(error) if error.code == FilesErrorCode::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(root) = device_path.and_then(mount_root) {
        if !root_exists(&root) {
            return Err(unreachable("the drive of the source file is not there"));
        }
    }
    match place.check_root().await {
        Ok(()) => Ok(SourceCheck::Invalid(InvalidReason::NoFile)),
        Err(error) if error.code == FilesErrorCode::Unreachable => {
            Ok(SourceCheck::Invalid(InvalidReason::FolderGone))
        }
        Err(error) => Err(error),
    }
}

/// The check a run and the preview make before they scan a file rule: `Ok` when its source file is
/// there, else the reason the run stops (`ScanError`, so it ends like any scan that stops).
pub async fn check_file_source(rule: &SyncRule, source: &dyn Place) -> Result<(), ScanError> {
    let device_path =
        (rule.source.location == SourceRef::Device).then_some(rule.source.path.as_str());
    let Some((_, name)) = split_file_path(&rule.source.location, &rule.source.path) else {
        return Err(ScanError::InvalidSource {
            reason: InvalidReason::NoFile.key().to_owned(),
        });
    };
    match source_check(source, &name, device_path, &|root| root.exists()).await {
        Ok(SourceCheck::Valid) => Ok(()),
        Ok(SourceCheck::Invalid(reason)) => Err(ScanError::InvalidSource {
            reason: reason.key().to_owned(),
        }),
        Err(error) if error.code == FilesErrorCode::Unreachable => Err(ScanError::Unreachable {
            side: SyncRuleSide::Source,
            reason: error.message,
        }),
        Err(error) => Err(ScanError::Failed {
            side: SyncRuleSide::Source,
            reason: error.message,
        }),
    }
}

#[cfg(test)]
#[path = "place_file_tests.rs"]
mod tests;
