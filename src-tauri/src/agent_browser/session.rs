//! One agent's browser session (spec 065, data-model.md "Im Speicher"): its tabs (at most
//! [`MAX_TABS`], FR-029), the websites allowed once (FR-007), the approval mode of its last call
//! and its last action. Its tabs belong to one run of the browser; when the browser started anew,
//! they are gone ([`Session::tabs_of`]).
//!
//! The website check of every document of its tabs runs here ([`Session::check`]): a website of
//! the agent's rights loads, any other asks the user once, or is refused in Plan. So does the
//! question before a request that may change something on a website ([`Session::check_request`],
//! research R5 step 2): it asks for that one request, in Manuell and Auto, unless the user just
//! allowed the action that sent it ([`Session::arm`]); it is refused in Plan.
//!
//! The session lives in the registry of [`super::sessions`]; what it shows the user (agent, tabs,
//! last action, FR-015) it keeps outside its tabs' lock, so the sessions bar never waits for a
//! running call; so does it keep which targets are its tabs and their frames, so a tab their pages
//! open finds its session while the call that opened it still runs ([`super::new_tab`]). When the
//! user stops it, its tabs go back to the user ([`Session::end`]); while the user has taken over,
//! the agent waits (session_pause.rs).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use super::cdp::Connection;
use super::executor::BrowserEnv;
use super::marker::{MarkerTexts, Signal};
use super::prompts::{BrowserPrompts, ClosedNotice};
use super::sessions::{
    BrowserLastAction, BrowserPause, BrowserSessionView, BrowserTabView, SessionHooks,
};
use super::sites::Site;
use super::tab::{Tab, TabHooks};
use crate::caller::Caller;
use crate::chat::tools::permission::PermissionMode;

#[path = "session_checks.rs"]
mod checks;
#[path = "session_grants.rs"]
mod grants;
#[path = "session_pause.rs"]
mod pause;

pub use pause::Front;

/// The most agent tabs one session has (FR-029).
pub const MAX_TABS: usize = 5;

/// What the session of one agent keeps.
pub struct Session {
    /// Names the session for the window; never shown to an agent.
    pub id: String,
    pub caller: Caller,
    once: Mutex<BTreeSet<Site>>,
    mode: Mutex<PermissionMode>,
    /// One question at a time, so frames of the same new website ask once.
    pub(super) asking: tokio::sync::Mutex<()>,
    pub tabs: tokio::sync::Mutex<Tabs>,
    env: Arc<dyn BrowserEnv>,
    prompts: BrowserPrompts,
    closed: ClosedNotice,
    /// Ends the questions and the running call of this session when it ends.
    pub cancel: CancellationToken,
    hooks: SessionHooks,
    /// The agent's name as it claims it; empty for the built-in agent.
    agent: OnceCell<String>,
    name_notified: AtomicBool,
    /// What the sessions bar shows.
    shown: Mutex<Shown>,
    /// Submit grants and their first new tab, when the action opens one.
    armed: Mutex<grants::SubmitGrants>,
    /// Questions a page set off that are open now (submitting, a new tab); an action waits for
    /// them before it reports.
    questions: AtomicUsize,
    /// The last tab number given out.
    numbered: AtomicU32,
    /// The targets of the tabs and of their frames, each with its tab's id.
    targets: Mutex<BTreeMap<String, String>>,
    /// Who has the browser: `None` while the agent steers, else why the user has it.
    pause: tokio::sync::watch::Sender<Option<BrowserPause>>,
    /// One taking over or giving back at a time, the markers' change included.
    steering: Arc<tokio::sync::Mutex<()>>,
    own_input: Mutex<pause::OwnInput>,
    /// The last page each tab loaded while the user had the browser, perhaps still on its way.
    loaded_while_paused: Mutex<BTreeMap<String, String>>,
}

#[derive(Default)]
struct Shown {
    tabs: Vec<BrowserTabView>,
    last_action: Option<BrowserLastAction>,
}

/// The tabs of a session.
#[derive(Default)]
pub struct Tabs {
    pub open: Vec<Tab>,
    /// The tab `browser.open` uses.
    pub current: Option<String>,
    /// The run of the browser the tabs belong to.
    run: Option<Weak<Connection>>,
}

