//! Copying, moving and deleting on this device (spec 044 FR-018 to FR-024, FR-026). Runs on a
//! blocking thread; [`super::TransferManager`] hands in the cancellation, the conflict question and
//! the progress through [`Hooks`].
//!
//! - A file is written to `.<name>.holzi-part-<uuid>` beside its target and renamed at the end, so a
//!   cancelled or failed transfer never leaves half a file (FR-020). "Replace" moves the old entry
//!   aside first and removes it only once the new one is in place; otherwise it goes back.
//! - A folder onto a folder of the same name merges; only names of files (or a file against a
//!   folder) ask (FR-021).
//! - Copying into the folder the entries are in keeps both without asking; moving there does
//!   nothing.
//! - A move is a rename; across file systems it is a copy, then the source is deleted.
//! - Links are copied, moved and deleted as links, never followed.

use std::ffi::OsStr;
use std::fs::{File, Metadata};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

pub use super::entries::free_name;
use super::entries::{copy_link, entry_name, remove, write_error};
use super::{ConflictChoice, TransferOp, TransferProgress};
use crate::files::local::ops::io_error;
use crate::files::local::OwnPlaces;
use crate::files::{FilesError, FilesErrorCode};

/// The most of a file held in memory at once.
pub const CHUNK: usize = 256 * 1024;

/// What a transfer does, with resolved paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub op: TransferOp,
    /// The entries themselves (links not followed).
    pub sources: Vec<PathBuf>,
    /// The folder they go into; `None` for a delete.
    pub target: Option<PathBuf>,
}

/// Files and bytes of a job, for the progress.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub items: u64,
    pub bytes: u64,
}

/// How a delete removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    /// Into the system's trash (desktops).
    Trash,
    /// For good (Android, after the window asked).
    Permanent,
}

/// How a transfer ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Cancelled,
    Failed(FilesError),
}

/// What the manager hands to [`run`].
pub struct Hooks<'a> {
    pub cancel: &'a CancellationToken,
    /// Asks about a name at the target; `None` when the transfer ended while waiting. The flag is
    /// "for all".
    pub ask: &'a mut dyn FnMut(&str) -> Option<(ConflictChoice, bool)>,
    pub progress: &'a mut dyn FnMut(TransferProgress),
    pub removal: Removal,
    /// `std::fs::rename`; tests stand in a file system boundary.
    pub rename: &'a dyn Fn(&Path, &Path) -> std::io::Result<()>,
}

/// Checks a job before it starts and measures it: a folder into itself or below (FR-022), and too
/// little space at the target for a copy or a move across file systems.
pub fn prepare(
    job: &Job,
    free_space: impl Fn(&Path) -> Option<u64>,
    same_device: impl Fn(&Path, &Path) -> bool,
) -> Result<Totals, FilesError> {
    let mut totals = Totals::default();
    let mut needed = 0;
    for source in &job.sources {
        let meta = std::fs::symlink_metadata(source).map_err(|error| io_error(error, source))?;
        if let Some(target) = &job.target {
            if meta.is_dir() && target.starts_with(source) {
                return Err(FilesError::new(
                    FilesErrorCode::IntoItself,
                    "a folder cannot go into itself",
                ));
            }
        }
        let size = measure(source);
        totals.items += size.items;
        totals.bytes += size.bytes;
        let crossing = job
            .target
            .as_deref()
            .is_some_and(|target| !same_device(source, target));
        if job.op == TransferOp::Copy || (job.op == TransferOp::Move && crossing) {
            needed += size.bytes;
        }
    }
    if let Some(free) = job.target.as_deref().and_then(free_space) {
        if needed > free {
            return Err(FilesError::new(
                FilesErrorCode::NoSpace,
                "not enough free space at the target",
            ));
        }
    }
    Ok(totals)
}

/// Whether `job` would change holzi's own places (FR-037): moving or deleting a folder that holds
/// one, or a folder merging into a folder of the same name that holds one. Reading them is allowed.
pub fn touches_own(job: &Job, own: &OwnPlaces) -> bool {
    job.sources.iter().any(|source| {
        (job.op != TransferOp::Copy && own.touches(source, true))
            || job.target.as_deref().is_some_and(|target| {
                source
                    .file_name()
                    .is_some_and(|name| own.touches(&target.join(name), true))
            })
    })
}

/// Files and bytes below `path` (a file or link counts once).
fn measure(path: &Path) -> Totals {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Totals::default();
    };
    if !meta.is_dir() {
        return Totals {
            items: 1,
            bytes: if meta.is_file() { meta.len() } else { 0 },
        };
    }
    let mut totals = Totals::default();
    if let Ok(read) = std::fs::read_dir(path) {
        for child in read.flatten() {
            let size = measure(&child.path());
            totals.items += size.items;
            totals.bytes += size.bytes;
        }
    }
    totals
}

