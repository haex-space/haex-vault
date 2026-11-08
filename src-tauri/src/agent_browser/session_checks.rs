//! What a session's guard decides about the requests its tabs hold (research R5 step 2, R9): the
//! website of each document, asking the user once for a website outside the rights, and asking
//! before a request that may change something on a website, unless the user just allowed the
//! action that sent it. An action waits for the new tabs it opened before it reports.

use std::sync::atomic::Ordering;
use std::sync::{Arc, PoisonError};

use tokio_util::sync::CancellationToken;

use super::Session;
use crate::agent_browser::prompts::{
    self, need, BrowserDecision, BrowserPromptKind, BrowserQuestion, Effect, Need, Remember,
};
use crate::agent_browser::sites::{check_document, is_blocked, Site, Verdict};
use crate::agent_browser::submit;
use crate::agent_browser::tab::{Guard, Paused};
use crate::chat::tools::permission::PermissionMode;
use crate::mcp_server::grants::{RequestRule, SiteLevel};
use crate::prompt_park::PromptEnd;

impl Session {
    /// The guard of this session's tab `tab`.
    pub fn guard(self: &Arc<Self>, tab: String) -> Guard {
        let subject = crate::chat::tools::agent_task::current().and_then(|task| task.subject);
        self.remember_tab_subject(&tab, subject);
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
    /// request that may change something may go (research R5 step 2). While the user has the
    /// browser, the user acts, and every request goes.
    pub async fn check_request(&self, tab: &str, paused: Paused) -> Verdict {
        let subject = self.tab_subject(tab);
        crate::chat::tools::agent_task::with_subject(subject, async {
            if self.is_paused() {
                if paused.document && paused.main {
                    self.loaded_while_paused(tab, &paused.url);
                }
                return Verdict::Go;
            }
            if paused.document {
                match self.check(&paused.url).await {
                    Verdict::Go => {}
                    other => return other,
                }
            } else if self.blocks(&paused.url).await {
                // A page's own requests never reach a blocked website either (FR-047), whatever a
                // remembered request rule says.
                return Verdict::Refuse("site_blocked");
            }
            self.check_send(tab, &paused).await
        })
        .await
    }

    /// Whether the agent's rights block the website of `url` (FR-047).
    async fn blocks(&self, url: &str) -> bool {
        self.env
            .rights(&self.caller)
            .await
            .is_some_and(|rights| is_blocked(url, &rights.sites))
    }

    /// Whether a held request of tab `tab` may go when it may change something: once after the
    /// user allowed the action that sends it, otherwise after asking (research R5 step 2).
    pub(in crate::agent_browser) async fn check_send(&self, tab: &str, paused: &Paused) -> Verdict {
        if !submit::changing(&paused.method) {
            return Verdict::Go;
        }
        let asks = submit::request_asks_with_body(
            &paused.method,
            &paused.url,
            paused.body.as_deref(),
            paused.has_body,
        );
        // The allowance of an action is for the request that needed one, not for a harmless one
        // that happens to go first.
        if asks && self.take_armed(tab) {
            return Verdict::Go;
        }
        self.ask_to_submit(paused, asks).await
    }

    /// The first changing request of a new tab may use only its opener's reserved grant.
    pub(in crate::agent_browser) async fn check_new_tab_send(
        &self,
        grant: Option<u64>,
        paused: &Paused,
    ) -> Verdict {
        if !submit::changing(&paused.method) {
            return Verdict::Go;
        }
        let asks = submit::request_asks_with_body(
            &paused.method,
            &paused.url,
            paused.body.as_deref(),
            paused.has_body,
        );
        if asks && grant.is_some_and(|id| self.take_pending_armed(id)) {
            return Verdict::Go;
        }
        self.ask_to_submit(paused, asks).await
    }

    /// The verdict for a request that may change something. `asks` says whether it carries what
    /// the user is asked about (spec 065 FR-011, amended 2026-11-08); Plan mode refuses it either
    /// way, and a rule the user remembered for the website counts either way.
    async fn ask_to_submit(&self, paused: &Paused, asks: bool) -> Verdict {
        if let Need::Refuse(code) = need(self.mode(), Effect::Submit) {
            return Verdict::Refuse(code);
        }
        let site = Site::parse(&paused.url).ok();
        // A rule the user remembered answers before anything is asked (FR-044).
        if let (Some(site), Some(rights)) = (&site, self.env.rights(&self.caller).await) {
            match rights.requests.get(site) {
                Some(RequestRule::Allow) => return Verdict::Go,
                Some(RequestRule::Deny) => return Verdict::Refuse("request_denied"),
                None => {}
            }
        }
        if !asks {
            return Verdict::Go;
        }
        let profile = self.env.profile_note(&self.caller).await;
        let question = BrowserQuestion {
            kind: BrowserPromptKind::Request,
            agent: self.env.agent_name(&self.caller).await,
            site: site.as_ref().map(ToString::to_string),
            tool: None,
            label: Some(format!("{} {}", paused.method, paused.url)),
            fields: submit::fields_of(paused.body.as_deref()),
            reason: None,
            bytes: None,
            entries: Vec::new(),
            profile: profile.clone(),
            mapping: Vec::new(),
        };
        let answer = match self.ask_for_page(&question).await {
            Ok(answer) => answer,
            Err(PromptEnd::Expired) => return Verdict::Refuse("no_answer"),
            Err(_) => return Verdict::Refuse("stopped"),
        };
        let allowed = answer.decision == BrowserDecision::Allow;
        if answer.remember == Some(Remember::Request) {
            if self
                .env
                .profile_note(&self.caller)
                .await
                .as_ref()
                .map(|p| p.id.as_str())
                != profile.as_ref().map(|p| p.id.as_str())
            {
                return Verdict::Refuse("profile_changed");
            }
            if let Some(site) = &site {
                let rule = if allowed {
                    RequestRule::Allow
                } else {
                    RequestRule::Deny
                };
                if let Err(error) = self
                    .env
                    .remember_request(
                        &self.caller,
                        site,
                        rule,
                        profile.as_ref().map(|p| p.id.as_str()),
                    )
                    .await
                {
                    log::warn!("agent browser: could not remember a request rule: {error}");
                }
            }
        }
        if allowed {
            Verdict::Go
        } else {
            Verdict::Refuse("user_declined")
        }
    }

    /// Asks `question`, which a page set off; the action that caused it waits for the answer.
    pub(in crate::agent_browser) async fn ask_for_page(
        &self,
        question: &BrowserQuestion,
    ) -> Result<prompts::BrowserAnswer, PromptEnd> {
        let mut question = question.clone();
        crate::agent_browser::executor::hide_filled(&self.filled(), &mut question);
        self.questions.fetch_add(1, Ordering::SeqCst);
        let answer = prompts::ask(&self.prompts, &self.closed, &question, &self.cancel).await;
        self.questions.fetch_sub(1, Ordering::SeqCst);
        answer
    }

    /// Whether a question a page set off is open.
    pub fn questions_open(&self) -> bool {
        self.questions.load(Ordering::SeqCst) > 0
    }

    pub(in crate::agent_browser) fn opened_tab_started(&self) {
        self.pending_opened_tabs.fetch_add(1, Ordering::SeqCst);
    }

    pub(in crate::agent_browser) fn opened_tab_finished(&self) {
        if self.pending_opened_tabs.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.opened_tabs_changed.notify_waiters();
        }
    }

