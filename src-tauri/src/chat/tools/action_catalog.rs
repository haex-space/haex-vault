//! Every action an agent may call, as the frontend last pushed it (spec 021 research R4, ADR-0006).
//! The chat registers only the actions the built-in agent may call; the MCP server offers external
//! agents from this full list, so both entrances share one definition per action. A version counter
//! tells the MCP server when the list changed, so it can tell connected agents (FR-017).

use std::sync::{Arc, PoisonError, RwLock};

use tokio::sync::watch;

use super::action_tool::AgentActionDef;

/// The pushed definitions. Cheap to clone.
#[derive(Clone)]
pub struct ActionCatalog {
    defs: Arc<RwLock<Arc<Vec<AgentActionDef>>>>,
    version: Arc<watch::Sender<u64>>,
}

impl Default for ActionCatalog {
    fn default() -> Self {
        let (version, _) = watch::channel(0);
        Self {
            defs: Arc::new(RwLock::new(Arc::new(Vec::new()))),
            version: Arc::new(version),
        }
    }
}

impl ActionCatalog {
    /// Replaces the list and moves the version on.
    pub fn replace(&self, defs: Vec<AgentActionDef>) {
        *self.defs.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(defs);
        self.version
            .send_modify(|version| *version = version.wrapping_add(1));
    }

    /// The list as it is now.
    pub fn snapshot(&self) -> Arc<Vec<AgentActionDef>> {
        Arc::clone(&self.defs.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Changes with every [`Self::replace`].
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version.subscribe()
    }
}
