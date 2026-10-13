//! A frame for an agent's call (spec 018, research R9): Rust asks holzi's window with the event
//! `extension-agent-frame`; the window uses an open tab of the extension in the active workspace or
//! opens one minimized, and answers with `extension_agent_frame_ready` once the frame's port is
//! ready, or with `null`.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures::channel::oneshot;
use serde_json::json;
use uuid::Uuid;

use crate::extensions::bridge::dispatch::Emit;

/// Event: holzi's window shall provide a frame of the extension.
pub const AGENT_FRAME: &str = "extension-agent-frame";

/// How long a call waits for the window's answer: the window's own limit (10 s) plus room for its
/// answer to arrive.
pub const AGENT_FRAME_WAIT: Duration = Duration::from_secs(12);

/// Requests waiting for holzi's window.
#[derive(Default)]
pub struct FrameRequests {
    pending: Mutex<HashMap<String, oneshot::Sender<Option<String>>>>,
}

impl FrameRequests {
    fn pending(&self) -> MutexGuard<'_, HashMap<String, oneshot::Sender<Option<String>>>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Asks for a frame of `extension_id`: its id once ready, `None` when the window could not
    /// provide one or did not answer within `wait`.
    pub async fn request(
        &self,
        emit: &impl Emit,
        extension_id: Uuid,
        wait: Duration,
    ) -> Option<String> {
        let request_id = Uuid::new_v4().to_string();
        let (answer, answered) = oneshot::channel();
        self.pending().insert(request_id.clone(), answer);
        emit.emit(
            AGENT_FRAME,
            json!({ "extensionId": extension_id.to_string(), "requestId": request_id }),
        );
        let frame = tokio::time::timeout(wait, answered).await;
        self.pending().remove(&request_id);
        frame.ok().and_then(Result::ok).flatten()
    }

    /// The window's answer; `false` for a request nobody waits for any more.
    pub fn answer(&self, request_id: &str, frame: Option<String>) -> bool {
        match self.pending().remove(request_id) {
            Some(waiting) => waiting.send(frame).is_ok(),
            None => false,
        }
    }
}
