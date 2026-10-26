//! The pause of a transfer: the window sets it, the copy loops wait on it between two chunks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

/// How often a paused transfer looks whether it goes on or was cancelled.
const POLL: Duration = Duration::from_millis(50);

/// Shared between the manager and the thread of one transfer.
#[derive(Clone, Default)]
pub struct Pause(Arc<AtomicBool>);

impl Pause {
    pub fn set(&self, paused: bool) {
        self.0.store(paused, Ordering::Release);
    }

    /// Blocks while paused; returns at once on a cancel, which the caller's own check ends.
    pub fn wait(&self, cancel: &CancellationToken) {
        while self.0.load(Ordering::Acquire) && !cancel.is_cancelled() {
            std::thread::sleep(POLL);
        }
    }
}
