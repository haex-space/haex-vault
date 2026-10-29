//! One agent's browser session (spec 065, data-model.md "Im Speicher"): its tabs (at most
//! [`MAX_TABS`], FR-029), the websites allowed once (FR-007), the approval mode of its last call
//! and its last action. Its tabs belong to one run of the browser; when the browser started anew,
//! they are gone ([`Session::tabs_of`]).
//!
//! The website check of every document of its tabs runs here ([`Session::check`]): a website of
//! the agent's rights loads, any other asks the user once, or is refused in Plan.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError};

use tokio_util::sync::CancellationToken;

use super::cdp::Connection;
use super::executor::BrowserEnv;
use super::prompts::{
    self, BrowserDecision, BrowserPromptKind, BrowserPrompts, BrowserQuestion, ClosedNotice,
    Remember,
};
use super::sites::{check_document, Site, Verdict};
use super::tab::{Guard, Tab};
use crate::caller::Caller;
use crate::chat::tools::permission::PermissionMode;
use crate::mcp_server::grants::SiteLevel;

/// The most agent tabs one session has (FR-029).
pub const MAX_TABS: usize = 5;

/// What the session of one agent keeps.
pub struct Session {
    pub caller: Caller,
    once: Mutex<BTreeSet<Site>>,
    mode: Mutex<PermissionMode>,
    /// One question at a time, so frames of the same new website ask once.
    asking: tokio::sync::Mutex<()>,
    pub tabs: tokio::sync::Mutex<Tabs>,
    env: Arc<dyn BrowserEnv>,
    prompts: BrowserPrompts,
    closed: ClosedNotice,
    /// Ends the questions of this session when it ends.
    pub cancel: CancellationToken,
}

/// The tabs of a session.
#[derive(Default)]
pub struct Tabs {
    pub open: Vec<Tab>,
    next: u32,
    /// The tab `browser.open` uses.
    pub current: Option<String>,
    /// The last action, for the sessions bar (PR C3).
    pub last_action: Option<String>,
    /// The run of the browser the tabs belong to.
    run: Option<std::sync::Weak<Connection>>,
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
        caller: Caller,
        env: Arc<dyn BrowserEnv>,
        prompts: BrowserPrompts,
        closed: ClosedNotice,
    ) -> Arc<Self> {
        Arc::new(Self {
            caller,
            once: Mutex::default(),
            mode: Mutex::new(PermissionMode::Manual),
            asking: tokio::sync::Mutex::new(()),
            tabs: tokio::sync::Mutex::default(),
            env,
            prompts,
            closed,
            cancel: CancellationToken::new(),
        })
    }

    /// The tabs of this session in the run of `connection`; tabs of an earlier run are dropped.
    pub async fn tabs_of(&self, connection: &Arc<Connection>) -> tokio::sync::MutexGuard<'_, Tabs> {
        let mut tabs = self.tabs.lock().await;
        let same = tabs
            .run
            .as_ref()
            .and_then(std::sync::Weak::upgrade)
            .is_some_and(|run| Arc::ptr_eq(&run, connection));
        if !same {
            tabs.open.clear();
            tabs.current = None;
            tabs.run = Some(Arc::downgrade(connection));
        }
        tabs
    }

    /// Notes the approval mode of the current call; the website check uses the latest.
    pub fn set_mode(&self, mode: PermissionMode) {
        *self.mode.lock().unwrap_or_else(PoisonError::into_inner) = mode;
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
        let once = self
            .once
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        check_document(
            url,
            &rights.sites,
            &once,
            self.mode() != PermissionMode::Plan,
        )
    }

    /// Closes every tab of the session.
    pub async fn end(&self) {
        self.cancel.cancel();
        let tabs = std::mem::take(&mut self.tabs.lock().await.open);
        for tab in tabs {
            tab.close().await;
        }
    }
}
