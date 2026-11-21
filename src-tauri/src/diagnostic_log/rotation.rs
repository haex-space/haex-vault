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
        };
        this.reopen()?;
        this.compact()?;
        Ok(this)
    }

    /// Appends one record (which may span lines, a stack) and a line break. Rotates first when
    /// the record would pass the limit. An error leaves the record unwritten; the next call tries
    /// again.
    pub fn write_line(&mut self, record: &str) -> io::Result<()> {
        let len = record.len() as u64 + 1;
        if self.file.is_none() {
            self.reopen()?;
        }
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
        file.write_all(&bytes)?;
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
            return Ok(());
        }
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
            fs::write(self.staged(slot), text)?;
        }
        for index in &indices {
            remove_if_exists(&self.archive(*index))?;
        }
        remove_if_exists(&self.current())?;
        for slot in 0..slots.len() {
            let target = if slot == 0 {
                self.current()
            } else {
                self.archive(slot)
            };
            fs::rename(self.staged(slot), target)?;
        }
        self.reopen()
    }

    fn staged(&self, slot: usize) -> PathBuf {
        self.folder.join(format!("{FILE_NAME}.compact-{slot}"))
    }
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
