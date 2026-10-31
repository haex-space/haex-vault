//! Taking over and giving back (spec 065, FR-017, FR-018, research R6). The user takes over with
//! "Eingreifen" (in holzi or in the marker), by the agent's request (`browser.takeover.request`)
//! or by pressing a mouse button or key in an agent tab while the agent steers. Then the agent's
//! running call ends and every further one gets `paused`, the markers say the user has the tab,
//! and the tabs' requests go without holzi's questions, since the user is the one acting.
//!
//! Giving back checks each tab's page against the agent's rights, as a new document would be: a
//! page outside them asks, and a tab without the user's yes stays with the user, without marker.
//! Then the agent steers again.

use std::future::Future;
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

use super::{Session, Tabs};
use crate::agent_browser::sessions::BrowserPause;
use crate::agent_browser::sites::Verdict;

/// How long after its own input holzi takes an input the marker reports for its own: the report
/// comes back over the browser's pipe after the input event.
const OWN_INPUT_ECHO: Duration = Duration::from_millis(500);

/// How long changing one tab's marker or bringing it to the front may take.
const MARKER_STEP: Duration = Duration::from_secs(1);

/// holzi's own input into the pages of a session.
#[derive(Default)]
pub(super) struct OwnInput {
    running: usize,
    last: Option<Instant>,
}

/// Which tab comes to the front when the user takes over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Front {
    /// None: the user is in the tab already.
    Stay,
    /// The tab the agent used last.
    Current,
    Tab(String),
}

/// Counts holzi's own input while it runs, also when the call is dropped halfway.
struct Inputting<'a>(&'a Session);

impl Drop for Inputting<'_> {
    fn drop(&mut self) {
        let mut own = self.0.own_input();
        own.running -= 1;
        own.last = Some(Instant::now());
    }
}

impl Session {
    fn own_input(&self) -> std::sync::MutexGuard<'_, OwnInput> {
        self.own_input
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Who has the browser: `None` while the agent steers.
    pub fn pause(&self) -> Option<BrowserPause> {
        self.pause.borrow().clone()
    }

    pub fn is_paused(&self) -> bool {
        self.pause.borrow().is_some()
    }

    /// Resolves once the user has the browser.
    pub async fn until_paused(&self) {
        let mut state = self.pause.subscribe();
        let _ = state.wait_for(Option::is_some).await;
    }

    /// Resolves once the user gave the browser back.
    pub async fn until_returned(&self) {
        let mut state = self.pause.subscribe();
        let _ = state.wait_for(Option::is_none).await;
    }

    /// The user takes over: the agent's calls get `paused` until the user gives back, and the
    /// markers say so. `false` when the user has the browser already. Returns at once; the markers
    /// change once the running call let go of the tabs.
    pub async fn take_over(self: &Arc<Self>, by: BrowserPause, front: Front) -> bool {
        let steering = Arc::clone(&self.steering).lock_owned().await;
        let taken = self.pause.send_if_modified(|state| {
            if state.is_some() {
                return false;
            }
            *state = Some(by);
            true
        });
        if !taken {
            return false;
        }
        self.drop_grants();
        (self.hooks.changed)();
        let session = Arc::clone(self);
        tokio::spawn(async move {
            let _steering = steering;
            let tabs = session.tabs.lock().await;
            session.show_markers(&tabs, true).await;
            let front = match front {
                Front::Stay => None,
                Front::Current => tabs.current.clone(),
                Front::Tab(tab) => Some(tab),
            };
            if let Some(tab) = front.and_then(|id| tabs.get(&id)) {
                if tokio::time::timeout(MARKER_STEP, tab.activate())
                    .await
                    .is_err()
                {
                    log::debug!("agent browser: bringing a tab to the front: no answer");
                }
            }
        });
        true
    }

    /// The user gives the browser back; `false` when the agent steers already. A tab whose page,
    /// or the last page it began to load meanwhile, is outside the agent's rights asks, and stays
    /// with the user unless allowed.
    pub async fn give_back(&self) -> bool {
        let _steering = self.steering.lock().await;
        if !self.is_paused() {
            return false;
        }
        let mut tabs = self.tabs.lock().await;
        let mut loaded = std::mem::take(
            &mut *self
                .loaded_while_paused
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for tab in std::mem::take(&mut tabs.open) {
            let allowed = match tab.url().await {
                Ok(url) => self.allowed(loaded.remove(&tab.id), url).await,
                Err(_) => false,
            };
            if allowed {
                tabs.open.push(tab);
                continue;
            }
            if tabs.current.as_deref() == Some(tab.id.as_str()) {
                tabs.current = None;
            }
            self.forget_targets(&tab.id);
            tab.release().await;
        }
        self.pause.send_replace(None);
        self.show_markers(&tabs, false).await;
        self.note(&tabs, None);
        true
    }

    /// Whether the agent may have a tab whose page is `current` and which last began to load
    /// `loaded`, which may not have arrived yet.
    async fn allowed(&self, loaded: Option<String>, current: String) -> bool {
        for url in loaded.into_iter().chain(std::iter::once(current)) {
            if self.check(&url).await != Verdict::Go {
                return false;
            }
        }
        true
    }

    /// Notes that tab `tab` began to load `url` while the user had the browser.
    pub(super) fn loaded_while_paused(&self, tab: &str, url: &str) {
        self.loaded_while_paused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(tab.to_owned(), url.to_owned());
    }

    /// Shows the markers of the current documents as `paused` or not. A page waiting for a slow
    /// document does not answer before it is there; its new document asks for the state itself.
    async fn show_markers(&self, tabs: &Tabs, paused: bool) {
        let shown = tabs.open.iter().map(|tab| async move {
            match tokio::time::timeout(MARKER_STEP, tab.show_marker(paused)).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => log::debug!("agent browser: changing a marker: {error}"),
                Err(_) => log::debug!("agent browser: changing a marker: no answer"),
            }
        });
        futures::future::join_all(shown).await;
    }

    /// The marker saw the user press a mouse button or key in a tab: unless that was holzi's own
    /// input, the agent pauses (FR-017).
    pub async fn noticed_input(self: &Arc<Self>) {
        let own = {
            let own = self.own_input();
            own.running > 0 || own.last.is_some_and(|last| last.elapsed() < OWN_INPUT_ECHO)
        };
        if !own {
            self.take_over(BrowserPause::User, Front::Stay).await;
        }
    }

    /// Runs `input`, holzi's own input into a page, so that the marker's report of it does not
    /// pause the agent.
    pub async fn own<T>(&self, input: impl Future<Output = T>) -> T {
        self.own_input().running += 1;
        let _inputting = Inputting(self);
        input.await
    }
}
