//! What a run reports while it works (the window shows it as a bar): the phase, the files copied of
//! the planned ones, and the bytes. The reports go out through a sink the session provides, so the
//! run does not know about the window; copies report often, so they are thinned out.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use ts_rs::TS;

/// The event the window hears while a rule runs.
pub const SYNC_RULE_PROGRESS: &str = "sync-rule-progress";

/// The shortest time between two reports of bytes of one file.
const BYTES_EVERY: Duration = Duration::from_millis(150);

/// Where a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRulePhase {
    /// Both places are listed and compared.
    Scanning,
    /// The planned files are copied.
    Copying,
}

/// The progress of one run of a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRuleProgress {
    pub rule_id: String,
    pub phase: SyncRulePhase,
    pub files_done: u32,
    pub files_total: u32,
    #[ts(type = "number")]
    pub bytes_done: u64,
    #[ts(type = "number")]
    pub bytes_total: u64,
}

type Sink = Arc<dyn Fn(&SyncRuleProgress) + Send + Sync>;

struct Inner {
    phase: SyncRulePhase,
    files_done: u32,
    files_total: u32,
    /// Bytes of the files already copied.
    bytes_before: u64,
    /// Bytes of the file being copied.
    bytes_current: u64,
    bytes_total: u64,
    last_bytes_report: Option<Instant>,
}

/// Reports the progress of one run. Cheap to clone, so a copy on a blocking thread can report.
#[derive(Clone)]
pub struct Reporter {
    rule_id: String,
    sink: Option<Sink>,
    inner: Arc<Mutex<Inner>>,
}

impl Reporter {
    /// A reporter of the run of `rule_id`; every report goes to `sink`.
    pub fn new(rule_id: &str, sink: impl Fn(&SyncRuleProgress) + Send + Sync + 'static) -> Self {
        Self::with_sink(rule_id, Some(Arc::new(sink)))
    }

    /// A reporter nobody hears.
    pub fn silent() -> Self {
        Self::with_sink("", None)
    }

    fn with_sink(rule_id: &str, sink: Option<Sink>) -> Self {
        Self {
            rule_id: rule_id.to_owned(),
            sink,
            inner: Arc::new(Mutex::new(Inner {
                phase: SyncRulePhase::Scanning,
                files_done: 0,
                files_total: 0,
                bytes_before: 0,
                bytes_current: 0,
                bytes_total: 0,
                last_bytes_report: None,
            })),
        }
    }

    fn report(&self, inner: &Inner) {
        if let Some(sink) = &self.sink {
            sink(&SyncRuleProgress {
                rule_id: self.rule_id.clone(),
                phase: inner.phase,
                files_done: inner.files_done,
                files_total: inner.files_total,
                bytes_done: (inner.bytes_before + inner.bytes_current).min(inner.bytes_total),
                bytes_total: inner.bytes_total,
            });
        }
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The run starts listing the places.
    pub fn scanning(&self) {
        let inner = self.inner();
        self.report(&inner);
    }

    /// The plan is built: `files` are to be copied, `bytes` in all.
    pub fn planned(&self, files: u32, bytes: u64) {
        let mut inner = self.inner();
        inner.phase = SyncRulePhase::Copying;
        inner.files_total = files;
        inner.bytes_total = bytes;
        self.report(&inner);
    }

    /// `bytes` of the file being copied are done.
    pub fn bytes(&self, bytes: u64) {
        let mut inner = self.inner();
        inner.bytes_current = bytes;
        let now = Instant::now();
        if inner
            .last_bytes_report
            .is_some_and(|last| now.duration_since(last) < BYTES_EVERY)
        {
            return;
        }
        inner.last_bytes_report = Some(now);
        self.report(&inner);
    }

    /// The file being copied, of `size` bytes, is done.
    pub fn file_done(&self, size: u64) {
        let mut inner = self.inner();
        inner.files_done = inner.files_done.saturating_add(1);
        inner.bytes_before = inner.bytes_before.saturating_add(size);
        inner.bytes_current = 0;
        inner.last_bytes_report = None;
        self.report(&inner);
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
