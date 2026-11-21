//! The one place log records are written to a vault's folder (spec 098; research R1).
//!
//! The sink is a `log::Log` behind a swappable writer. With no writer attached it drops every
//! record: nothing is written, kept or buffered before a vault is unlocked (FR-008). With one it
//! formats the line, checks the level and writes it, and it never touches a file outside the
//! folder it was attached to (FR-007). A failing disk never fails the app: the line is dropped,
//! the sink remembers it (FR-016) and tries again on the next record.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::{self, JoinHandle};

use jiff::civil::DateTime;
use log::{Level, Log, Metadata, Record};
use uuid::Uuid;

use super::level::LogLevel;
use super::rotation::{Limits, RotatingFile};

/// The origin of a record the interface wrote.
pub const UI_ORIGIN: &str = "ui";
/// The longest record, in bytes; a longer text is cut (an error text that carries a page).
pub const MAX_RECORD: usize = 16 * 1024;
const CUT: &str = " …[cut]";

const STANDARD: u8 = 0;
const VERBOSE: u8 = 1;
const DETACHED: u8 = 2;
const QUEUE_CAPACITY: usize = 512;

struct Attached {
    level: LogLevel,
    session: String,
}

struct Shared {
    unwritable: AtomicBool,
    on_unwritable: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

enum WriterCommand {
    Attach {
        folder: std::path::PathBuf,
        limits: Limits,
        session: String,
        reply: SyncSender<std::io::Result<()>>,
    },
    SetLimits {
        limits: Limits,
        session: String,
        reply: SyncSender<std::io::Result<()>>,
    },
    Write {
        session: String,
        line: String,
    },
    Flush(Option<SyncSender<std::io::Result<()>>>),
    Detach(SyncSender<std::io::Result<()>>),
    Shutdown,
}

struct Writer {
    sender: SyncSender<WriterCommand>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Writer {
    fn drop(&mut self) {
        let _ = self.sender.send(WriterCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub struct DiagnosticSink {
    attached: Mutex<Option<Attached>>,
    /// The level as the cheap check of `enabled`, or `DETACHED`.
    level: AtomicU8,
    writer: Mutex<Option<Writer>>,
    shared: Arc<Shared>,
}

impl DiagnosticSink {
    pub fn new() -> Self {
        Self {
            attached: Mutex::new(None),
            level: AtomicU8::new(DETACHED),
            writer: Mutex::new(None),
            shared: Arc::new(Shared {
                unwritable: AtomicBool::new(false),
                on_unwritable: OnceLock::new(),
            }),
        }
    }

    /// Runs `hook` when a write fails after the earlier ones went through. It must not log.
    pub fn on_unwritable(&self, hook: impl Fn() + Send + Sync + 'static) {
        // The first hook stays: the app handle it holds is the same for every vault.
        let _ = self.shared.on_unwritable.set(Box::new(hook));
    }

    /// Starts writing into `folder`, at `level`, from now on; what an earlier folder held is not
    /// touched again. Returns the token of this attachment. A failure leaves the sink detached.
    pub fn attach(&self, folder: &Path, level: LogLevel) -> std::io::Result<String> {
        if let Err(error) = self.ensure_writer() {
            self.failed();
            return Err(error);
        }
        self.lock().take();
        self.level.store(DETACHED, Ordering::Release);
        let session = Uuid::new_v4().to_string();
        let (reply, result) = mpsc::sync_channel(1);
        if self
            .sender()?
            .send(WriterCommand::Attach {
                folder: folder.to_path_buf(),
                limits: level.limits(),
                session: session.clone(),
                reply,
            })
            .is_err()
        {
            self.failed();
            return Err(channel_error(mpsc::RecvError));
        }
        if let Err(error) = result.recv().map_err(channel_error)? {
            self.failed();
            return Err(error);
        }
        *self.lock() = Some(Attached {
            level,
            session: session.clone(),
        });
        self.level.store(code(level), Ordering::Release);
        self.shared.unwritable.store(false, Ordering::Release);
        Ok(session)
    }

    /// Flushes and stops writing; later records are dropped again.
    pub fn detach(&self) {
        self.level.store(DETACHED, Ordering::Release);
        self.lock().take();
        let Ok(sender) = self.sender() else {
            return;
        };
        let (reply, result) = mpsc::sync_channel(1);
        let Ok(()) = sender.send(WriterCommand::Detach(reply)) else {
            self.failed();
            return;
        };
        if result.recv().map_or(true, |result| result.is_err()) {
            self.failed();
        }
    }

    /// Changes the level of the attached log at once, and the size it may take with it.
    pub fn set_level(&self, level: LogLevel) -> std::io::Result<()> {
        let mut guard = self.lock();
        let Some(attached) = guard.as_mut() else {
            return Ok(());
        };
        attached.level = level;
        let session = attached.session.clone();
        self.level.store(code(level), Ordering::Release);
        drop(guard);
        let sender = self.sender().inspect_err(|_| self.failed())?;
        let (reply, result) = mpsc::sync_channel(1);
        if sender
            .send(WriterCommand::SetLimits {
                limits: level.limits(),
                session,
                reply,
            })
            .is_err()
        {
            self.failed();
            return Err(channel_error(mpsc::RecvError));
        }
        match result.recv().map_err(channel_error)? {
            Ok(()) => Ok(()),
            Err(error) => {
                self.failed();
                Err(error)
            }
        }
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
        self.shared.unwritable.load(Ordering::Acquire)
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
        let session = {
            let guard = self.lock();
            let Some(attached) = guard.as_ref() else {
                return;
            };
            if session.is_some_and(|token| token != attached.session) {
                return;
            }
            if !attached.level.allows(origin, level) {
                return;
            }
            attached.session.clone()
        };
        let line = format_record(now(), level, origin, message);
        let sender = self
            .writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|writer| writer.sender.clone());
        if let Some(sender) = sender {
            match sender.try_send(WriterCommand::Write { session, line }) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => self.failed(),
            }
        }
    }

    /// Waits for queued records to finish. Used by tests that inspect the file directly.
    #[cfg(test)]
    pub(crate) fn flush_sync(&self) {
        let Ok(sender) = self.sender() else { return };
        let (reply, result) = mpsc::sync_channel(1);
        if sender.send(WriterCommand::Flush(Some(reply))).is_ok() {
            let _ = result.recv();
        }
    }

    fn ensure_writer(&self) -> std::io::Result<()> {
        let mut slot = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.is_some() {
            return Ok(());
        }
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let shared = Arc::clone(&self.shared);
        let thread = thread::Builder::new()
            .name("diagnostic-log-writer".into())
            .spawn(move || writer_loop(receiver, shared))?;
        *slot = Some(Writer {
            sender,
            thread: Some(thread),
        });
        Ok(())
    }

    fn sender(&self) -> std::io::Result<SyncSender<WriterCommand>> {
        self.ensure_writer()?;
        self.writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|writer| writer.sender.clone())
            .ok_or_else(|| std::io::Error::other("diagnostic log writer is unavailable"))
    }

    fn failed(&self) {
        if !self.shared.unwritable.swap(true, Ordering::AcqRel) {
            if let Some(hook) = self.shared.on_unwritable.get() {
                hook();
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
        let sender = self
            .writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|writer| writer.sender.clone());
        if let Some(sender) = sender {
            let _ = sender.try_send(WriterCommand::Flush(None));
        }
    }
}

fn writer_loop(receiver: Receiver<WriterCommand>, shared: Arc<Shared>) {
    let mut attached: Option<(RotatingFile, String)> = None;
    let failed = || {
        if !shared.unwritable.swap(true, Ordering::AcqRel) {
            if let Some(hook) = shared.on_unwritable.get() {
                hook();
            }
        }
    };
    while let Ok(command) = receiver.recv() {
        match command {
            WriterCommand::Attach {
                folder,
                limits,
                session,
                reply,
            } => {
                if let Some((mut file, _)) = attached.take() {
                    let _ = file.flush();
                }
                let result = match RotatingFile::open(&folder, limits) {
                    Ok(file) => {
                        attached = Some((file, session));
                        shared.unwritable.store(false, Ordering::Release);
                        Ok(())
                    }
                    Err(error) => {
                        failed();
                        Err(error)
                    }
                };
                let _ = reply.send(result);
            }
            WriterCommand::SetLimits {
                limits,
                session,
                reply,
            } => {
                let result = match attached.as_mut() {
                    Some((file, active)) if *active == session => file.set_limits(limits),
                    _ => Ok(()),
                };
                if result.is_ok() {
                    shared.unwritable.store(false, Ordering::Release);
                } else {
                    failed();
                }
                let _ = reply.send(result);
            }
            WriterCommand::Write { session, line } => {
                if let Some((file, active)) = attached.as_mut() {
                    if *active == session {
                        if file.write_line(&line).is_ok() {
                            shared.unwritable.store(false, Ordering::Release);
                        } else {
                            failed();
                        }
                    }
                }
            }
            WriterCommand::Flush(reply) => {
                let result = attached.as_mut().map_or(Ok(()), |(file, _)| file.flush());
                if result.is_err() {
                    failed();
                }
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
            }
            WriterCommand::Detach(reply) => {
                let result = attached.take().map_or(Ok(()), |(mut file, _)| file.flush());
                if result.is_err() {
                    failed();
                }
                let _ = reply.send(result);
            }
            WriterCommand::Shutdown => {
                if let Some((mut file, _)) = attached.take() {
                    let _ = file.flush();
                }
                break;
            }
        }
    }
}

fn channel_error(_: std::sync::mpsc::RecvError) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "diagnostic log writer stopped",
    )
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
