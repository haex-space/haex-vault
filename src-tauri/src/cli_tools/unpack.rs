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
use std::io::{self, BufReader, Read};
use std::path::{Component, Path};

use flate2::read::GzDecoder;
use tar::{Archive, EntryType};
use tokio_util::sync::CancellationToken;

/// What a total may exceed the listed unpacked size by (file system and header rounding).
const SLACK_BYTES: u64 = 1024 * 1024;
/// Bounds tar headers and metadata that do not contribute to extracted file sizes.
const ARCHIVE_OVERHEAD_BYTES: u64 = 16 * 1024 * 1024;

struct BoundedReader<R> {
    inner: R,
    remaining: u64,
    cancel: CancellationToken,
}

impl<R: Read> Read for BoundedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.cancel.is_cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        if self.remaining == 0 {
            let mut byte = [0];
            return match self.inner.read(&mut byte)? {
                0 => Ok(0),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "archive exceeds input limit",
                )),
            };
        }
        let remaining = usize::try_from(self.remaining).unwrap_or(usize::MAX);
        let allowed = buffer.len().min(remaining);
        let read = self.inner.read(&mut buffer[..allowed])?;
        self.remaining -= read as u64;
        Ok(read)
    }
}

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
    let cap = limit.saturating_add(SLACK_BYTES);
    let bounded = BoundedReader {
        inner: GzDecoder::new(BufReader::new(file)),
        remaining: cap.saturating_add(ARCHIVE_OVERHEAD_BYTES),
        cancel: cancel.clone(),
    };
    let mut package = Archive::new(bounded);
    package.set_preserve_permissions(true);
    package.set_unpack_xattrs(false);
    package.set_preserve_mtime(false);

    let mut written: u64 = 0;
    let entries = match package.entries() {
        Ok(entries) => entries,
        Err(_) if cancel.is_cancelled() => return Err(UnpackError::Cancelled),
        Err(error) => return Err(io("read the archive", error)),
    };
    for entry in entries {
        if cancel.is_cancelled() {
            return Err(UnpackError::Cancelled);
        }
        let mut entry = match entry {
            Ok(entry) => entry,
            Err(_) if cancel.is_cancelled() => return Err(UnpackError::Cancelled),
            Err(error) => return Err(io("read an entry", error)),
        };
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
