//! A place of a rule as the engine sees it (research R1): a folder of the device or of a storage
//! (also another own device or an encrypted folder), with only what a run needs. Paths are relative
//! to the place, names joined by `/`, without a leading `/`. Copying goes through
//! [`crate::files::transfer::sync_copy`], not through this layer.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::files::transfer::remote::Side;
use crate::files::{FilesError, FilesErrorCode};

/// The folder in a place that mirrored deletions go to where there is no system trash.
pub const RULE_TRASH: &str = ".holzi-trash";

/// What a place says about one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub size: u64,
    /// Milliseconds since 1970; 0 when the place does not say.
    pub mtime_ms: i64,
    /// SHA-256 of the content (hex) when the place has it without reading the file (the sidecar
    /// of an encrypted folder).
    pub hash: Option<String>,
}

/// Why an entry of a place is left out of a run (FR-018).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// A symbolic link or shortcut; never followed.
    Link,
    /// The system denies reading it.
    NoAccess,
    /// The name is not valid text.
    BadName,
}

/// Every file and folder below the root of a place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    pub files: BTreeMap<String, Facts>,
    pub folders: BTreeSet<String>,
    pub skipped: Vec<(String, SkipReason)>,
}

/// Where a mirrored deletion went (FR-046).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trashed {
    /// The system's trash; holzi cannot open it.
    System,
    /// The rule trash of the place, at this relative path.
    Rule(String),
}

/// A place of a rule.
#[async_trait]
pub trait Place: Send + Sync {
    /// Every file and folder below the root. A root that is missing or cannot be read is
    /// [`FilesErrorCode::Unreachable`]; a folder below it that cannot be listed fails the whole
    /// listing, since leaving it out would look like deletions (FR-022).
    async fn list_tree(&self, cancel: &CancellationToken) -> Result<Listing, FilesError>;

    /// The facts of one file; `None` when there is none.
    async fn stat(&self, path: &str) -> Result<Option<Facts>, FilesError>;

    /// SHA-256 of the content (hex), read in parts.
    async fn hash(&self, path: &str, cancel: &CancellationToken) -> Result<String, FilesError>;

    /// Creates the folder `path` and the folders above it that are missing.
    async fn create_folder(&self, path: &str) -> Result<(), FilesError>;

    /// Moves a file within the place, creating the folders above `to`.
    async fn rename(&self, from: &str, to: &str) -> Result<(), FilesError>;

    /// Deletes a file for good.
    async fn delete(&self, path: &str) -> Result<(), FilesError>;

    /// Moves a file into the system's trash, or into the rule trash below `run`.
    async fn trash(&self, path: &str, run: &str) -> Result<Trashed, FilesError>;

    /// A small text file; `None` when there is none, [`FilesErrorCode::TooLarge`] when it is longer
    /// than `max` bytes.
    async fn read_text(&self, path: &str, max: usize) -> Result<Option<String>, FilesError>;

    /// Writes a small text file in place of what is there, never leaving a half file.
    async fn write_text(&self, path: &str, text: &str) -> Result<(), FilesError>;

    /// Whether a folder exists as long as a file lies in it, so a folder with a file going in needs
    /// no request of its own (the prefixes of an object storage).
    fn implies_folders(&self) -> bool {
        false
    }

    /// The side copies of this place go through.
    fn side(&self) -> Side;

    /// `path` as the side names it: a device path or a window path.
    fn full_path(&self, path: &str) -> String;
}

/// The folder of `path` (empty for the root) and its name.
pub fn split(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

/// The path of the rule trash for `path` in the run `run`.
pub fn trash_path(run: &str, path: &str) -> String {
    format!("{RULE_TRASH}/{run}/{path}")
}

/// The error for a place whose root is gone or unreadable (FR-022).
pub fn unreachable(reason: &str) -> FilesError {
    FilesError::new(FilesErrorCode::Unreachable, reason)
}

#[cfg(test)]
#[path = "place_tests.rs"]
mod tests;
