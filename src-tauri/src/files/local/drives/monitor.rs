//! Keeps the drive list of the open file browser windows live (spec 064 FR-004, research R4, R8).
//! While at least one window listens, a thread asks the backend every few seconds and sends the
//! whole list when it changed; with the last listener gone, the thread ends and nothing is asked.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

use super::backend::DriveBackend;
use super::{current, Drive};

/// The drive list after a change; always the whole list, so a window cannot miss a step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct DrivesChanged {
    pub drives: Vec<Drive>,
}

/// Where a listener receives the list; `false` when it is gone and should be dropped.
pub type DriveSink = Box<dyn Fn(&DrivesChanged) -> bool + Send>;

/// Whether `next` differs from `prev` in anything but the free space, which changes with every
/// write and alone is no reason to send.
pub fn changed(prev: &[Drive], next: &[Drive]) -> bool {
    prev.len() != next.len()
        || prev.iter().zip(next).any(|(a, b)| {
            Drive {
                free: None,
                ..a.clone()
            } != Drive {
                free: None,
                ..b.clone()
            }
        })
}

#[derive(Clone)]
pub struct DriveMonitor {
    inner: Arc<Inner>,
}

struct Inner {
    backend: Arc<dyn DriveBackend>,
    interval: Duration,
    state: Mutex<State>,
    wake: Condvar,
}

#[derive(Default)]
struct State {
    sinks: HashMap<u64, DriveSink>,
    next_id: u64,
    last: Vec<Drive>,
    /// Bumped whenever a polling thread starts or must stop; a thread runs while it holds the
    /// current one.
    generation: u64,
    running: bool,
}

impl DriveMonitor {
    pub fn new(backend: Arc<dyn DriveBackend>, interval: Duration) -> Self {
        Self {
            inner: Arc::new(Inner {
                backend,
                interval,
                state: Mutex::new(State::default()),
                wake: Condvar::new(),
            }),
        }
    }

    /// Adds a listener and sends it the current list at once. Blocks on the backend; call it off
    /// the async executor.
    pub fn subscribe(&self, sink: DriveSink) -> u64 {
        let drives = current(&*self.inner.backend);
        let mut state = self.inner.lock();
        state.last = drives.clone();
        let id = state.next_id;
        state.next_id += 1;
        if sink(&DrivesChanged { drives }) {
            state.sinks.insert(id, sink);
        }
        if !state.running && !state.sinks.is_empty() {
            state.running = true;
            state.generation += 1;
            let generation = state.generation;
            let inner = Arc::clone(&self.inner);
            let spawned = std::thread::Builder::new()
                .name("haex-drives".to_owned())
                .spawn(move || inner.poll(generation));
            if let Err(error) = spawned {
                log::error!("files: the drive monitor did not start: {error}");
                state.running = false;
            }
        }
        id
    }

    pub fn unsubscribe(&self, id: u64) {
        let mut state = self.inner.lock();
        state.sinks.remove(&id);
        self.inner.stop_if_idle(&mut state);
    }

    /// Drops every listener, as on a reload of the window.
    pub fn unsubscribe_all(&self) {
        let mut state = self.inner.lock();
        state.sinks.clear();
        self.inner.stop_if_idle(&mut state);
    }

    #[cfg(test)]
    fn is_running(&self) -> bool {
        self.inner.lock().running
    }
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn stop_if_idle(&self, state: &mut State) {
        if state.sinks.is_empty() && state.running {
            state.running = false;
            state.generation += 1;
            self.wake.notify_all();
        }
    }

    fn poll(&self, generation: u64) {
        loop {
            {
                let state = self.lock();
                let (state, _) = self
                    .wake
                    .wait_timeout_while(state, self.interval, |state| {
                        state.generation == generation
                    })
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if state.generation != generation {
                    return;
                }
            }
            // The backend is asked without the lock, so subscribing never waits for it.
            let drives = current(&*self.backend);
            let mut state = self.lock();
            if state.generation != generation {
                return;
            }
            let send = changed(&state.last, &drives);
            state.last = drives.clone();
            if send {
                let message = DrivesChanged { drives };
                state.sinks.retain(|_, sink| sink(&message));
                self.stop_if_idle(&mut state);
            }
        }
    }
}

#[cfg(test)]
#[path = "monitor_tests.rs"]
mod tests;
