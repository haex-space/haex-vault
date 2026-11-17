//! The record of tool and delegate activity of one vault (spec 094 FR-019).
//!
//! What happened to a vault's tools (an install, an update, a connect flow, a run) is written to
//! `<vault>/home/records/tools.log`, in the vault's folder, and never to a device-wide log: a
//! file outside the vault folder must not say which vault exists or what it did. The record is
//! append-only, rotated at 64 kB (one older generation is kept) and written on a best-effort
//! basis: a record that cannot be written never fails the action it describes.
//!
//! A line is `<unix seconds> <event> key=value ...`. The event is a fixed name, and a value is
//! cut down to letters, digits, `.`, `_` and `-`: a path, a space or a line break cannot pass
//! through, so a value is one token and a line stays one line. Callers pass only vendor ids,
//! versions and fixed codes, never a name or the text of a request. The record lies in the
//! vault folder, so it can be encrypted later with the other subfolders.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The file name of the record inside the records folder.
pub const RECORD_FILE: &str = "tools.log";
/// Where the previous generation goes when the record is rotated.
pub const ROTATED_FILE: &str = "tools.log.1";
/// The record is rotated before a line that would take it past this size.
pub const MAX_BYTES: u64 = 64 * 1024;

const MAX_VALUE_CHARS: usize = 64;

/// The record of one vault.
#[derive(Debug, Clone)]
pub struct Record {
    dir: PathBuf,
}

impl Record {
    /// The record in `records_dir` (`VaultPaths::records_dir`); nothing is created until the
    /// first line.
    pub fn new(records_dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: records_dir.into(),
        }
    }

    /// Appends one line. Never fails and never panics; the line is dropped when the record
    /// cannot be written.
    pub fn append(&self, event: &'static str, fields: &[(&'static str, &str)]) {
        let _ = self.try_append(event, fields);
    }

    fn try_append(
        &self,
        event: &'static str,
        fields: &[(&'static str, &str)],
    ) -> std::io::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let mut line = format!("{now} {event}");
        for (key, value) in fields {
            line.push_str(&format!(" {key}={}", clean(value)));
        }
        line.push('\n');

        fs::create_dir_all(&self.dir)?;
        let file = self.dir.join(RECORD_FILE);
        let size = fs::metadata(&file).map_or(0, |meta| meta.len());
        if size > 0 && size + line.len() as u64 > MAX_BYTES {
            fs::rename(&file, self.dir.join(ROTATED_FILE))?;
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(file)?
            .write_all(line.as_bytes())
    }

    /// The path of the current record file.
    pub fn path(&self) -> PathBuf {
        self.dir.join(RECORD_FILE)
    }

    /// The records folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

fn clean(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .take(MAX_VALUE_CHARS)
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '?'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "-".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
