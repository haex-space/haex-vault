//! The one downloaded bundle that waits in the host for its install preview (spec 069, FR-010,
//! research R6). The bytes never touch the disk and never travel to the webview: the preview and
//! the install read them from here (`BundleSource::Marketplace`).
//!
//! A bundle waits for [`super::PENDING_IDLE`] without being used, then it is dropped. A closing
//! vault drops it as well, and so does every new download: there is only one preview at a time.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::commands::refused;
use super::kinds;
use crate::error::Result;

/// A downloaded and checked bundle and where it came from.
#[derive(Debug, PartialEq, Eq)]
pub struct PendingBundle {
    pub source_id: String,
    /// The name of the marketplace as the user knows it, shown in the install preview (FR-011).
    pub source_name: String,
    pub slug: String,
    pub version: String,
    pub bytes: Arc<Vec<u8>>,
}

struct Entry {
    id: Uuid,
    bundle: Arc<PendingBundle>,
    last_used: Instant,
    /// Cancelled when the entry leaves the slot, whatever the reason; ends its expiry task.
    gone: CancellationToken,
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.gone.cancel();
    }
}

/// The slot: at most one bundle.
#[derive(Default)]
pub struct PendingSlot {
    entry: Mutex<Option<Entry>>,
}

impl PendingSlot {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Entry>> {
        // A poisoned lock only means another thread panicked while holding it; the slot itself
        // is always in a usable state.
        self.entry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Puts `bundle` under `id`, dropping the one that waited before. The bundle expires after
    /// `idle` without use, or when `closing` fires, whichever comes first. Needs a tokio runtime.
    pub fn put(
        self: &Arc<Self>,
        id: Uuid,
        bundle: PendingBundle,
        idle: Duration,
        closing: CancellationToken,
    ) {
        let gone = CancellationToken::new();
        *self.lock() = Some(Entry {
            id,
            bundle: Arc::new(bundle),
            last_used: Instant::now(),
            gone: gone.clone(),
        });
        let slot = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                let Some(deadline) = slot.last_used(id).map(|used| used + idle) else {
                    return;
                };
                tokio::select! {
                    biased;
                    () = gone.cancelled() => return,
                    () = closing.cancelled() => {
                        slot.take(&id.to_string());
                        return;
                    }
                    () = tokio::time::sleep_until(deadline) => {}
                }
                if slot.expire_if_idle(id, idle) {
                    return;
                }
            }
        });
    }

    fn last_used(&self, id: Uuid) -> Option<Instant> {
        self.lock()
            .as_ref()
            .filter(|entry| entry.id == id)
            .map(|entry| entry.last_used)
    }

    fn expire_if_idle(&self, id: Uuid, idle: Duration) -> bool {
        let mut guard = self.lock();
        let expired = guard
            .as_ref()
            .is_some_and(|entry| entry.id == id && entry.last_used.elapsed() >= idle);
        if expired {
            *guard = None;
        }
        expired
    }

    /// The bundle under `id` for the install preview; counts as use. A slot that is empty, holds
    /// another bundle or has expired is `marketplace_pending_missing`.
    pub fn peek(&self, id: &str) -> Result<Arc<PendingBundle>> {
        let id = Uuid::parse_str(id).map_err(|_| refused(kinds::PENDING_MISSING))?;
        let mut guard = self.lock();
        match guard.as_mut().filter(|entry| entry.id == id) {
            Some(entry) => {
                entry.last_used = Instant::now();
                Ok(Arc::clone(&entry.bundle))
            }
            None => Err(refused(kinds::PENDING_MISSING)),
        }
    }

    /// Removes and returns the bundle under `id`: after a successful install, or when the preview
    /// is closed without installing.
    pub fn take(&self, id: &str) -> Option<Arc<PendingBundle>> {
        let id = Uuid::parse_str(id).ok()?;
        let mut guard = self.lock();
        if guard.as_ref().is_some_and(|entry| entry.id == id) {
            return guard.take().map(|entry| Arc::clone(&entry.bundle));
        }
        None
    }

    /// Whether any bundle waits.
    pub fn is_empty(&self) -> bool {
        self.lock().is_none()
    }
}

#[cfg(test)]
#[path = "pending_tests.rs"]
mod tests;
