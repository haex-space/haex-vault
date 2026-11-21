//! The one place log records are written to a vault's folder (spec 098; research R1).
//!
//! The sink is a `log::Log` behind a swappable writer. With no writer attached it drops every
//! record: nothing is written, kept or buffered before a vault is unlocked (FR-008). With one it
//! formats the line, checks the level and writes it, and it never touches a file outside the
//! folder it was attached to (FR-007). A failing disk never fails the app: the line is dropped,
//! the sink remembers it (FR-016) and tries again on the next record.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use jiff::civil::DateTime;
use log::{Level, Log, Metadata, Record};
use uuid::Uuid;

use super::level::LogLevel;
use super::rotation::RotatingFile;

/// The origin of a record the interface wrote.
pub const UI_ORIGIN: &str = "ui";
/// The longest record, in bytes; a longer text is cut (an error text that carries a page).
pub const MAX_RECORD: usize = 16 * 1024;
const CUT: &str = " …[cut]";

const STANDARD: u8 = 0;
const VERBOSE: u8 = 1;
const DETACHED: u8 = 2;

struct Attached {
    file: RotatingFile,
    level: LogLevel,
    /// Tells a queued message of the interface which vault it was written for.
    session: String,
}

pub struct DiagnosticSink {
    attached: Mutex<Option<Attached>>,
    /// The level as the cheap check of `enabled`, or `DETACHED`.
    level: AtomicU8,
    unwritable: AtomicBool,
    on_unwritable: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

impl DiagnosticSink {
    pub const fn new() -> Self {
        Self {
            attached: Mutex::new(None),
            level: AtomicU8::new(DETACHED),
            unwritable: AtomicBool::new(false),
            on_unwritable: OnceLock::new(),
        }
    }

    /// Runs `hook` when a write fails after the earlier ones went through. It must not log.
    pub fn on_unwritable(&self, hook: impl Fn() + Send + Sync + 'static) {
        // The first hook stays: the app handle it holds is the same for every vault.
        let _ = self.on_unwritable.set(Box::new(hook));
    }

    /// Starts writing into `folder`, at `level`, from now on; what an earlier folder held is not
    /// touched again. Returns the token of this attachment. A failure leaves the sink detached.
    pub fn attach(&self, folder: &Path, level: LogLevel) -> std::io::Result<String> {
        let mut guard = self.lock();
        // The old writer is flushed and dropped before the new one opens.
        if let Some(mut old) = guard.take() {
            let _ = old.file.flush();
        }
        self.level.store(DETACHED, Ordering::Release);
        let file = match RotatingFile::open(folder, level.limits()) {
            Ok(file) => file,
            Err(error) => {
                self.unwritable.store(true, Ordering::Release);
                return Err(error);
            }
        };
        let session = Uuid::new_v4().to_string();
        *guard = Some(Attached {
            file,
            level,
            session: session.clone(),
        });
        self.level.store(code(level), Ordering::Release);
        self.unwritable.store(false, Ordering::Release);
        Ok(session)
    }

    /// Flushes and stops writing; later records are dropped again.
    pub fn detach(&self) {
        let mut guard = self.lock();
        if let Some(mut old) = guard.take() {
            let _ = old.file.flush();
        }
        self.level.store(DETACHED, Ordering::Release);
    }

    /// Changes the level of the attached log at once, and the size it may take with it.
    pub fn set_level(&self, level: LogLevel) -> std::io::Result<()> {
        let mut guard = self.lock();
        let Some(attached) = guard.as_mut() else {
            return Ok(());
        };
        attached.level = level;
        self.level.store(code(level), Ordering::Release);
        attached.file.set_limits(level.limits())
    }

    pub fn session(&self) -> Option<String> {
        self.lock()
            .as_ref()
            .map(|attached| attached.session.clone())
    }

    pub fn is_attached(&self) -> bool {
        self.level.load(Ordering::Acquire) != DETACHED
    }

    /// Whether the last write or the attachment failed.
    pub fn unwritable(&self) -> bool {
        self.unwritable.load(Ordering::Acquire)
    }

    /// Writes one record of the backend.
    pub fn write_record(&self, level: Level, origin: &str, message: &str) {
        self.write(None, level, origin, message);
    }

    /// Writes one record of the interface, if `session` is the token of the vault that is
    /// attached now; a message queued for an earlier vault is dropped (FR-007).
    pub fn write_ui(&self, session: &str, level: Level, message: &str) {
        self.write(Some(session), level, UI_ORIGIN, message);
    }

    fn write(&self, session: Option<&str>, level: Level, origin: &str, message: &str) {
        let outcome = {
            let mut guard = self.lock();
            let Some(attached) = guard.as_mut() else {
                return;
            };
            if session.is_some_and(|token| token != attached.session) {
                return;
            }
            if !attached.level.allows(origin, level) {
                return;
            }
            let line = format_record(now(), level, origin, message);
            attached.file.write_line(&line)
        };
        match outcome {
            Ok(()) => self.unwritable.store(false, Ordering::Release),
            Err(_) => {
                // Only the first failure after a success is announced, not every line.
                if !self.unwritable.swap(true, Ordering::AcqRel) {
                    if let Some(hook) = self.on_unwritable.get() {
                        hook();
                    }
                }
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Attached>> {
        self.attached.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for DiagnosticSink {
    fn default() -> Self {
        Self::new()
    }
}

impl Log for DiagnosticSink {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        match self.level.load(Ordering::Acquire) {
            STANDARD => LogLevel::Standard.allows(metadata.target(), metadata.level()),
            VERBOSE => LogLevel::Verbose.allows(metadata.target(), metadata.level()),
            _ => false,
        }
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            self.write_record(record.level(), record.target(), &record.args().to_string());
        }
    }

    fn flush(&self) {
        if let Some(attached) = self.lock().as_mut() {
            let _ = attached.file.flush();
        }
    }
}

fn code(level: LogLevel) -> u8 {
    match level {
        LogLevel::Standard => STANDARD,
        LogLevel::Verbose => VERBOSE,
    }
}

fn now() -> DateTime {
    jiff::Zoned::now().datetime()
}

/// `[2026-11-21][07:02:02.123][WARN][origin] message`. A message of several lines (a stack) stays
/// one record: every line after the first starts with four spaces (see `contracts/log-format.md`).
pub fn format_record(at: DateTime, level: Level, origin: &str, message: &str) -> String {
    let mut text = message.replace("\r\n", "\n").replace('\r', "\n");
    if text.len() > MAX_RECORD {
        let mut end = MAX_RECORD;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str(CUT);
    }
    let text = text.replace('\n', "\n    ");
    format!(
        "[{:04}-{:02}-{:02}][{:02}:{:02}:{:02}.{:03}][{}][{origin}] {text}",
        at.year(),
        at.month(),
        at.day(),
        at.hour(),
        at.minute(),
        at.second(),
        at.subsec_nanosecond() / 1_000_000,
        level.as_str(),
    )
}

#[cfg(test)]
#[path = "sink_tests.rs"]
mod tests;
