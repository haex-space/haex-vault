//! The open connections of a running server (spec 021 US7, FR-017, FR-027): what the window changes
//! about an agent reaches its connections at once. New rights or a new approval mode apply from the
//! next call and send `notifications/tools/list_changed`; a revocation ends the connection after the
//! control line `closing: revoked`, and its waiting prompts and calls with it.

use std::sync::{Arc, Mutex, PoisonError};

use rmcp::service::{Peer, RoleServer};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::grants::Rights;
use super::hello::CloseReason;
use super::registry::AgentMode;
use super::session::Session;

/// How one connection ends from holzi's side. Its token is a child of the server's, so it also
/// fires when the vault closes or the access is turned off.
pub struct Link {
    pub close: CancellationToken,
    reason: Mutex<Option<CloseReason>>,
}

impl Link {
    pub fn new(close: CancellationToken) -> Self {
        Self {
            close,
            reason: Mutex::new(None),
        }
    }

    /// Ends the connection with `reason`; the first reason stays.
    pub fn end(&self, reason: CloseReason) {
        self.reason
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert(reason);
        self.close.cancel();
    }

    /// Why [`Self::end`] ended it; `None` when the server's token did.
    pub fn reason(&self) -> Option<CloseReason> {
        *self.reason.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct Entry {
    id: u64,
    session: Session,
    peer: Peer<RoleServer>,
}

/// The connections of one running server.
#[derive(Default)]
pub struct Live {
    next: Mutex<u64>,
    entries: Arc<Mutex<Vec<Entry>>>,
}

/// Removes its connection from [`Live`] when dropped.
pub struct LiveGuard {
    id: u64,
    entries: Arc<Mutex<Vec<Entry>>>,
}

impl Drop for LiveGuard {
    fn drop(&mut self) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|entry| entry.id != self.id);
    }
}

impl Live {
    /// Adds a connection; it counts for an agent once its session knows the agent.
    pub fn add(&self, session: Session, peer: Peer<RoleServer>) -> LiveGuard {
        let id = {
            let mut next = self.next.lock().unwrap_or_else(PoisonError::into_inner);
            *next += 1;
            *next
        };
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Entry { id, session, peer });
        LiveGuard {
            id,
            entries: Arc::clone(&self.entries),
        }
    }

    fn of(&self, agent: Uuid) -> Vec<(Session, Peer<RoleServer>)> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|entry| entry.session.agent().is_some_and(|known| known.id == agent))
            .map(|entry| (entry.session.clone(), entry.peer.clone()))
            .collect()
    }

    /// Whether `agent` holds a connection now.
    pub fn connected(&self, agent: Uuid) -> bool {
        !self.of(agent).is_empty()
    }

    /// Gives the connections of `agent` new rights and tells them their tool list changed.
    pub async fn set_rights(&self, agent: Uuid, rights: &Rights) {
        for (session, peer) in self.of(agent) {
            session.update_agent(|known| known.rights = rights.clone());
            let _ = peer.notify_tool_list_changed().await;
        }
    }

    /// Gives the connections of `agent` a new approval mode, from their next call on.
    pub fn set_mode(&self, agent: Uuid, mode: AgentMode) {
        for (session, _) in self.of(agent) {
            session.update_agent(|known| known.mode = mode);
        }
    }

    /// Ends every connection of `agent` with `reason`.
    pub fn close(&self, agent: Uuid, reason: CloseReason) {
        for (session, _) in self.of(agent) {
            session.link().end(reason);
        }
    }
}
