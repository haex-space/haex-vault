//! Size-based rotation of the log files of one vault (spec 098 FR-011; research R6).
//!
//! `haex.log` is the current file; `haex.log.1` to `haex.log.N` are older ones, the highest number
//! the oldest. A file is never split inside a record, and the oldest lines go first when the
//! limit is reached. The limits are arguments, so a test uses small ones.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The name of the current file.
pub const FILE_NAME: &str = "haex.log";
const COMPACT_READY: &str = "haex.log.compact-ready";
const COMPACT_READY_TEMP: &str = "haex.log.compact-ready.tmp";

/// The log files of `folder` in time order, the oldest first: `haex.log.N` down to `haex.log.1`,
/// then `haex.log`. Only files that exist and are no compaction leftovers.
pub fn files_oldest_first(folder: &Path) -> Vec<PathBuf> {
    let mut archives: Vec<usize> = fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let number = name.to_str()?.strip_prefix(FILE_NAME)?.strip_prefix('.')?;
            number.parse::<usize>().ok().filter(|index| *index >= 1)
        })
        .collect();
    archives.sort_unstable_by(|a, b| b.cmp(a));
    let mut files: Vec<PathBuf> = archives
        .into_iter()
        .map(|index| folder.join(format!("{FILE_NAME}.{index}")))
        .collect();
    let current = folder.join(FILE_NAME);
    if current.is_file() {
        files.push(current);
    }
    files
}

/// How much a log may keep: `files` files (the current one and its archives) of `bytes` each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub files: usize,
    pub bytes: u64,
}

impl Limits {
    /// "Standard": about 1 MiB.
    pub const STANDARD: Self = Self {
        files: 4,
        bytes: 256 * 1024,
    };
    /// "Ausführlich": about 20 MiB, so a session of half an hour is still whole.
    pub const VERBOSE: Self = Self {
        files: 5,
        bytes: 4 * 1024 * 1024,
    };

    fn archives(self) -> usize {
        self.files.saturating_sub(1)
    }
}

/// The current file and the rules for rotating it.
pub struct RotatingFile {
    folder: PathBuf,
    limits: Limits,
    /// `None` after a failed reopen; the next write tries again.
    file: Option<File>,
    size: u64,
    repair_to: Option<u64>,
    needs_compact: bool,
}

impl RotatingFile {
    /// Opens `haex.log` in `folder` for appending, creating the folder. Files left by a longer
    /// limit (a vault that was verbose before the app was restarted) are brought within `limits`.
    pub fn open(folder: &Path, limits: Limits) -> io::Result<Self> {
        fs::create_dir_all(folder)?;
        let mut this = Self {
            folder: folder.to_path_buf(),
            limits,
            file: None,
            size: 0,
            repair_to: None,
            needs_compact: false,
        };
        this.recover_compaction()?;
        this.reopen()?;
        this.compact()?;
        Ok(this)
    }

    /// Appends one record (which may span lines, a stack) and a line break. Rotates first when
    /// the record would pass the limit. An error leaves the record unwritten; the next call tries
    /// again.
    pub fn write_line(&mut self, record: &str) -> io::Result<()> {
        let len = record.len() as u64 + 1;
        if self.needs_compact || self.ready_marker().exists() {
            self.compact()?;
        }
        if self.file.is_none() {
            self.reopen()?;
        }
        self.repair_tail()?;
        if self.size > 0 && self.size + len > self.limits.bytes {
            self.rotate()?;
        }
        let mut bytes = Vec::with_capacity(record.len() + 1);
        bytes.extend_from_slice(record.as_bytes());
        bytes.push(b'\n');
        let Some(file) = self.file.as_mut() else {
            return Err(io::Error::other("the log file is not open"));
        };
        // One call per record, unbuffered: a crash loses no line that was reported as written.
        if let Err(error) = file.write_all(&bytes) {
            self.repair_to = Some(self.size);
            if truncate_file(&self.current(), self.size).is_ok() {
                self.repair_to = None;
            }
            return Err(error);
        }
        self.size += len;
        Ok(())
    }

