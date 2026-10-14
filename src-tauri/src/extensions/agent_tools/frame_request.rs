//! A frame for an agent's call (spec 018, research R9): Rust asks holzi's window with the event
//! `extension-agent-frame`; the window uses an open tab of the extension in the active workspace or
//! opens one minimized, and answers with `extension_agent_frame_ready` once the frame's port is
//! ready, or with `null`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures::channel::oneshot;
use serde_json::json;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use super::commands::to_frame;
use super::slot::McpLink;
use super::tool::Links;
use crate::extensions::bridge::dispatch::Emit;
use crate::extensions::bridge::frames::FrameSession;
use crate::extensions::commands::frames::WindowEmitter;
use crate::state::AppState;

/// Event: holzi's window shall provide a frame of the extension.
pub const AGENT_FRAME: &str = "extension-agent-frame";

/// How long a call waits for the window's answer: the window's own limit (10 s) plus room for its
/// answer to arrive.
pub const AGENT_FRAME_WAIT: Duration = Duration::from_secs(12);

/// How long the MCP handshake with a ready frame may take (research R9); `connect` has no limit
/// of its own.
pub const LINK_WAIT: Duration = Duration::from_secs(10);

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

/// The agent's way to an extension's frame in holzi's window: a frame from the window, then the
/// frame session's MCP connection.
pub struct WindowLinks(pub AppHandle);

#[async_trait]
impl Links for WindowLinks {
    async fn open(&self, extension_id: Uuid) -> Result<(Arc<FrameSession>, Arc<McpLink>), String> {
        let host = self.0.state::<AppState>().extensions();
        let frame = host
            .agent_frames
            .request(
                &WindowEmitter(self.0.clone()),
                extension_id,
                AGENT_FRAME_WAIT,
            )
            .await
            .ok_or("no frame of the extension could be opened")?;
        let session = host
            .frames
            .get(&frame)
            .filter(|session| session.extension_id == extension_id)
            .ok_or("the frame of the extension closed")?;
        let send = to_frame(WindowEmitter(self.0.clone()), frame);
        let link = tokio::time::timeout(LINK_WAIT, session.mcp.link(send))
            .await
            .map_err(|_| "the extension did not connect in time".to_owned())?
            .map_err(|error| error.to_string())?;
        Ok((session, link))
    }
}
