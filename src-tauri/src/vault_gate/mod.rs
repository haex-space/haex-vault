//! The vault gate: the one owner of "may requests be answered, and what is still running"
//! (spec 013, data-model.md).
//!
//! One app process serves at most one vault session, and closing it ends the process. The gate
//! carries what that needs: the phase of the session, one cancellation token that fires when a
//! close starts, and one tracker that counts tracked tasks, blocking work and every live
//! [`VaultDb`]. The drain ladder ([`VaultGate::drain`]) waits for the tracker to empty, and the
//! invoke-handler wrapper ([`VaultGate::wrap`]) makes every command default-deny once a close
//! has started.

mod children;
mod db;
mod drain;
mod invoke;

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use haex_crdt::Database;
use tokio::runtime::Handle;
use tokio::sync::Notify;
use tokio::task::{AbortHandle, JoinHandle};
use tokio_util::sync::CancellationToken;
use tokio_util::task::task_tracker::TaskTrackerToken;
use tokio_util::task::TaskTracker;

use crate::error::{HaexError, Result};

pub(crate) use children::kill_process_tree;
pub use children::{ChildGuard, ChildRegistry};
pub use db::VaultDb;
pub use drain::{
    hard_end_after, on_plain_thread, DrainOutcome, COOPERATIVE_WINDOW, HARD_END_GRACE, TOTAL_LIMIT,
};
pub use invoke::APP_SCOPED_COMMANDS;

/// Where the app process is in its one-session life. It only moves forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultPhase {
    /// No vault is open yet; a failed unlock leaves the gate here.
    Idle,
    /// A vault session is running.
    Active,
    /// A close was requested; only the app-scoped allow-list is answered.
    Closing,
}

/// What ends the process after a close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosePolicy {
    /// Start the app again, which shows the unlock screen.
    Relaunch,
    /// Just end the process.
    Exit,
}

/// The policy for this build. Debug builds exit: under `pnpm tauri:dev`, a relaunch ends the whole
/// dev session instead of the dev runner re-attaching (research R2, task T048, confirmed
/// 2026-09-24). Desktop release builds relaunch. Android always exits: the next start shows the
/// vault picker in a new process (spec 043 FR-006).
pub fn close_policy() -> ClosePolicy {
    close_policy_for(crate::platform::capabilities())
}

/// The policy the capability table asks for.
pub fn close_policy_for(capabilities: crate::platform::PlatformCapabilities) -> ClosePolicy {
    if capabilities.relaunch_on_close {
        ClosePolicy::Relaunch
    } else {
        ClosePolicy::Exit
    }
}

/// The effects of a close that reach outside the process's own state, behind a trait so a test
/// can record them instead of changing a window or ending the test process (spec 013, T039).
/// Every method returns at once and none can fail: a failure is logged inside the implementation,
/// because a close that can be refused is exactly what this feature removes.
pub trait CloseEffects: Send + Sync + 'static {
    /// Replaces the page with the closing spinner, so nothing of the vault stays on screen.
    fn show_closing_page(&self);
    /// Tells listeners that the vault `name` was closed (`instance-list-changed`).
    fn announce_closed(&self, name: Option<String>);
    /// Asks the event loop to end the process the normal way, for `policy`.
    fn request_end(&self, policy: ClosePolicy);
    /// Ends the process now, for `policy`, whatever is still running: the last resort that runs on
    /// a plain thread when the normal end did not happen. It first ends every registered child
    /// process, because ending the process skips the `Drop`-based kills.
    fn force_end(&self, policy: ClosePolicy);
}

/// The gate. Cheap to clone; every clone is the same gate.
#[derive(Clone)]
pub struct VaultGate {
    inner: Arc<Inner>,
}

struct Inner {
    /// Held for phase reads, admission and transition, never across an await.
    phase: Mutex<VaultPhase>,
    /// Fires once, on the first transition into `Closing`.
    cancel: CancellationToken,
    /// Counts tracked tasks, blocking work and every live `VaultDb`.
    tasks: TaskTracker,
    /// Abort handles of tracked async tasks, used by the second rung of the drain.
    aborts: Mutex<Vec<AbortHandle>>,
    /// The child processes started for the vault, ended by the drain ladder and the forced end.
    children: ChildRegistry,
    /// Set by whoever gets to run the forced end, so it runs once however often it is armed.
    forced_end: AtomicBool,
    /// Runtime that runs tracked work. `None` means Tauri's own async runtime.
    runtime: Option<Handle>,
    /// Woken after every committed `VaultDb` write, so the sync service (spec 024) can tell
    /// connected devices about the new progress.
    sync_notify: Arc<Notify>,
}

pub(crate) struct Admission<'a> {
    gate: &'a VaultGate,
    _phase: MutexGuard<'a, VaultPhase>,
}

impl Admission<'_> {
    pub(crate) fn tracker_token(&self) -> TaskTrackerToken {
        self.gate.inner.tasks.token()
    }

    pub(crate) fn vault_db(&self, db: Arc<Database>) -> VaultDb {
        VaultDb::new(
            db,
            Arc::clone(&self.gate.inner.sync_notify),
            self.tracker_token(),
        )
    }

    pub(crate) fn spawn<F>(&self, fut: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let handle = self.gate.inner.tasks.spawn_on(fut, &self.gate.runtime());
        let mut aborts = self
            .gate
            .inner
            .aborts
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        aborts.retain(|abort| !abort.is_finished());
        aborts.push(handle.abort_handle());
        handle
    }

    pub(crate) fn spawn_blocking<F, R>(&self, work: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.gate
            .inner
            .tasks
            .spawn_blocking_on(work, &self.gate.runtime())
    }
}