impl Tabs {
    pub fn get(&self, id: &str) -> Option<&Tab> {
        self.open.iter().find(|tab| tab.id == id)
    }
}

impl Session {
    pub fn new(
        id: String,
        caller: Caller,
        env: Arc<dyn BrowserEnv>,
        prompts: BrowserPrompts,
        closed: ClosedNotice,
        hooks: SessionHooks,
    ) -> Arc<Self> {
        Arc::new(Self {
            id,
            caller,
            once: Mutex::default(),
            mode: Mutex::new(PermissionMode::Manual),
            asking: tokio::sync::Mutex::new(()),
            tabs: tokio::sync::Mutex::default(),
            env,
            prompts,
            closed,
            cancel: CancellationToken::new(),
            hooks,
            agent: OnceCell::new(),
            name_notified: AtomicBool::new(false),
            armed: Mutex::default(),
            questions: AtomicUsize::new(0),
            numbered: AtomicU32::new(0),
            targets: Mutex::default(),
            shown: Mutex::default(),
            pause: tokio::sync::watch::Sender::new(None),
            steering: Arc::default(),
            own_input: Mutex::default(),
            loaded_while_paused: Mutex::default(),
        })
    }

    /// The agent's name as it claims it; empty for the built-in agent.
    pub async fn agent_name(&self) -> String {
        let name = self
            .agent
            .get_or_init(|| self.env.agent_name(&self.caller))
            .await
            .clone();
        if !self.name_notified.swap(true, Ordering::Relaxed) {
            // Notify after OnceCell has published the name so the window's immediate query sees it.
            (self.hooks.changed)();
        }
        name
    }

    /// The next tab id, `t1`, `t2`, ….
    pub fn next_tab_id(&self) -> String {
        format!("t{}", self.numbered.fetch_add(1, Ordering::SeqCst) + 1)
    }

    /// The marker of the new tab `tab` of this session, where the marker reports, and where the
    /// tab reports its targets.
    pub async fn tab_hooks(self: &Arc<Self>, tab: &str) -> TabHooks {
        let (session, tab) = (Arc::downgrade(self), tab.to_owned());
        let (signalled, stop) = (Arc::downgrade(self), Arc::clone(&self.hooks.stop));
        let asked = Arc::downgrade(self);
        TabHooks {
            texts: MarkerTexts::new(
                &self.env.locale(),
                self.caller == Caller::BuiltinAgent,
                &self.agent_name().await,
            ),
            paused: Arc::new(move || asked.upgrade().is_some_and(|session| session.is_paused())),
            on_signal: Arc::new(move |signal| {
                if signal == Signal::Stop {
                    return stop();
                }
                let (Some(session), Ok(runtime)) =
                    (signalled.upgrade(), tokio::runtime::Handle::try_current())
                else {
                    return;
                };
                runtime.spawn(async move {
                    match signal {
                        Signal::Takeover => {
                            session.take_over(BrowserPause::User, Front::Stay).await;
                        }
                        Signal::Return => {
                            session.give_back().await;
                        }
                        Signal::Input => session.noticed_input().await,
                        Signal::Stop | Signal::Hello => {}
                    }
                });
            }),
            on_target: Arc::new(move |target: &str| {
                if let Some(session) = session.upgrade() {
                    session.targets().insert(target.to_owned(), tab.clone());
                }
            }),
        }
    }

