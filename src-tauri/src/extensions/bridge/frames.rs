//! Frame sessions (research R13, R14): holzi's own window opens a session for every extension
//! frame it mounts. The session, never the extension, says which extension a bridge call comes
//! from. Sessions live in memory and end with the frame or the process (ADR-0003: one process, one
//! vault).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use uuid::Uuid;

use crate::extensions::agent_tools::slot::McpSlot;
use crate::extensions::protocol::token;

/// What a frame shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSource {
    /// A stored bundle, served by holzi's protocol.
    Bundle(Uuid),
    /// A development server on this device (US12); holzi serves nothing for it.
    DevServer,
}

impl FrameSource {
    /// The bundle of a frame that runs one.
    pub fn bundle(self) -> Option<Uuid> {
        match self {
            Self::Bundle(id) => Some(id),
            Self::DevServer => None,
        }
    }
}

#[derive(Debug)]
pub struct FrameSession {
    /// Random id the frontend uses for bridge calls; the extension never sees it.
    pub frame: String,
    pub extension_id: Uuid,
    pub source: FrameSource,
    pub tab_id: String,
    /// Start token in the frame URL; HTML of the extension is served only with it.
    pub token: String,
    pub opened_at: Instant,
    /// The agent's tool connection to this frame (spec 018).
    pub mcp: McpSlot,
}

#[derive(Default)]
pub struct FrameRegistry {
    sessions: Mutex<HashMap<String, Arc<FrameSession>>>,
}

impl FrameRegistry {
    fn sessions(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<FrameSession>>> {
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Opens the session of a frame that runs the stored bundle `bundle_id`.
    pub fn open(&self, extension_id: Uuid, bundle_id: Uuid, tab_id: &str) -> Arc<FrameSession> {
        self.open_with(extension_id, FrameSource::Bundle(bundle_id), tab_id)
    }

    /// Opens the session of a frame that shows a development server.
    pub fn open_dev(&self, extension_id: Uuid, tab_id: &str) -> Arc<FrameSession> {
        self.open_with(extension_id, FrameSource::DevServer, tab_id)
    }

    fn open_with(
        &self,
        extension_id: Uuid,
        source: FrameSource,
        tab_id: &str,
    ) -> Arc<FrameSession> {
        let session = Arc::new(FrameSession {
            frame: token::mint(),
            extension_id,
            source,
            tab_id: tab_id.to_owned(),
            token: token::mint(),
            opened_at: Instant::now(),
            mcp: McpSlot::default(),
        });
        self.sessions()
            .insert(session.frame.clone(), Arc::clone(&session));
        session
    }

    /// Ends a session and its tool connection.
    pub fn close(&self, frame: &str) -> Option<Arc<FrameSession>> {
        let closed = self.sessions().remove(frame);
        if let Some(session) = &closed {
            session.mcp.close();
        }
        closed
    }

    pub fn get(&self, frame: &str) -> Option<Arc<FrameSession>> {
        self.sessions().get(frame).cloned()
    }

    /// The session of `extension_id` whose start token is `given`.
    pub fn by_token(&self, extension_id: Uuid, given: &str) -> Option<Arc<FrameSession>> {
        self.sessions()
            .values()
            .find(|s| s.extension_id == extension_id && token::matches(&s.token, given))
            .cloned()
    }

    /// The extensions with at least one open frame.
    pub fn extensions(&self) -> Vec<Uuid> {
        let mut ids: Vec<Uuid> = self.sessions().values().map(|s| s.extension_id).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Every open frame of an extension, newest first.
    pub fn of_extension(&self, extension_id: Uuid) -> Vec<Arc<FrameSession>> {
        let mut frames: Vec<_> = self
            .sessions()
            .values()
            .filter(|s| s.extension_id == extension_id)
            .cloned()
            .collect();
        frames.sort_by_key(|s| std::cmp::Reverse(s.opened_at));
        frames
    }
}

#[cfg(test)]
#[path = "frames_tests.rs"]
mod tests;
