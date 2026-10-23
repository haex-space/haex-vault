//! A question to the user that Rust waits for (spec 049, plan K1, research R10): the question goes
//! out through an emitter, the caller parks on a oneshot until the window answers, the time runs
//! out or a token cancels it. The bridge's questions, the storage and contacts questions of
//! `files/agent/prompt.rs` and the action calls of `chat/tools/action_bridge.rs` all park here,
//! each with its own types (T080).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Why a question ended without an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptEnd {
    /// No window could hear it.
    NoWindow,
    /// Nobody answered in time.
    Expired,
    /// The token fired, or [`PromptPark::cancel_all`] ran.
    Cancelled,
}

/// Delivers a question with its id to the window; `false` when no window could hear it.
pub type Emitter<Q> = Arc<dyn Fn(&str, &Q) -> bool + Send + Sync>;

struct Inner<Q, A> {
    emitter: Mutex<Option<Emitter<Q>>>,
    pending: Mutex<HashMap<String, oneshot::Sender<A>>>,
    timeout: Duration,
}

/// The parked questions of one kind. Cheap to clone.
pub struct PromptPark<Q, A> {
    inner: Arc<Inner<Q, A>>,
}

impl<Q, A> Clone for PromptPark<Q, A> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<Q, A: Send> PromptPark<Q, A> {
    pub fn new(timeout: Duration) -> Self {
        Self {
            inner: Arc::new(Inner {
                emitter: Mutex::new(None),
                pending: Mutex::new(HashMap::new()),
                timeout,
            }),
        }
    }

    /// Set once the window exists (from `setup()`), or by a test.
    pub fn set_emitter(&self, emitter: Emitter<Q>) {
        *self
            .inner
            .emitter
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(emitter);
    }

    /// Set the emitter only when no emitter has been set yet.
    pub fn set_emitter_once(&self, emitter: Emitter<Q>) -> bool {
        let mut configured = self
            .inner
            .emitter
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if configured.is_some() {
            return false;
        }
        *configured = Some(emitter);
        true
    }

    /// Whether a window can be asked yet.
    pub fn has_emitter(&self) -> bool {
        self.inner
            .emitter
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// Asks and waits. `on_id` learns the id before the question goes out, so a caller can tie it
    /// to its own state (a cancel event, a dedupe map).
    pub async fn ask(
        &self,
        question: &Q,
        cancel: &CancellationToken,
        on_id: impl FnOnce(&str),
    ) -> Result<A, PromptEnd> {
        let emitter = self
            .inner
            .emitter
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or(PromptEnd::NoWindow)?;
        let id = Uuid::new_v4().to_string();
        let (sender, receiver) = oneshot::channel();
        self.pending().insert(id.clone(), sender);
        on_id(&id);
        let result = if emitter(&id, question) {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => Err(PromptEnd::Cancelled),
                answer = receiver => answer.map_err(|_| PromptEnd::Cancelled),
                _ = tokio::time::sleep(self.inner.timeout) => Err(PromptEnd::Expired),
            }
        } else {
            Err(PromptEnd::NoWindow)
        };
        self.pending().remove(&id);
        result
    }

    /// The window's answer; `false` for an unknown or already answered id.
    pub fn answer(&self, id: &str, answer: A) -> bool {
        match self.pending().remove(id) {
            Some(sender) => sender.send(answer).is_ok(),
            None => false,
        }
    }

    /// Ends every waiting question as [`PromptEnd::Cancelled`].
    pub fn cancel_all(&self) {
        self.pending().clear();
    }

    /// How many questions wait.
    pub fn waiting(&self) -> usize {
        self.pending().len()
    }

    fn pending(&self) -> MutexGuard<'_, HashMap<String, oneshot::Sender<A>>> {
        self.inner
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "prompt_park_tests.rs"]
mod tests;
