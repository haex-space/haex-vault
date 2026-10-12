//! Transfers between sources (spec 044 US5, FR-018 to FR-020, FR-025): up from the device to a
//! storage, down from a storage, within one bucket and between storages. Runs on a blocking thread
//! like [`super::local`], with the same [`Hooks`] (conflict question, progress, cancel); each call
//! to a provider goes through the runtime's `block_on`.
//!
//! - Up: a file under [`PART`] in one `put`, a larger one in parts of [`PART`]; a cancel or a
//!   failure aborts the upload, so no part stays at the provider (FR-020).
//! - Down: into a part file beside the target, renamed at the end, as on the device.
//! - Within one bucket the provider copies; between storages the bytes stream through holzi, never
//!   more than one part in memory.
//! - A network error is tried again after 1, 2 and 4 seconds, then the transfer fails and can be
//!   started again (FR-025).
//! - A folder onto a folder merges; only files ask about a taken name.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::runtime::Handle;
use tokio::time::Instant;

use super::local::{free_name, Hooks, Outcome, Totals, CHUNK};
use super::{ConflictChoice, TransferOp, TransferProgress};
use crate::files::local::ops::io_error;
use crate::files::storage_source::{folder_prefix, storage_error, StorageFiles};
use crate::files::{FilesError, FilesErrorCode};
use crate::remote_storage::StorageError;

/// The size of an upload's parts, and the largest file sent in one `put`.
pub const PART: usize = 8 * 1024 * 1024;

/// The waits before trying a call again (FR-025).
pub const RETRIES: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];

/// How long one call may take; a part of 8 MiB on a slow line needs a while.
pub(super) const CALL: Duration = Duration::from_secs(5 * 60);

pub use super::remote_plan::{prepare_remote, Plan, RemoteJob, Side};
use super::remote_plan::{remove_empty, EncSource, EncTarget, Item};