/// Why a transfer stopped early.
enum Stop {
    Cancelled,
    Failed(FilesError),
}

impl From<FilesError> for Stop {
    fn from(error: FilesError) -> Self {
        Self::Failed(error)
    }
}

/// The running state of one transfer.
struct Run<'h, 'a> {
    hooks: &'h mut Hooks<'a>,
    totals: Totals,
    done: Totals,
    for_all: Option<ConflictChoice>,
}

/// Runs a prepared job to its end.
pub fn run(job: &Job, totals: &Totals, mut hooks: Hooks<'_>) -> Outcome {
    let mut state = Run {
        hooks: &mut hooks,
        totals: *totals,
        done: Totals::default(),
        for_all: None,
    };
    let result = match (job.op, &job.target) {
        (TransferOp::Delete, _) => job.sources.iter().try_for_each(|path| state.delete(path)),
        (TransferOp::Copy, Some(target)) => job
            .sources
            .iter()
            .try_for_each(|path| state.copy_into(path, target)),
        (TransferOp::Move, Some(target)) => job
            .sources
            .iter()
            .try_for_each(|path| state.move_into(path, target)),
        (_, None) => Err(Stop::Failed(FilesError::invalid_path("no target folder"))),
    };
    match result {
        Ok(()) => Outcome::Done,
        Err(Stop::Cancelled) => Outcome::Cancelled,
        Err(Stop::Failed(error)) => Outcome::Failed(error),
    }
}

/// What to do with a name at the target.
enum Placement {
    /// Write to this path.
    At(PathBuf),
    /// Merge into the folder there.
    Merge(PathBuf),
    /// Write to this path in place of what is there ([`Run::replacing`]).
    Replace(PathBuf),
    Skip,
}

impl Run<'_, '_> {
    fn check(&self) -> Result<(), Stop> {
        if self.hooks.cancel.is_cancelled() {
            Err(Stop::Cancelled)
        } else {
            Ok(())
        }
    }

    fn report(&mut self) {
        (self.hooks.progress)(TransferProgress {
            items_done: self.done.items,
            items_total: self.totals.items,
            bytes_done: self.done.bytes,
            bytes_total: self.totals.bytes,
        });
    }

    fn count(&mut self, size: Totals) {
        self.done.items += size.items;
        self.done.bytes += size.bytes;
        self.report();
    }

    /// Where `source` goes in `folder`; asks when the name is taken.
    fn place(&mut self, source: &Path, folder: &Path, name: &OsStr) -> Result<Placement, Stop> {
        let target = folder.join(name);
        if target == source {
            return Ok(Placement::At(free_name(folder, &name.to_string_lossy())));
        }
        let Ok(there) = std::fs::symlink_metadata(&target) else {
            return Ok(Placement::At(target));
        };
        let source_dir = std::fs::symlink_metadata(source).is_ok_and(|meta| meta.is_dir());
        if source_dir && there.is_dir() {
            return Ok(Placement::Merge(target));
        }
        let choice = match self.for_all {
            Some(choice) => choice,
            None => {
                let (choice, for_all) =
                    (self.hooks.ask)(&name.to_string_lossy()).ok_or(Stop::Cancelled)?;
                if for_all {
                    self.for_all = Some(choice);
                }
                choice
            }
        };
        self.check()?;
        Ok(match choice {
            ConflictChoice::Replace => Placement::Replace(target),
            ConflictChoice::KeepBoth => Placement::At(free_name(folder, &name.to_string_lossy())),
            ConflictChoice::Skip => Placement::Skip,
        })
    }

    fn copy_into(&mut self, source: &Path, folder: &Path) -> Result<(), Stop> {
        self.check()?;
        let name = entry_name(source)?;
        match self.place(source, folder, name)? {
            Placement::Skip => {
                self.count(measure(source));
                Ok(())
            }
            Placement::At(target) | Placement::Merge(target) => self.copy_to(source, &target),
            Placement::Replace(target) => {
                self.replacing(&target, |run| run.copy_to(source, &target))
            }
        }
    }

    /// Puts the new entry at `target` through `put`. What is there moves aside first (in its own
    /// folder, so never across file systems) and goes for good only once `put` succeeded; after a
    /// cancel or failure, what `put` left at `target` goes and the old entry comes back.
    fn replacing(
        &mut self,
        target: &Path,
        put: impl FnOnce(&mut Self) -> Result<(), Stop>,
    ) -> Result<(), Stop> {
        let folder = target
            .parent()
            .ok_or_else(|| FilesError::invalid_path("no target folder"))?;
        let name = entry_name(target)?.to_string_lossy();
        let aside = folder.join(format!(".{name}.holzi-old-{}", uuid::Uuid::new_v4()));
        std::fs::rename(target, &aside).map_err(|error| io_error(error, target))?;
        match put(self) {
            Ok(()) => remove(&aside, Removal::Permanent).map_err(Stop::from),
            Err(stop) => {
                let _ = remove(target, Removal::Permanent);
                if let Err(error) = std::fs::rename(&aside, target) {
                    log::warn!(
                        "files: {} could not come back from {}: {error}",
                        target.display(),
                        aside.display()
                    );
                }
                Err(stop)
            }
        }
    }

