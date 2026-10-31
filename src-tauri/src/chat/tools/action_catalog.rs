//! Every action an agent may call, as the frontend last pushed it (spec 021 research R4, ADR-0006).
//! The chat registers only the actions the built-in agent may call; the MCP server offers external
//! agents from this full list, so both entrances share one definition per action. A version counter
//! tells the MCP server when the list changed, so it can tell connected agents (FR-017). The tools
//! of extensions are published the same way (spec 021 US4), from the chat's own registration.

use std::sync::{Arc, PoisonError, RwLock};

use tokio::sync::watch;

use super::action_tool::AgentActionDef;
use super::Tool;

/// A published list that tells its readers when it changed. Cheap to clone.
pub struct Catalog<T> {
    defs: Arc<RwLock<Arc<Vec<T>>>>,
    version: Arc<watch::Sender<u64>>,
}

/// The pushed action definitions.
pub type ActionCatalog = Catalog<AgentActionDef>;

/// The tools of extensions the chat registered (spec 018), for external agents (spec 021 US4).
pub type ExtensionToolCatalog = Catalog<Arc<dyn Tool>>;

impl<T> Clone for Catalog<T> {
    fn clone(&self) -> Self {
        Self {
            defs: Arc::clone(&self.defs),
            version: Arc::clone(&self.version),
        }
    }
}

impl<T> Default for Catalog<T> {
    fn default() -> Self {
        let (version, _) = watch::channel(0);
        Self {
            defs: Arc::new(RwLock::new(Arc::new(Vec::new()))),
            version: Arc::new(version),
        }
    }
}

impl<T> Catalog<T> {
    /// Replaces the list and moves the version on.
    pub fn replace(&self, defs: Vec<T>) {
        *self.defs.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(defs);
        self.version
            .send_modify(|version| *version = version.wrapping_add(1));
    }

    /// The list as it is now.
    pub fn snapshot(&self) -> Arc<Vec<T>> {
        Arc::clone(&self.defs.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Changes with every [`Self::replace`].
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version.subscribe()
    }
}
