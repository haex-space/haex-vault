//! Placing a downloaded tool into a staging folder (spec 094 FR-005, FR-007).
//!
//! A tool comes as the program itself (`file`) or as a `tar.gz` package. The archive is the
//! vendor's, checked against its SHA-256 before it is opened here, but it is still opened
//! defensively: only regular files and folders are unpacked, an entry that would land outside the
//! target is refused, and so is anything that is a link or a device, because the packages of the
//! supported vendors hold none and a link is how an archive writes outside its folder. The
//! total is capped at the size the catalog lists, so a decompression bomb ends early.
//!
//! The functions block: callers on the async executor run them in `spawn_blocking`.

use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Component, Path};

use flate2::read::GzDecoder;
use tar::{Archive, EntryType};
use tokio_util::sync::CancellationToken;

/// What a total may exceed the listed unpacked size by (file system and header rounding).
const SLACK_BYTES: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UnpackError {
    /// The archive holds something holzi never unpacks.
    #[error("refused: {0}")]
    Refused(String),
    #[error("{0}")]
    Io(String),
    #[error("cancelled")]
    Cancelled,
}

fn io(context: &str, error: impl std::fmt::Display) -> UnpackError {
    UnpackError::Io(format!("{context}: {error}"))
}

/// Unpacks the `tar.gz` at `archive` into `target` (an existing folder) and returns the bytes of
/// regular files written. `limit` is the unpacked size the catalog lists.
pub fn unpack_tar_gz(
    archive: &Path,
    target: &Path,
    limit: u64,
    cancel: &CancellationToken,
) -> Result<u64, UnpackError> {
    let file = File::open(archive).map_err(|e| io("open the archive", e))?;
    let mut package = Archive::new(GzDecoder::new(BufReader::new(file)));
    package.set_preserve_permissions(true);
    package.set_unpack_xattrs(false);
    package.set_preserve_mtime(false);

    let cap = limit.saturating_add(SLACK_BYTES);
    let mut written: u64 = 0;
    let entries = package.entries().map_err(|e| io("read the archive", e))?;
    for entry in entries {
        if cancel.is_cancelled() {
            return Err(UnpackError::Cancelled);
        }
        let mut entry = entry.map_err(|e| io("read an entry", e))?;
        let kind = entry.header().entry_type();
        match kind {
            EntryType::Regular | EntryType::Continuous | EntryType::Directory => {}
            // A pax global header describes the archive; it has no file to write.
            EntryType::XGlobalHeader => continue,
            other => {
                return Err(UnpackError::Refused(format!(
                    "an entry of kind {other:?} (links and devices are not unpacked)"
                )))
            }
        }
        let path = entry.path().map_err(|e| io("read an entry path", e))?;
        if path
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
        {
            return Err(UnpackError::Refused(
                "an entry path that leaves the target".into(),
            ));
        }
        drop(path);
        written = written.saturating_add(entry.size());
        if written > cap {
            return Err(UnpackError::Refused(
                "the archive is larger than the catalog lists".into(),
            ));
        }
        let inside = entry
            .unpack_in(target)
            .map_err(|e| io("unpack an entry", e))?;
        if !inside {
            return Err(UnpackError::Refused(
                "an entry path that leaves the target".into(),
            ));
        }
    }
    Ok(written)
}

/// Moves the downloaded program `downloaded` to `target` and makes it executable where the
/// platform has an executable bit.
pub fn place_file(downloaded: &Path, target: &Path) -> Result<(), UnpackError> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| io("create the program folder", e))?;
    }
    if downloaded != target {
        fs::rename(downloaded, target).map_err(|e| io("place the program", e))?;
    }
    make_executable(target)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), UnpackError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|e| io("make the program executable", e))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), UnpackError> {
    Ok(())
}

#[cfg(test)]
#[path = "unpack_tests.rs"]
mod tests;