    /// Changes the limits and brings the files within them at once: on a switch from
    /// "Ausführlich" to "Standard" only the newest whole records that fit stay (spec FR-011).
    pub fn set_limits(&mut self, limits: Limits) -> io::Result<()> {
        self.limits = limits;
        self.compact()
    }

    pub fn flush(&mut self) -> io::Result<()> {
        match self.file.as_mut() {
            Some(file) => file.flush(),
            None => Ok(()),
        }
    }

    fn current(&self) -> PathBuf {
        self.folder.join(FILE_NAME)
    }

    fn archive(&self, index: usize) -> PathBuf {
        self.folder.join(format!("{FILE_NAME}.{index}"))
    }

    fn reopen(&mut self) -> io::Result<()> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.current())?;
        self.size = file.metadata()?.len();
        self.file = Some(file);
        Ok(())
    }

    fn repair_tail(&mut self) -> io::Result<()> {
        let Some(size) = self.repair_to else {
            return Ok(());
        };
        if self.file.is_none() {
            return Err(io::Error::other("the log file is not open"));
        }
        // Append handles on Windows do not include FILE_WRITE_DATA, which set_len requires.
        // Open a separate truncation handle rather than relying on append mode for this repair.
        truncate_file(&self.current(), size)?;
        self.size = size;
        self.repair_to = None;
        Ok(())
    }

    /// The oldest file goes, the others move up one number, the current file becomes `.1`.
    fn rotate(&mut self) -> io::Result<()> {
        // A file that is open cannot be renamed on Windows.
        self.file = None;
        let archives = self.limits.archives();
        if archives == 0 {
            fs::remove_file(self.current())?;
        } else {
            remove_if_exists(&self.archive(archives))?;
            for index in (1..archives).rev() {
                rename_if_exists(&self.archive(index), &self.archive(index + 1))?;
            }
            fs::rename(self.current(), self.archive(1))?;
        }
        self.reopen()
    }

    /// The numbers of the archives on disk, highest first.
    fn archive_indices(&self) -> Vec<usize> {
        let mut found: Vec<usize> = fs::read_dir(&self.folder)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name();
                let number = name.to_str()?.strip_prefix(FILE_NAME)?.strip_prefix('.')?;
                number.parse::<usize>().ok().filter(|index| *index >= 1)
            })
            .collect();
        found.sort_unstable_by(|a, b| b.cmp(a));
        found
    }

    /// Brings the files within the limits when they are not: more archives than the limit keeps,
    /// or a file above the size.
    fn compact(&mut self) -> io::Result<()> {
        self.recover_compaction()?;
        // A failed append may have left an incomplete record. Repair it before reading the
        // current file into a compacted generation, even if the rollback previously failed.
        self.repair_tail()?;
        let indices = self.archive_indices();
        let too_many = indices
            .first()
            .is_some_and(|top| *top > self.limits.archives());
        let too_big = indices
            .iter()
            .map(|index| self.archive(*index))
            .chain([self.current()])
            .any(|path| fs::metadata(path).is_ok_and(|meta| meta.len() > self.limits.bytes));
        if !too_many && !too_big {
            self.needs_compact = false;
            return Ok(());
        }
        self.needs_compact = true;
        self.file = None;
        // Oldest first: the highest archive, down to `.1`, then the current file.
        let mut content = Vec::new();
        for index in &indices {
            append_file(&mut content, &self.archive(*index))?;
        }
        append_file(&mut content, &self.current())?;
        let slots = fit(&content, self.limits);
        // New files first, then the old ones go and the new ones take their place.
        for (slot, text) in slots.iter().enumerate() {
            let staged = self.staged(slot);
            fs::write(&staged, text)?;
            OpenOptions::new().write(true).open(staged)?.sync_all()?;
        }
        self.mark_compaction_ready(slots.len())?;
        self.recover_compaction()?;
        self.needs_compact = false;
        self.reopen()
    }

    fn staged(&self, slot: usize) -> PathBuf {
        self.folder.join(format!("{FILE_NAME}.compact-{slot}"))
    }

    fn ready_marker(&self) -> PathBuf {
        self.folder.join(COMPACT_READY)
    }

    fn mark_compaction_ready(&self, slots: usize) -> io::Result<()> {
        let mut marker = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(self.folder.join(COMPACT_READY_TEMP))?;
        write!(marker, "{slots}")?;
        marker.sync_all()?;
        fs::rename(self.folder.join(COMPACT_READY_TEMP), self.ready_marker())
    }

    /// Installs a complete staged generation. The marker is written only after every staged file
    /// is complete; if installation stops part-way through, the next open finishes it.
    fn recover_compaction(&mut self) -> io::Result<()> {
        let marker = self.ready_marker();
        let count = match fs::read_to_string(&marker) {
            Ok(value) => value
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|count| *count > 0)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "diagnostic log compaction marker is invalid",
                    )
                })?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                remove_if_exists(&self.folder.join(COMPACT_READY_TEMP))?;
                self.remove_staged_files(0)?;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        for slot in 0..count {
            let staged = self.staged(slot);
            let target = if slot == 0 {
                self.current()
            } else {
                self.archive(slot)
            };
            if staged.exists() {
                remove_if_exists(&target)?;
                fs::rename(staged, target)?;
            } else if !target.exists() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "diagnostic log compaction is missing a staged file",
                ));
            }
        }
        for index in self.archive_indices() {
            if index >= count {
                remove_if_exists(&self.archive(index))?;
            }
        }
        remove_if_exists(&marker)?;
        remove_if_exists(&self.folder.join(COMPACT_READY_TEMP))?;
        self.remove_staged_files(count)?;
        Ok(())
    }

    fn remove_staged_files(&self, from: usize) -> io::Result<()> {
        let Ok(entries) = fs::read_dir(&self.folder) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(slot) = name
                .strip_prefix(&format!("{FILE_NAME}.compact-"))
                .and_then(|slot| slot.parse::<usize>().ok())
            else {
                continue;
            };
            if slot >= from {
                remove_if_exists(&entry.path())?;
            }
        }
        Ok(())
    }
}

