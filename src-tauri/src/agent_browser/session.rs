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
//! user stops it, its tabs go back to the user ([`Session::end`]).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use tokio::sync::{Notify, OnceCell};
use tokio_util::sync::CancellationToken;

use super::cdp::Connection;
use super::executor::BrowserEnv;
use super::marker::MarkerTexts;
use super::prompts::{
    self, need, BrowserDecision, BrowserPromptKind, BrowserPrompts, BrowserQuestion, ClosedNotice,
    Effect, Need, Remember,
};
use super::sessions::{BrowserLastAction, BrowserSessionView, BrowserTabView, SessionHooks};
use super::sites::{check_document, Site, Verdict};
use super::submit;
use super::tab::{Guard, Paused, Tab, TabHooks};
use crate::caller::Caller;
use crate::chat::tools::permission::PermissionMode;
use crate::mcp_server::grants::SiteLevel;
use crate::prompt_park::PromptEnd;

#[path = "session_grants.rs"]
mod grants;

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
    /// Popup targets claimed from the browser but not yet adopted or closed.
    pending_opened_tabs: AtomicUsize,
    opened_tabs_changed: Notify,
    /// The last tab number given out.
    numbered: AtomicU32,
    /// Targets and their tab ids; `None` reserves the one tool-created tab being opened under `tabs`.
    targets: Mutex<BTreeMap<Option<String>, String>>,
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
            pending_opened_tabs: AtomicUsize::new(0),
            opened_tabs_changed: Notify::new(),
            numbered: AtomicU32::new(0),
            targets: Mutex::default(),
            shown: Mutex::default(),
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
    fn next_tab_id(&self) -> String {
        format!("t{}", self.numbered.fetch_add(1, Ordering::SeqCst) + 1)
    }

    /// The marker of the new tab `tab` of this session, and where it reports its targets.
    pub async fn tab_hooks(self: &Arc<Self>, tab: &str) -> TabHooks {
        let (session, tab) = (Arc::downgrade(self), tab.to_owned());
        TabHooks {
            texts: MarkerTexts::new(
                &self.env.locale(),
                self.caller == Caller::BuiltinAgent,
                &self.agent_name().await,
            ),
            on_stop: Arc::clone(&self.hooks.stop),
            on_target: Arc::new(move |target: &str| {
                if let Some(session) = session.upgrade() {
                    let mut targets = session.targets();
                    if targets.get(&None) == Some(&tab) {
                        targets.remove(&None);
                    }
                    targets.insert(Some(target.to_owned()), tab.clone());
                }
            }),
        }
    }

    fn targets(&self) -> std::sync::MutexGuard<'_, BTreeMap<Option<String>, String>> {
        self.targets.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The tab whose page or frame is the target `target`.
    pub(super) fn tab_of_target(&self, target: &str) -> Option<String> {
        self.targets().get(&Some(target.to_owned())).cloned()
    }

    /// Reserves before any asynchronous opening: `Some` adopts a popup, `None` creates a target.
    pub(super) fn reserve_new_tab(&self, target: Option<&str>) -> Option<String> {
        let mut targets = self.targets();
        let target = target.map(str::to_owned);
        if targets.contains_key(&target)
            || targets.values().collect::<BTreeSet<_>>().len() >= MAX_TABS
        {
            return None;
        }
        let tab = self.next_tab_id();
        targets.insert(target, tab.clone());
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
        // An active tool-created tab holds this lock until its target is registered. A pending
        // reservation here therefore belongs to a cancelled call; popup reservations have targets.
        self.targets().remove(&None);
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

    /// The guard of this session's tab `tab`.
    pub fn guard(self: &Arc<Self>, tab: String) -> Guard {
        let session = Arc::downgrade(self);
        Arc::new(move |paused: Paused| {
            let (session, tab) = (session.clone(), tab.clone());
            Box::pin(async move {
                match session.upgrade() {
                    Some(session) => session.check_request(&tab, paused).await,
                    None => Verdict::Refuse("site_not_allowed"),
                }
            })
        })
    }

    /// The verdict for a held request of tab `tab`: a document's website first, then whether a
    /// request that may change something may go (research R5 step 2).
    pub async fn check_request(&self, tab: &str, paused: Paused) -> Verdict {
        if paused.document {
            match self.check(&paused.url).await {
                Verdict::Go => {}
                other => return other,
            }
        }
        self.check_send(tab, &paused).await
    }

    /// Whether a held request of tab `tab` may go when it may change something: once after the
    /// user allowed the action that sends it, otherwise after asking (research R5 step 2).
    pub(super) async fn check_send(&self, tab: &str, paused: &Paused) -> Verdict {
        if !submit::changing(&paused.method) {
            return Verdict::Go;
        }
        if self.take_armed(tab) {
            return Verdict::Go;
        }
        self.ask_to_submit(paused).await
    }

    /// The first changing request of a new tab may use only its opener's reserved grant.
    pub(super) async fn check_new_tab_send(&self, grant: Option<u64>, paused: &Paused) -> Verdict {
        if !submit::changing(&paused.method) {
            return Verdict::Go;
        }
        if grant.is_some_and(|id| self.take_pending_armed(id)) {
            return Verdict::Go;
        }
        self.ask_to_submit(paused).await
    }

    async fn ask_to_submit(&self, paused: &Paused) -> Verdict {
        if let Need::Refuse(code) = need(self.mode(), Effect::Submit) {
            return Verdict::Refuse(code);
        }
        let question = BrowserQuestion {
            kind: BrowserPromptKind::Submit,
            agent: self.env.agent_name(&self.caller).await,
            site: Site::parse(&paused.url).ok().map(|site| site.to_string()),
            tool: None,
            label: Some(format!("{} {}", paused.method, paused.url)),
            fields: submit::fields_of(paused.body.as_deref()),
        };
        let answer = self.ask_for_page(&question).await;
        match answer {
            Ok(answer) if answer.decision == BrowserDecision::Allow => Verdict::Go,
            Ok(_) => Verdict::Refuse("user_declined"),
            Err(PromptEnd::Expired) => Verdict::Refuse("no_answer"),
            Err(_) => Verdict::Refuse("stopped"),
        }
    }

    /// Asks `question`, which a page set off; the action that caused it waits for the answer.
    pub(super) async fn ask_for_page(
        &self,
        question: &BrowserQuestion,
    ) -> Result<prompts::BrowserAnswer, PromptEnd> {
        self.questions.fetch_add(1, Ordering::SeqCst);
        let answer = prompts::ask(&self.prompts, &self.closed, question, &self.cancel).await;
        self.questions.fetch_sub(1, Ordering::SeqCst);
        answer
    }

    /// Whether a question a page set off is open.
    pub fn questions_open(&self) -> bool {
        self.questions.load(Ordering::SeqCst) > 0
    }

    pub(super) fn opened_tab_started(&self) {
        self.pending_opened_tabs.fetch_add(1, Ordering::SeqCst);
    }

    pub(super) fn opened_tab_finished(&self) {
        if self.pending_opened_tabs.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.opened_tabs_changed.notify_waiters();
        }
    }

    /// Waits until popups caused by the last browser action have joined the tabs or closed.
    pub(super) async fn wait_for_opened_tabs(&self, cancel: &CancellationToken) {
        loop {
            let notified = self.opened_tabs_changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.pending_opened_tabs.load(Ordering::SeqCst) == 0 {
                return;
            }
            tokio::select! {
                _ = notified => {}
                _ = cancel.cancelled() => return,
            }
        }
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
            fields: Vec::new(),
        };
        match prompts::ask(&self.prompts, &self.closed, &question, &self.cancel).await {
            Ok(answer) if answer.decision == BrowserDecision::Allow => {
                self.allow_site(&site, answer.remember).await;
                Verdict::Go
            }
            _ => Verdict::Refuse("site_not_allowed"),
        }
    }

    /// The user allowed `site` for this session, and with `remember` for the agent beyond it.
    pub(super) async fn allow_site(&self, site: &Site, remember: Option<Remember>) {
        self.once
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(site.clone());
        let level = match remember {
            Some(Remember::SiteRead) => Some(SiteLevel::Read),
            // `http` can only be read (research R9).
            Some(Remember::SiteOperate) if !site.is_plain_http() => Some(SiteLevel::Operate),
            Some(Remember::SiteOperate) => Some(SiteLevel::Read),
            None => None,
        };
        if let Some(level) = level {
            if let Err(error) = self.env.remember(&self.caller, site, level).await {
                log::warn!("agent browser: could not remember a website: {error}");
            }
        }
    }

    pub(super) async fn verdict(&self, url: &str) -> Verdict {
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
        self.targets().clear();
        futures::future::join_all(
            tabs.into_iter()
                .map(|tab| async move { tab.release().await }),
        )
        .await;
    }
}