fn soon() -> Instant {
    Instant::now() + CALL
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

/// The running state of a transfer.
struct Run<'h, 'a> {
    job: &'h RemoteJob,
    handle: &'h Handle,
    retries: &'h [Duration],
    hooks: &'h mut Hooks<'a>,
    /// The target when it is in an encrypted folder (spec 048).
    target: Option<&'h EncTarget>,
    totals: Totals,
    done: Totals,
    for_all: Option<ConflictChoice>,
    /// Folders a move out of encrypted folders leaves, for the end.
    emptied: Vec<EncSource>,
}

/// Runs a prepared job to its end; `retries` are the waits before trying a call again.
pub fn run_remote(
    job: &RemoteJob,
    plan: &Plan,
    handle: &Handle,
    retries: &[Duration],
    mut hooks: Hooks<'_>,
) -> Outcome {
    let mut run = Run {
        job,
        handle,
        retries,
        hooks: &mut hooks,
        target: plan.target.as_ref(),
        totals: plan.totals,
        done: Totals::default(),
        for_all: None,
        emptied: Vec::new(),
    };
    let mut result = plan.items.iter().try_for_each(|item| run.item(item));
    // A move leaves the device folders it emptied behind; they go too.
    if result.is_ok() && job.op == TransferOp::Move && matches!(job.from, Side::Device) {
        for folder in &plan.folders {
            remove_empty(folder);
        }
    }
    if result.is_ok() && !run.emptied.is_empty() {
        result = run.finish_move();
    }
    match result {
        Ok(()) => Outcome::Done,
        Err(Stop::Cancelled) => Outcome::Cancelled,
        Err(Stop::Failed(error)) => Outcome::Failed(error),
    }
}

/// The place of an item at the target.
enum Place {
    Device(PathBuf),
    Key(String),
    /// The names below the target folder in an encrypted folder.
    Encrypted(Vec<String>),
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

    /// Runs `call` until it works, trying a network failure again after each wait of `retries`.
    fn retried<T>(&self, mut call: impl FnMut() -> Result<T, StorageError>) -> Result<T, Stop> {
        let mut waits = self.retries.iter();
        loop {
            self.check()?;
            match call() {
                Ok(value) => return Ok(value),
                Err(error @ (StorageError::Network | StorageError::TimedOut)) => {
                    let Some(wait) = waits.next() else {
                        return Err(Stop::Failed(storage_error(error)));
                    };
                    let until = std::time::Instant::now() + *wait;
                    while std::time::Instant::now() < until {
                        self.check()?;
                        std::thread::sleep(Duration::from_millis(50).min(*wait));
                    }
                }
                Err(error) => return Err(Stop::Failed(storage_error(error))),
            }
        }
    }

    fn target_storage(&self) -> Option<&StorageFiles> {
        match &self.job.to {
            Side::Storage(files) => Some(files),
            Side::Device => None,
        }
    }

    fn place_of(&self, rel: &[String], folder: bool) -> Result<Place, Stop> {
        if self.target.is_some() {
            return Ok(Place::Encrypted(rel.to_vec()));
        }
        Ok(match &self.job.to {
            Side::Device => {
                let mut path = PathBuf::from(&self.job.target);
                path.extend(rel);
                Place::Device(path)
            }
            Side::Storage(_) => {
                let mut key = folder_prefix(&self.job.target)?;
                key.push_str(&rel.join("/"));
                if folder {
                    key.push('/');
                }
                Place::Key(key)
            }
        })
    }

    fn taken(&self, place: &Place) -> Result<bool, Stop> {
        match place {
            Place::Device(path) => Ok(std::fs::symlink_metadata(path).is_ok()),
            Place::Encrypted(rel) => self.encrypted_taken(rel),
            Place::Key(key) => {
                let files = self.target_storage().expect("a key is on a storage");
                match self.retried(|| {
                    self.handle
                        .block_on(files.store().head(files.access(), key, soon()))
                }) {
                    Ok(_) => Ok(true),
                    Err(Stop::Failed(error)) if error.code == FilesErrorCode::NotFound => Ok(false),
                    Err(stop) => Err(stop),
                }
            }
        }
    }

    /// Where a file goes, after asking when its name is taken; `None` to skip it.
    fn resolve(&mut self, rel: &[String]) -> Result<Option<Place>, Stop> {
        let place = self.place_of(rel, false)?;
        if !self.taken(&place)? {
            return Ok(Some(place));
        }
        let name = rel.last().cloned().unwrap_or_default();
        let choice = match self.for_all {
            Some(choice) => choice,
            None => {
                let (choice, for_all) = (self.hooks.ask)(&name).ok_or(Stop::Cancelled)?;
                if for_all {
                    self.for_all = Some(choice);
                }
                choice
            }
        };
        self.check()?;
        Ok(match choice {
            ConflictChoice::Replace => Some(place),
            ConflictChoice::Skip => None,
            ConflictChoice::KeepBoth => Some(match place {
                Place::Device(path) => {
                    Place::Device(free_name(path.parent().unwrap_or(&path), &name))
                }
                Place::Key(key) => Place::Key(self.free_key(&key)?),
                Place::Encrypted(rel) => Place::Encrypted(self.encrypted_free(rel)?),
            }),
        })
    }

    /// The first free `<stem> (n)<ext>` beside `key`.
    fn free_key(&self, key: &str) -> Result<String, Stop> {
        let (folder, name) = key.rsplit_once('/').map_or(("", key), |(f, n)| (f, n));
        let (stem, ext) = match name.rfind('.') {
            Some(dot) if dot > 0 => name.split_at(dot),
            _ => (name, ""),
        };
        for n in 2.. {
            let candidate = if folder.is_empty() {
                format!("{stem} ({n}){ext}")
            } else {
                format!("{folder}/{stem} ({n}){ext}")
            };
            if !self.taken(&Place::Key(candidate.clone()))? {
                return Ok(candidate);
            }
        }
        unreachable!("some number is free")
    }

    fn item(&mut self, item: &Item) -> Result<(), Stop> {
        self.check()?;
        if let Some(source) = item.encrypted.as_ref().filter(|source| source.relink) {
            return self.relink(item, source);
        }
        if item.folder {
            self.make_folder(item)?;
        } else {
            let Some(place) = self.resolve(&item.rel)? else {
                self.done.items += 1;
                self.done.bytes += item.size;
                self.report();
                return Ok(());
            };
            self.send(item, &place)?;
            self.done.items += 1;
            self.report();
        }
        if self.job.op == TransferOp::Move {
            self.remove_source(item)?;
        }
        Ok(())
    }

    fn make_folder(&mut self, item: &Item) -> Result<(), Stop> {
        match self.place_of(&item.rel, true)? {
            Place::Encrypted(rel) => {
                self.encrypted_folder(&rel)?;
            }
            Place::Device(path) => {
                std::fs::create_dir_all(&path).map_err(|error| io_error(error, &path))?;
            }
            Place::Key(key) => {
                if !self.taken(&Place::Key(key.clone()))? {
                    let files = self.target_storage().expect("a key is on a storage");
                    self.retried(|| {
                        self.handle.block_on(files.store().put(
                            files.access(),
                            &key,
                            Vec::new(),
                            soon(),
                        ))
                    })?;
                }
            }
        }
        Ok(())
    }

    fn send(&mut self, item: &Item, place: &Place) -> Result<(), Stop> {
        if item.encrypted.is_some() || matches!(place, Place::Encrypted(_)) {
            return self.send_encrypted(item, place);
        }
        match (&self.job.from, place) {
            (Side::Device, Place::Key(key)) => {
                let mut file = std::fs::File::open(&item.from)
                    .map_err(|error| io_error(error, Path::new(&item.from)))?;
                self.upload(key, &mut |buf| file.read(buf))
            }
            (Side::Storage(from), Place::Device(path)) => {
                let from = Arc::clone(from);
                self.download(path, |run, part| {
                    run.fetch(&from, &item.from, item.size, part)
                })
            }
            (Side::Storage(from), Place::Key(key)) => {
                let from = Arc::clone(from);
                let to = self.target_storage().expect("a key is on a storage");
                // Within one bucket the provider copies, when it can (spec 050 research R8).
                if from.access().location == to.access().location
                    && from.store().copies_on_provider(from.access())
                {
                    self.retried(|| {
                        self.handle.block_on(from.store().copy(
                            from.access(),
                            &item.from,
                            key,
                            soon(),
                        ))
                    })?;
                    self.done.bytes += item.size;
                    return Ok(());
                }
                let mut reader = self.retried(|| {
                    self.handle.block_on(from.store().get_range(
                        from.access(),
                        &item.from,
                        0,
                        item.size,
                        soon(),
                    ))
                })?;
                let handle = self.handle.clone();
                self.upload(key, &mut |buf| handle.block_on(reader.read(buf)))
            }
            _ => Err(Stop::Failed(FilesError::new(
                FilesErrorCode::Unsupported,
                "device to device runs as a local transfer",
            ))),
        }
    }

    /// Fills a part file beside `path` with `fetch` and renames it at the end.
    fn download(
        &mut self,
        path: &Path,
        fetch: impl FnOnce(&mut Self, &Path) -> Result<(), Stop>,
    ) -> Result<(), Stop> {
        let folder = path
            .parent()
            .ok_or_else(|| FilesError::invalid_path("no target folder"))?;
        std::fs::create_dir_all(folder).map_err(|error| io_error(error, folder))?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let part = folder.join(format!(".{name}.holzi-part-{}", uuid::Uuid::new_v4()));
        let result = fetch(self, &part).and_then(|()| {
            (self.hooks.rename)(&part, path).map_err(|error| Stop::from(io_error(error, path)))
        });
        if result.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        result
    }

    fn fetch(
        &mut self,
        files: &StorageFiles,
        key: &str,
        size: u64,
        part: &Path,
    ) -> Result<(), Stop> {
        let mut reader = self.retried(|| {
            self.handle.block_on(
                files
                    .store()
                    .get_range(files.access(), key, 0, size, soon()),
            )
        })?;
        let mut out = std::fs::File::create(part).map_err(|error| io_error(error, part))?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            self.check()?;
            let read = self
                .handle
                .block_on(reader.read(&mut buf))
                .map_err(|_| Stop::Failed(storage_error(StorageError::Network)))?;
            if read == 0 {
                break;
            }
            out.write_all(&buf[..read])
                .map_err(|error| io_error(error, part))?;
            self.done.bytes += read as u64;
            self.report();
        }
        out.sync_all().map_err(|error| io_error(error, part).into())
    }

    /// After a move: the source goes (a device file for good, an object of a storage, a file of
    /// an encrypted folder with its versions).
    fn remove_source(&mut self, item: &Item) -> Result<(), Stop> {
        if let Some(source) = &item.encrypted {
            return self.remove_encrypted(source);
        }
        match &self.job.from {
            Side::Device => {
                if !item.folder {
                    std::fs::remove_file(&item.from)
                        .map_err(|error| io_error(error, Path::new(&item.from)))?;
                }
                Ok(())
            }
            Side::Storage(files) => self.retried(|| {
                self.handle
                    .block_on(files.store().delete(files.access(), &item.from, soon()))
            }),
        }
    }
}

#[path = "remote_encrypted.rs"]
mod encrypted;
#[path = "remote_encrypted_move.rs"]
mod encrypted_move;
#[path = "remote_upload.rs"]
mod upload;

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