fn truncate_file(path: &Path, length: u64) -> io::Result<()> {
    OpenOptions::new().write(true).open(path)?.set_len(length)
}

/// Splits `content` into records: a line that starts with four spaces continues the record
/// before it (a stack); every other line starts a new one. Each record ends with a line break.
fn records(content: &[u8]) -> Vec<Vec<u8>> {
    let mut found: Vec<Vec<u8>> = Vec::new();
    for line in content.split_inclusive(|byte| *byte == b'\n') {
        match found.last_mut() {
            Some(last) if line.starts_with(b"    ") => last.extend_from_slice(line),
            _ => found.push(line.to_vec()),
        }
    }
    for record in &mut found {
        if record.last() != Some(&b'\n') {
            record.push(b'\n');
        }
    }
    found
}

/// The newest whole records that fit the limits, as one text per file: index 0 is the current
/// file, the others the archives `.1`, `.2` …; older records are dropped. A record above the file
/// size stands alone in a file.
fn fit(content: &[u8], limits: Limits) -> Vec<Vec<u8>> {
    // Newest first, so each text is built back to front and reversed at the end.
    let mut slots: Vec<Vec<Vec<u8>>> = vec![Vec::new()];
    let mut used = 0u64;
    for record in records(content).into_iter().rev() {
        let len = record.len() as u64;
        if used > 0 && used + len > limits.bytes {
            if slots.len() >= limits.files {
                break;
            }
            slots.push(Vec::new());
            used = 0;
        }
        used += len;
        if let Some(slot) = slots.last_mut() {
            slot.push(record);
        }
    }
    slots
        .into_iter()
        .map(|newest_first| newest_first.into_iter().rev().flatten().collect())
        .collect()
}

fn append_file(into: &mut Vec<u8>, path: &Path) -> io::Result<()> {
    match fs::read(path) {
        Ok(bytes) => {
            into.extend_from_slice(&bytes);
            if bytes.last().is_some_and(|byte| *byte != b'\n') {
                into.push(b'\n');
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn rename_if_exists(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "rotation_tests.rs"]
mod tests;
