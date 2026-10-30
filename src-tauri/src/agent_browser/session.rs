//! One agent's browser session (spec 065, data-model.md "Im Speicher"): its tabs (at most
//! [`MAX_TABS`], FR-029), the websites allowed once (FR-007), the approval mode of its last call
//! and its last action. Its tabs belong to one run of the browser; when the browser started anew,
//! they are gone ([`Session::tabs_of`]).
//!
//! The website check of every document of its tabs runs here ([`Session::check`]): a website of
//! the agent's rights loads, any other asks the user once, or is refused in Plan.
//!
//! The session lives in the registry of [`super::sessions`]; what it shows the user (agent, tabs,
//! last action, FR-015) it keeps outside its tabs' lock, so the sessions bar never waits for a
//! running call. When the user stops it, its tabs go back to the user ([`Session::end`]).

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError, Weak};

use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use super::cdp::Connection;
use super::executor::BrowserEnv;
use super::marker::MarkerTexts;
use super::prompts::{
    self, BrowserDecision, BrowserPromptKind, BrowserPrompts, BrowserQuestion, ClosedNotice,
    Remember,
};
use super::sessions::{BrowserLastAction, BrowserSessionView, BrowserTabView, SessionHooks};
use super::sites::{check_document, Site, Verdict};
use super::tab::{Guard, Tab, TabMarker};
use crate::caller::Caller;
use crate::chat::tools::permission::PermissionMode;
use crate::mcp_server::grants::SiteLevel;

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
    asking: tokio::sync::Mutex<()>,
    pub tabs: tokio::sync::Mutex<Tabs>,
    env: Arc<dyn BrowserEnv>,
    prompts: BrowserPrompts,
    closed: ClosedNotice,
    /// Ends the questions and the running call of this session when it ends.
    pub cancel: CancellationToken,
    hooks: SessionHooks,
    /// The agent's name as it claims it; empty for the built-in agent.
    agent: OnceCell<String>,
    /// What the sessions bar shows.
    shown: Mutex<Shown>,
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
    next: u32,
    /// The tab `browser.open` uses.
    pub current: Option<String>,
    /// The run of the browser the tabs belong to.
    run: Option<Weak<Connection>>,
}

impl Tabs {
    /// The next tab id, `t1`, `t2`, ….
    pub fn next_id(&mut self) -> String {
        self.next += 1;
        format!("t{}", self.next)
    }

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
            shown: Mutex::default(),
        })
    }

    /// The agent's name as it claims it; empty for the built-in agent.
    pub async fn agent_name(&self) -> String {
        self.agent
            .get_or_init(|| self.env.agent_name(&self.caller))
            .await
            .clone()
    }

    /// The marker of a new tab of this session.
    pub async fn marker(&self) -> TabMarker {
        TabMarker {
            texts: MarkerTexts::new(&self.env.locale(), &self.agent_name().await),
            on_stop: Arc::clone(&self.hooks.stop),
        }
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
        let same = tabs
            .run
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|run| Arc::ptr_eq(&run, connection));
        if !same {
            tabs.open.clear();
            tabs.current = None;
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

    fn mode(&self) -> PermissionMode {
        *self.mode.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The guard of this session's tabs.
    pub fn guard(self: &Arc<Self>) -> Guard {
        let session = Arc::downgrade(self);
        Arc::new(move |url: String| {
            let session = session.clone();
            Box::pin(async move {
                match session.upgrade() {
                    Some(session) => session.check(&url).await,
                    None => Verdict::Refuse("site_not_allowed"),
                }
            })
        })
    }

    /// The verdict for a document at `url` of one of this session's tabs.
    pub async fn check(&self, url: &str) -> Verdict {
        let first = self.verdict(url).await;
        let Verdict::Ask(_) = first else {
            return first;
        };
        // One question at a time: after waiting, the answer to another may already cover this one.
        let _asking = self.asking.lock().await;
        let site = match self.verdict(url).await {
            Verdict::Ask(site) => site,
            other => return other,
        };
        let question = BrowserQuestion {
            kind: BrowserPromptKind::Site,
            agent: self.env.agent_name(&self.caller).await,
            site: Some(site.to_string()),
            tool: None,
            label: Some(url.to_owned()),
        };
        match prompts::ask(&self.prompts, &self.closed, &question, &self.cancel).await {
            Ok(answer) if answer.decision == BrowserDecision::Allow => {
                self.once
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(site.clone());
                let level = match answer.remember {
                    Some(Remember::SiteRead) => Some(SiteLevel::Read),
                    // `http` can only be read (research R9).
                    Some(Remember::SiteOperate) if !site.is_plain_http() => {
                        Some(SiteLevel::Operate)
                    }
                    Some(Remember::SiteOperate) => Some(SiteLevel::Read),
                    None => None,
                };
                if let Some(level) = level {
                    if let Err(error) = self.env.remember(&self.caller, &site, level).await {
                        log::warn!("agent browser: could not remember a website: {error}");
                    }
                }
                Verdict::Go
            }
            _ => Verdict::Refuse("site_not_allowed"),
        }
    }

    async fn verdict(&self, url: &str) -> Verdict {
        let Some(rights) = self.env.rights(&self.caller).await else {
            return Verdict::Refuse("site_not_allowed");
        };
        check_document(
            url,
            &rights.sites,
            &self.once_allowed_sites(),
            self.mode() != PermissionMode::Plan,
        )
    }

    /// Ends the session: its questions and running call end, and its tabs go back to the user,
    /// open, without marker and without holzi's check (FR-016).
    pub async fn end(&self) {
        self.cancel.cancel();
        let tabs = std::mem::take(&mut self.tabs.lock().await.open);
        for tab in tabs {
            tab.release().await;
        }
    }
}
