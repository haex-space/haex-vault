//! The browser sessions of all agents (spec 065, data-model.md "Browser-Sitzung", FR-015,
//! FR-016): one per agent, shared by the browser tools and holzi's window. The window lists them
//! in the sessions bar and stops them; a stop ends the running call within a second, gives the
//! tabs back to the user and refuses the agent's further browser calls with `stopped` until it
//! begins a new session: an external agent with a new connection, the built-in agent with the
//! user's next chat message ([`BrowserSessions::resume`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, Weak};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::session::Session;
use super::tab::OnStop;
use crate::caller::Caller;

/// The window hears that the sessions changed (`agent-browser-sessions-changed`).
pub const EVENT_SESSIONS_CHANGED: &str = "agent-browser-sessions-changed";

/// Tells the window that the sessions changed.
pub type Changed = Arc<dyn Fn() + Send + Sync>;

/// What a session reports back to the registry.
#[derive(Clone)]
pub struct SessionHooks {
    /// The user chose "Stoppen" in a tab's marker.
    pub stop: OnStop,
    /// The session's tabs or last action changed.
    pub changed: Changed,
    /// The browser the session's tabs belong to ended.
    pub gone: Changed,
}

/// One running session as the sessions bar shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BrowserSessionView {
    pub id: String,
    /// The agent's name as it claims it; empty for the built-in agent.
    pub agent: String,
    pub builtin: bool,
    pub tabs: Vec<BrowserTabView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_action: Option<BrowserLastAction>,
}

/// A tab of a session: the website and title of the page the agent last saw (the title comes
/// from the website).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BrowserTabView {
    pub tab: String,
    pub site: String,
    pub title: String,
}

/// The last browser tool of a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BrowserLastAction {
    pub tool: String,
    pub site: String,
    /// Unix milliseconds.
    pub at: i64,
}

/// What the window can do with a session (contracts/tauri-commands.md); taking over and giving
/// back follow with PR D.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum BrowserControl {
    Stop,
}

/// Why no session serves a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoSession {
    /// The caller is no agent.
    NotAnAgent,
    /// The user stopped the agent's session; a new one has not begun yet.
    Stopped,
}

enum Entry {
    Live(Arc<Session>),
    Stopped,
}

#[derive(Default)]
struct Shared {
    entries: Mutex<HashMap<String, Entry>>,
    changed: Mutex<Option<Changed>>,
}

/// The sessions of this app. Cheap to clone; every clone is the same registry.
#[derive(Clone, Default)]
pub struct BrowserSessions {
    shared: Arc<Shared>,
}

/// The registry key of `caller`'s session.
fn key(caller: &Caller) -> Option<String> {
    match caller {
        Caller::BuiltinAgent => Some("builtin".to_owned()),
        Caller::ExternalAgent { id } => Some(format!("agent:{id}")),
        _ => None,
    }
}

impl BrowserSessions {
    /// Sets how the window hears of changes; called from `setup()`.
    pub fn set_changed(&self, changed: Changed) {
        *self
            .shared
            .changed
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(changed);
    }

    fn changed(&self) {
        let changed = self
            .shared
            .changed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(changed) = changed {
            changed();
        }
    }

    /// The session of `caller`, a new one from `start` (with its id and hooks) when it has none.
    pub fn get_or_start(
        &self,
        caller: &Caller,
        start: impl FnOnce(String, SessionHooks) -> Arc<Session>,
    ) -> Result<Arc<Session>, NoSession> {
        let key = key(caller).ok_or(NoSession::NotAnAgent)?;
        let session = {
            let mut entries = self
                .shared
                .entries
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            match entries.get(&key) {
                Some(Entry::Live(session)) => return Ok(Arc::clone(session)),
                Some(Entry::Stopped) => return Err(NoSession::Stopped),
                None => {}
            }
            let id = uuid::Uuid::new_v4().to_string();
            let session = start(id.clone(), self.hooks(&id));
            entries.insert(key, Entry::Live(Arc::clone(&session)));
            session
        };
        self.changed();
        Ok(session)
    }