    /// Waits until popups caused by the last browser action have joined the tabs or closed.
    pub(in crate::agent_browser) async fn wait_for_opened_tabs(&self, cancel: &CancellationToken) {
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
        let profile = self.env.profile_note(&self.caller).await;
        let question = BrowserQuestion {
            kind: BrowserPromptKind::Site,
            agent: self.env.agent_name(&self.caller).await,
            site: Some(site.to_string()),
            tool: None,
            label: Some(self.filled().scrub(url)),
            fields: Vec::new(),
            reason: None,
            bytes: None,
            entries: Vec::new(),
            profile: profile.clone(),
            mapping: Vec::new(),
        };
        match prompts::ask(&self.prompts, &self.closed, &question, &self.cancel).await {
            Ok(answer) if answer.decision == BrowserDecision::Allow => {
                if answer.remember.is_some()
                    && self
                        .env
                        .profile_note(&self.caller)
                        .await
                        .as_ref()
                        .map(|p| p.id.as_str())
                        != profile.as_ref().map(|p| p.id.as_str())
                {
                    return Verdict::Refuse("profile_changed");
                }
                self.allow_site(
                    &site,
                    answer.remember,
                    profile.as_ref().map(|p| p.id.as_str()),
                )
                .await;
                Verdict::Go
            }
            _ => Verdict::Refuse("site_not_allowed"),
        }
    }

    /// The user allowed `site` for this session, and with `remember` for the agent beyond it.
    pub(in crate::agent_browser) async fn allow_site(
        &self,
        site: &Site,
        remember: Option<Remember>,
        profile_id: Option<&str>,
    ) {
        self.once
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(site.clone());
        let level = match remember {
            Some(Remember::SiteRead) => Some(SiteLevel::Read),
            // `http` can only be read (research R9).
            Some(Remember::SiteOperate) if !site.is_plain_http() => Some(SiteLevel::Operate),
            Some(Remember::SiteOperate) => Some(SiteLevel::Read),
            // Remembering a fill is no answer to a website question.
            Some(Remember::Fill | Remember::Request) | None => None,
        };
        if let Some(level) = level {
            if let Err(error) = self
                .env
                .remember(&self.caller, site, level, profile_id)
                .await
            {
                log::warn!("agent browser: could not remember a website: {error}");
            }
        }
    }

    pub(in crate::agent_browser) async fn verdict(&self, url: &str) -> Verdict {
        let Some(rights) = self.env.rights(&self.caller).await else {
            return Verdict::Refuse("site_not_allowed");
        };
        check_document(
            url,
            &rights.sites,
            rights.all,
            &self.once_allowed_sites(),
            self.mode() != PermissionMode::Plan,
        )
    }
}
