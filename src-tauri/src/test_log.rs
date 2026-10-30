//! A logger for tests that keeps every record, for searching it for secrets (spec 024 FR-002,
//! spec 050 SC-005). The logger is process-wide and other tests log into it too, which only makes
//! the search stricter.

use std::sync::{Mutex, Once};

static RECORDS: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());
static INSTALL: Once = Once::new();
struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        RECORDS.lock().unwrap_or_else(|e| e.into_inner()).push((
            record.level(),
            format!("{}: {}", record.target(), record.args()),
        ));
    }
    fn flush(&self) {}
}

/// Installs the logger once per process.
pub fn install() {
    INSTALL.call_once(|| {
        let _ = log::set_logger(&Capture);
        log::set_max_level(log::LevelFilter::Trace);
    });
}

/// Every record so far, of every level.
pub fn all() -> String {
    at_least(log::Level::Trace)
}

/// Every record whose target does not start with one of `targets`, for a test whose scripted
/// server logs what it receives (a secret a client sent is in the server's log, not the client's).
pub fn all_but(targets: &[&str]) -> String {
    RECORDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(_, line)| !targets.iter().any(|target| line.starts_with(target)))
        .map(|(_, line)| line.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The records of `level` and above, as a log at that level keeps them (holzi writes `Info` and
/// above, `lib.rs`).
pub fn at_least(level: log::Level) -> String {
    RECORDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(at, _)| *at <= level)
        .map(|(_, line)| line.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