    fn hooks(&self, id: &str) -> SessionHooks {
        let weak = Arc::downgrade(&self.shared);
        let stop_id = id.to_owned();
        let stop_weak = Weak::clone(&weak);
        let gone_id = id.to_owned();
        let gone_weak = Weak::clone(&weak);
        SessionHooks {
            stop: Arc::new(move || {
                let (Some(shared), Ok(runtime)) =
                    (stop_weak.upgrade(), tokio::runtime::Handle::try_current())
                else {
                    return;
                };
                let id = stop_id.clone();
                runtime.spawn(async move { BrowserSessions { shared }.stop(&id).await });
            }),
            changed: Arc::new(move || {
                if let Some(shared) = weak.upgrade() {
                    BrowserSessions { shared }.changed();
                }
            }),
            gone: Arc::new(move || {
                let (Some(shared), Ok(runtime)) =
                    (gone_weak.upgrade(), tokio::runtime::Handle::try_current())
                else {
                    return;
                };
                let id = gone_id.clone();
                runtime.spawn(async move { BrowserSessions { shared }.gone(&id).await });
            }),
        }
    }

    /// Takes the live session `id` out of the registry, leaving `then` in its place (or nothing).
    fn take(&self, id: &str, stopped: bool) -> Option<Arc<Session>> {
        let mut entries = self
            .shared
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let key = entries.iter().find_map(|(key, entry)| match entry {
            Entry::Live(session) if session.id == id => Some(key.clone()),
            _ => None,
        })?;
        let entry = if stopped {
            entries.insert(key, Entry::Stopped)
        } else {
            entries.remove(&key)
        };
        match entry {
            Some(Entry::Live(session)) => Some(session),
            _ => None,
        }
    }

    /// The user stopped session `id` (FR-016); `false` when it no longer runs. Its running call
    /// and questions end at once; its tabs go back to the user right after.
    pub async fn stop(&self, id: &str) -> bool {
        let Some(session) = self.take(id, true) else {
            return false;
        };
        session.cancel.cancel();
        self.changed();
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => drop(runtime.spawn(async move { session.end().await })),
            Err(_) => session.end().await,
        }
        true
    }

    /// The browser of session `id` ended; the agent's next call begins a new session.
    async fn gone(&self, id: &str) {
        if let Some(session) = self.take(id, false) {
            self.changed();
            session.end().await;
        }
    }

    /// Ends the session of `caller`, stopped or not: the agent was revoked, lost the browser
    /// rights or ended its connection.
    pub async fn end(&self, caller: &Caller) {
        let Some(key) = key(caller) else {
            return;
        };
        let entry = self
            .shared
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&key);
        match entry {
            Some(Entry::Live(session)) => {
                self.changed();
                session.end().await;
            }
            Some(Entry::Stopped) | None => {}
        }
    }

    /// Lets `caller` begin a new session after the user stopped its last one.
    pub fn resume(&self, caller: &Caller) {
        let Some(key) = key(caller) else {
            return;
        };
        let mut entries = self
            .shared
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if matches!(entries.get(&key), Some(Entry::Stopped)) {
            entries.remove(&key);
        }
    }

    /// Takes a new target of the browser for the live session whose page or frame opened it
    /// ([`super::new_tab`]).
    pub fn claim(&self) -> super::cdp::target::Claim {
        let shared = Arc::downgrade(&self.shared);
        Arc::new(move |connection, attached| {
            let Some(shared) = shared.upgrade() else {
                return false;
            };
            let live: Vec<Arc<Session>> = shared
                .entries
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .values()
                .filter_map(|entry| match entry {
                    Entry::Live(session) => Some(Arc::clone(session)),
                    Entry::Stopped => None,
                })
                .collect();
            live.iter()
                .any(|session| super::new_tab::opened(session, connection, attached))
        })
    }

    /// The running sessions, for the sessions bar.
    pub fn list(&self) -> Vec<BrowserSessionView> {
        let mut views: Vec<BrowserSessionView> = self
            .shared
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .filter_map(|entry| match entry {
                Entry::Live(session) => Some(session.view()),
                Entry::Stopped => None,
            })
            .collect();
        views.sort_by(|a, b| a.id.cmp(&b.id));
        views
    }
}
