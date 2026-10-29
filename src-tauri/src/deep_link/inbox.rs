//! The inbox between holzi processes for a `haex-vault://contact/…` link (spec 060 research R9).
//!
//! holzi runs one process per vault session, and on Linux and Windows the system starts a **new**
//! process for every link. That process cannot know which of the others has the vault the user
//! wants, so it drops the link as a file in a directory, and every process with an open vault looks
//! there. The first to `rename` the file owns it. A file nobody takes within [`HANDOFF_TIMEOUT`] is
//! removed by the process that wrote it, which then keeps the link itself.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use uuid::Uuid;

use crate::error::{HolziError, Result};

/// The directory below the app-local-data directory.
pub const INBOX_DIRECTORY: &str = "link-inbox";

/// How long a process that dropped a link waits for another to take it.
pub const HANDOFF_TIMEOUT: Duration = Duration::from_secs(2);

/// How long a process without the focus leaves a fresh link to the others, so that the window the
/// person used last takes it (spec 063 research R6). Shorter than [`HANDOFF_TIMEOUT`].
pub const UNFOCUSED_DELAY: Duration = Duration::from_secs(1);

/// How often the offering process looks whether its file was taken.
const POLL: Duration = Duration::from_millis(50);

const OFFERED_EXTENSION: &str = "url";
const WRITING_EXTENSION: &str = "tmp";
const CLAIMED_MARKER: &str = ".claimed-";

/// What became of an offered link.
#[derive(Debug, PartialEq, Eq)]
pub enum Offered {
    /// Another process took it.
    Taken,
    /// Nobody took it in time; the file is gone again.
    NoTaker,
}

/// Drops `url` in the inbox and waits up to [`HANDOFF_TIMEOUT`] for another process to take it.
/// Blocks, so a caller on an async thread uses `spawn_blocking`.
pub fn offer(dir: &Path, url: &str) -> Result<Offered> {
    fs::create_dir_all(dir).map_err(HolziError::from)?;
    let id = Uuid::new_v4();
    // Written under another name first: a process looking at the directory never reads half a link.
    let writing = dir.join(format!("{id}.{WRITING_EXTENSION}"));
    let offered = dir.join(format!("{id}.{OFFERED_EXTENSION}"));
    fs::write(&writing, url).map_err(HolziError::from)?;
    fs::rename(&writing, &offered).map_err(HolziError::from)?;

    let deadline = Instant::now() + HANDOFF_TIMEOUT;
    while offered.exists() {
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(POLL);
    }
    match fs::remove_file(&offered) {
        Ok(()) => Ok(Offered::NoTaker),
        // Taken between the last look and the removal.
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Offered::Taken),
        Err(e) => Err(HolziError::from(e)),
    }
}

/// Takes one offered link, if there is one: the file is renamed to `<id>.claimed-<pid>` (only one
/// process can win that), read and deleted, in any case. A file older than [`HANDOFF_TIMEOUT`] is
/// not taken, because its writer has given up on it; files older than twice that are removed. A
/// process whose window has no focus takes a link only after [`UNFOCUSED_DELAY`].
pub fn claim_next(dir: &Path, focused: bool) -> Result<Option<String>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(HolziError::from(e)),
    };
    let now = SystemTime::now();
    for entry in entries {
        let path = entry.map_err(HolziError::from)?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let offered = path.extension().is_some_and(|ext| ext == OFFERED_EXTENSION);
        let leftover = path.extension().is_some_and(|ext| ext == WRITING_EXTENSION)
            || name.contains(CLAIMED_MARKER);
        if !offered && !leftover {
            continue;
        }
        let age = fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .unwrap_or_default();
        if age > HANDOFF_TIMEOUT * 2 {
            let _ = fs::remove_file(&path);
            continue;
        }
        if !offered || age > HANDOFF_TIMEOUT || (!focused && age < UNFOCUSED_DELAY) {
            continue;
        }
        let claimed = path.with_extension(format!("claimed-{}", std::process::id()));
        // Lost the race, or the writer gave up and removed it.
        if fs::rename(&path, &claimed).is_err() {
            continue;
        }
        let read = fs::read_to_string(&claimed);
        let _ = fs::remove_file(&claimed);
        return read.map(Some).map_err(HolziError::from);
    }
    Ok(None)
}

#[cfg(test)]
#[path = "inbox_tests.rs"]
mod inbox_tests;