    /// Copies `source` to the free `target`.
    fn copy_to(&mut self, source: &Path, target: &Path) -> Result<(), Stop> {
        let meta = std::fs::symlink_metadata(source).map_err(|error| io_error(error, source))?;
        if meta.is_dir() {
            if !target.is_dir() {
                std::fs::create_dir(target).map_err(|error| io_error(error, target))?;
            }
            let children = std::fs::read_dir(source).map_err(|error| io_error(error, source))?;
            for child in children {
                let child = child.map_err(|error| io_error(error, source))?;
                self.copy_into(&child.path(), target)?;
            }
            return Ok(());
        }
        if meta.file_type().is_symlink() {
            copy_link(source, target)?;
            self.count(Totals { items: 1, bytes: 0 });
            return Ok(());
        }
        self.copy_file(source, &meta, target)
    }

    /// One file through a part file beside `target`.
    fn copy_file(&mut self, source: &Path, meta: &Metadata, target: &Path) -> Result<(), Stop> {
        let folder = target
            .parent()
            .ok_or_else(|| FilesError::invalid_path("no target folder"))?;
        let name = entry_name(target)?.to_string_lossy();
        let part = folder.join(format!(".{name}.holzi-part-{}", uuid::Uuid::new_v4()));
        let result = self.write_part(source, meta, &part).and_then(|()| {
            (self.hooks.rename)(&part, target).map_err(|error| Stop::from(io_error(error, target)))
        });
        if result.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        result?;
        self.done.items += 1;
        self.report();
        Ok(())
    }

    fn write_part(&mut self, source: &Path, meta: &Metadata, part: &Path) -> Result<(), Stop> {
        let mut from = File::open(source).map_err(|error| io_error(error, source))?;
        let mut to = File::create_new(part).map_err(|error| write_error(error, part))?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            self.check()?;
            let read = from
                .read(&mut buf)
                .map_err(|error| io_error(error, source))?;
            if read == 0 {
                break;
            }
            to.write_all(&buf[..read])
                .map_err(|error| write_error(error, part))?;
            self.done.bytes += read as u64;
            self.report();
        }
        to.sync_all().map_err(|error| write_error(error, part))?;
        // The copy keeps the permissions and the time of change.
        let _ = to.set_permissions(meta.permissions());
        if let Ok(modified) = meta.modified() {
            let _ = to.set_modified(modified);
        }
        Ok(())
    }

    fn move_into(&mut self, source: &Path, folder: &Path) -> Result<(), Stop> {
        self.check()?;
        // Gone: an earlier try of this transfer moved it (a retry).
        if std::fs::symlink_metadata(source).is_err() {
            return Ok(());
        }
        let name = entry_name(source)?;
        if folder.join(name) == source {
            self.count(measure(source));
            return Ok(());
        }
        match self.place(source, folder, name)? {
            Placement::Skip => {
                self.count(measure(source));
                Ok(())
            }
            Placement::Merge(target) => {
                let children =
                    std::fs::read_dir(source).map_err(|error| io_error(error, source))?;
                for child in children {
                    let child = child.map_err(|error| io_error(error, source))?;
                    self.move_into(&child.path(), &target)?;
                }
                // Skipped names stay behind in the source.
                let _ = std::fs::remove_dir(source);
                Ok(())
            }
            Placement::At(target) => self.move_to(source, &target),
            Placement::Replace(target) => {
                self.replacing(&target, |run| run.move_to(source, &target))
            }
        }
    }

    /// Moves `source` to the free `target`: a rename, or across file systems a copy and a delete.
    fn move_to(&mut self, source: &Path, target: &Path) -> Result<(), Stop> {
        let size = measure(source);
        match (self.hooks.rename)(source, target) {
            Ok(()) => {
                self.count(size);
                Ok(())
            }
            Err(error) if error.kind() == ErrorKind::CrossesDevices => {
                self.copy_to(source, target)?;
                self.check()?;
                remove(source, Removal::Permanent).map_err(Stop::from)
            }
            Err(error) => Err(io_error(error, source).into()),
        }
    }

    fn delete(&mut self, path: &Path) -> Result<(), Stop> {
        self.check()?;
        let size = measure(path);
        remove(path, self.hooks.removal)?;
        self.count(size);
        Ok(())
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "local_replace_tests.rs"]
mod replace_tests;