    fn targets(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, String>> {
        self.targets.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The tab whose page or frame is the target `target`.
    pub(super) fn tab_of_target(&self, target: &str) -> Option<String> {
        self.targets().get(target).cloned()
    }

    /// Reserves a slot for a page opened by a tab, before its asynchronous adoption starts.
    pub(super) fn reserve_new_tab(&self, target: &str) -> Option<String> {
        let mut targets = self.targets();
        if targets.contains_key(target)
            || targets.values().collect::<BTreeSet<_>>().len() >= MAX_TABS
        {
            return None;
        }
        let tab = self.next_tab_id();
        targets.insert(target.to_owned(), tab.clone());
        Some(tab)
    }

    /// Forgets the targets of tab `tab`, which left the session.
    pub fn forget_targets(&self, tab: &str) {
        self.targets().retain(|_, owner| owner != tab);
    }

    /// What the sessions bar shows of this session.
    pub fn view(&self) -> BrowserSessionView {
        let shown = self.shown.lock().unwrap_or_else(PoisonError::into_inner);
        BrowserSessionView {
            id: self.id.clone(),
            agent: self.agent.get().cloned().unwrap_or_default(),
            builtin: self.caller == Caller::BuiltinAgent,
            tabs: shown.tabs.clone(),
            last_action: shown.last_action.clone(),
            paused: self.pause(),
        }
    }

    /// Notes the tabs after a call and, when `tool` is set, the call as the last action; the
    /// window hears of it.
    pub fn note(&self, tabs: &Tabs, tool: Option<(&str, String)>) {
        {
            let mut shown = self.shown.lock().unwrap_or_else(PoisonError::into_inner);
            shown.tabs = tabs
                .open
                .iter()
                .map(|tab| {
                    let page = tab.page();
                    BrowserTabView {
                        tab: tab.id.clone(),
                        site: page.site,
                        title: page.title,
                    }
                })
                .collect();
            if let Some((tool, site)) = tool {
                shown.last_action = Some(BrowserLastAction {
                    tool: tool.to_owned(),
                    site,
                    at: crate::passwords::clock::unix_millis(std::time::SystemTime::now()),
                });
            }
        }
        (self.hooks.changed)();
    }

    /// The tabs of this session in the run of `connection`; tabs of an earlier run are dropped.
    pub async fn tabs_of(
        self: &Arc<Self>,
        connection: &Arc<Connection>,
    ) -> tokio::sync::MutexGuard<'_, Tabs> {
        let mut tabs = self.tabs.lock().await;
        if !Self::same_run(&tabs, connection) {
            tabs.open.clear();
            tabs.current = None;
            self.targets().clear();
            tabs.run = Some(Arc::downgrade(connection));
            self.watch_run(connection);
        }
        tabs
    }

    /// Follows the run of the browser the tabs belong to: a tab the user closes leaves the
    /// session, and when the browser ends (its window closed, a crash, the vault closing), the
    /// session ends with it.
    fn watch_run(self: &Arc<Self>, connection: &Arc<Connection>) {
        let mut events = connection.subscribe(None);
        let (session, run) = (Arc::downgrade(self), Arc::downgrade(connection));
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                if event.method != "Target.detachedFromTarget" {
                    continue;
                }
                let (Some(gone), Some(session)) =
                    (event.params["sessionId"].as_str(), session.upgrade())
                else {
                    continue;
                };
                session.forget_tab(gone).await;
            }
            let Some(session) = session.upgrade() else {
                return;
            };
            let current = session
                .tabs
                .lock()
                .await
                .run
                .as_ref()
                .is_some_and(|known| known.ptr_eq(&run));
            if current {
                (session.hooks.gone)();
            }
        });
    }

    /// Whether `tabs` belong to the run of `connection`.
    pub(super) fn same_run(tabs: &Tabs, connection: &Arc<Connection>) -> bool {
        tabs.run
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|run| Arc::ptr_eq(&run, connection))
    }

    /// Drops the tab whose CDP session `gone` ended (the user closed it).
    async fn forget_tab(&self, gone: &str) {
        let mut tabs = self.tabs.lock().await;
        let Some(index) = tabs.open.iter().position(|tab| tab.session == gone) else {
            return;
        };
        let tab = tabs.open.remove(index);
        if tabs.current.as_deref() == Some(tab.id.as_str()) {
            tabs.current = None;
        }
        self.forget_targets(&tab.id);
        self.note(&tabs, None);
    }

    /// Notes the approval mode of the current call; the website check uses the latest.
    pub fn set_mode(&self, mode: PermissionMode) {
        *self.mode.lock().unwrap_or_else(PoisonError::into_inner) = mode;
    }

    /// The websites this session was allowed to visit once.
    pub fn once_allowed_sites(&self) -> BTreeSet<Site> {
        self.once
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(super) fn mode(&self) -> PermissionMode {
        *self.mode.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ends the session: its questions and running call end, and its tabs go back to the user,
    /// open, without marker and without holzi's check (FR-016).
    pub async fn end(&self) {
        self.cancel.cancel();
        let tabs = std::mem::take(&mut self.tabs.lock().await.open);
        self.targets().clear();
        futures::future::join_all(
            tabs.into_iter()
                .map(|tab| async move { tab.release().await }),
        )
        .await;
    }
}