impl VaultGate {
    /// A gate in `Idle` whose tracked work runs on Tauri's async runtime.
    pub fn new() -> Self {
        Self::build(None)
    }

    /// A gate whose tracked work runs on `runtime`, for tests that drive their own runtime.
    pub fn with_runtime(runtime: Handle) -> Self {
        Self::build(Some(runtime))
    }

    fn build(runtime: Option<Handle>) -> Self {
        Self {
            inner: Arc::new(Inner {
                phase: Mutex::new(VaultPhase::Idle),
                cancel: CancellationToken::new(),
                tasks: TaskTracker::new(),
                aborts: Mutex::new(Vec::new()),
                children: ChildRegistry::default(),
                forced_end: AtomicBool::new(false),
                runtime,
                sync_notify: Arc::new(Notify::new()),
            }),
        }
    }

    fn phase_lock(&self) -> MutexGuard<'_, VaultPhase> {
        // The guarded value is a plain `Copy` enum, so a poisoned lock holds no broken state.
        self.inner.phase.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The current phase.
    pub fn phase(&self) -> VaultPhase {
        *self.phase_lock()
    }

    /// Whether a close was requested.
    pub fn is_closing(&self) -> bool {
        self.phase() == VaultPhase::Closing
    }

    /// Whether an open or create may start now, without changing the phase.
    pub fn ensure_can_open(&self) -> Result<()> {
        match self.phase() {
            VaultPhase::Idle => Ok(()),
            VaultPhase::Active => Err(HaexError::VaultAlreadyActive),
            VaultPhase::Closing => Err(HaexError::VaultClosed),
        }
    }

    /// Moves `Idle` to `Active`. Called only after the database has opened, so a failed unlock
    /// leaves the gate in `Idle`. Exactly one of several racing callers succeeds.
    pub fn begin_session(&self) -> Result<()> {
        let mut phase = self.phase_lock();
        match *phase {
            VaultPhase::Idle => {
                *phase = VaultPhase::Active;
                Ok(())
            }
            VaultPhase::Active => Err(HaexError::VaultAlreadyActive),
            VaultPhase::Closing => Err(HaexError::VaultClosed),
        }
    }

    /// Starts the close: moves to `Closing`, fires the cancellation token and closes the tracker.
    /// Returns whether this call was the first; a repeated request changes nothing (FR-002).
    pub fn request_close(&self) -> bool {
        let mut phase = self.phase_lock();
        if *phase == VaultPhase::Closing {
            return false;
        }
        *phase = VaultPhase::Closing;
        self.inner.cancel.cancel();
        self.inner.tasks.close();
        true
    }

    /// The registry of child processes this gate ends when the close runs out of patience.
    pub fn children(&self) -> ChildRegistry {
        self.inner.children.clone()
    }

    /// Claims the forced end of the process. Only the first caller gets `true`, so however many
    /// deadlines are armed, the forced end runs once.
    pub fn claim_forced_end(&self) -> bool {
        !self.inner.forced_end.swap(true, Ordering::SeqCst)
    }

    /// The token that fires when the close starts.
    pub fn token(&self) -> CancellationToken {
        self.inner.cancel.clone()
    }

    /// A token that keeps the tracker non-empty until it is dropped.
    pub fn tracker_token(&self) -> Result<TaskTrackerToken> {
        self.with_admission(|admission| Ok(admission.tracker_token()))
    }

    /// Woken after every committed [`VaultDb::write`]; one wake-up may stand for several
    /// writes.
    pub fn sync_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.inner.sync_notify)
    }

    /// Wraps a database handle so the tracker counts it while any clone is alive.
    pub fn vault_db(&self, db: Arc<Database>) -> Result<VaultDb> {
        self.with_admission(|admission| Ok(admission.vault_db(db)))
    }

    /// Whether no tracked work and no `VaultDb` is alive.
    pub fn is_idle(&self) -> bool {
        self.inner.tasks.is_empty()
    }

    fn runtime(&self) -> Handle {
        match &self.inner.runtime {
            Some(handle) => handle.clone(),
            None => tauri::async_runtime::handle().inner().clone(),
        }
    }

    /// Runs `fut` as tracked, abortable session work.
    pub fn spawn<F>(&self, fut: F) -> Result<JoinHandle<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.with_admission(|admission| Ok(admission.spawn(fut)))
    }

    /// Runs `work` on the blocking pool, tracked. The drain waits for the closure itself,
    /// because aborting the outer task cannot stop a running thread.
    pub fn spawn_blocking<F, R>(&self, work: F) -> Result<JoinHandle<R>>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.with_admission(|admission| Ok(admission.spawn_blocking(work)))
    }

    /// Races `fut` against the close: the future is dropped and `VaultClosed` returned once the
    /// close has started, even if its result was ready at the same moment (FR-001).
    pub async fn run<F: Future>(&self, fut: F) -> Result<F::Output> {
        self.with_admission(|_| Ok(()))?;
        tokio::select! {
            biased;
            _ = self.inner.cancel.cancelled() => Err(HaexError::VaultClosed),
            out = fut => Ok(out),
        }
    }

    pub(crate) fn with_admission<T>(
        &self,
        operation: impl FnOnce(&Admission<'_>) -> Result<T>,
    ) -> Result<T> {
        let phase = self.phase_lock();
        if *phase == VaultPhase::Closing {
            return Err(HaexError::VaultClosed);
        }
        operation(&Admission {
            gate: self,
            _phase: phase,
        })
    }
}

impl Default for VaultGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod gate_tests;
