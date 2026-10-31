//! Tabs a page of an agent tab opens itself (`window.open`, a link or form with `target=_blank`;
//! research R9, data-model.md "Browser-Rückfrage" `newTab`). Such a tab waits for holzi, as every
//! frame of an agent tab does, and attaches to the browser, not to the tab that opened it; the
//! session that owns the opener takes it over ([`opened`]). Its first document asks the user:
//! always in Manuell, in Auto when its website is not allowed (the answer then allows the website
//! once, so nothing asks twice), and Plan refuses it. Refused, it closes; at [`MAX_TABS`] it
//! closes at once. Allowed, it is an agent tab like any other, under the same checks. While the
//! user has taken over, a new tab stays the user's.
//!
//! The call that opened it holds the session's tabs until the browser lets the opener go on, which
//! it does only once the new tab runs. So the new tab is set up and runs first, and joins the tabs
//! when the call is done.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use serde_json::Value;

use super::cdp::{target, Connection};
use super::prompts::{BrowserDecision, BrowserPromptKind, BrowserQuestion};
use super::session::Session;
use super::sites::{Site, Verdict};
use super::tab::{Guard, Paused, Tab};
use crate::chat::tools::permission::PermissionMode;
use crate::prompt_park::PromptEnd;

/// Takes over the new page target of `attached` (a `Target.attachedToTarget` of the browser,
/// waiting for holzi) when a page or frame of one of `session`'s tabs opened it; `false` leaves it.
pub(super) fn opened(
    session: &Arc<Session>,
    connection: &Arc<Connection>,
    attached: &Value,
) -> bool {
    let info = &attached["targetInfo"];
    if info["type"] != "page" {
        return false;
    }
    let (Some(opener), Some(target), Some(cdp_session)) = (
        info["openerId"].as_str(),
        info["targetId"].as_str(),
        attached["sessionId"].as_str(),
    ) else {
        return false;
    };
    // While the user has the browser, a tab the user opens stays the user's.
    if session.is_paused() {
        return false;
    }
    let Some(opener) = session.tab_of_target(opener) else {
        return false;
    };
    let target = target.to_owned();
    let Some(id) = session.reserve_new_tab(&target) else {
        let connection = Arc::clone(connection);
        tokio::spawn(async move {
            let _ = target::close(&connection, &target).await;
        });
        return true;
    };
    let submit_grant = session.reserve_submit_for_new_tab(&opener);
    tokio::spawn(adopt(
        Arc::clone(session),
        Arc::clone(connection),
        target,
        cdp_session.to_owned(),
        id,
        submit_grant,
    ));
    true
}

async fn adopt(
    session: Arc<Session>,
    connection: Arc<Connection>,
    target: String,
    cdp_session: String,
    id: String,
    submit_grant: Option<u64>,
) {
    if session.cancel.is_cancelled() {
        let _ = target::close(&connection, &target).await;
        session.forget_targets(&id);
        session.finish_pending_new_tab(submit_grant);
        return;
    }
    let refused = Arc::new(AtomicBool::new(false));
    let guard = guard(&session, id.clone(), submit_grant, Arc::clone(&refused));
    let hooks = session.tab_hooks(&id).await;
    let Ok(tab) = Tab::adopt(&connection, id.clone(), target, cdp_session, guard, hooks).await
    else {
        session.forget_targets(&id);
        session.finish_pending_new_tab(submit_grant);
        return;
    };
    let mut tabs = session.tabs.lock().await;
    if refused.load(Ordering::SeqCst) || !Session::same_run(&tabs, &connection) {
        drop(tabs);
        session.forget_targets(&id);
        session.finish_pending_new_tab(submit_grant);
        tab.close().await;
    } else if session.cancel.is_cancelled() {
        drop(tabs);
        session.forget_targets(&id);
        session.finish_pending_new_tab(submit_grant);
        tab.release().await;
    } else {
        tabs.open.push(tab);
        session.note(&tabs, None);
    }
}

/// The guard of the new tab `tab`: its first document asks ([`Session::check_new_tab`]), and a
/// changing one counts as sent by the action of `opener` that the user may already have allowed;
/// later requests are checked like those of any tab. A refused first document closes the tab.
fn guard(
    session: &Arc<Session>,
    tab: String,
    submit_grant: Option<u64>,
    refused: Arc<AtomicBool>,
) -> Guard {
    let session = Arc::downgrade(session);
    let first = Arc::new(AtomicBool::new(true));
    Arc::new(move |paused: Paused| {
        let (session, tab) = (session.clone(), tab.clone());
        let (first, refused) = (Arc::clone(&first), Arc::clone(&refused));
        let submit_grant = submit_grant;
        Box::pin(async move {
            let Some(session) = session.upgrade() else {
                return Verdict::Refuse("site_not_allowed");
            };
            if !paused.document || !first.swap(false, Ordering::SeqCst) {
                return session.check_request(&tab, paused).await;
            }
            let verdict = match session.check_new_tab(&paused.url).await {
                Verdict::Go => session.check_new_tab_send(submit_grant, &paused).await,
                other => other,
            };
            session.finish_pending_new_tab(submit_grant);
            if verdict != Verdict::Go {
                refused.store(true, Ordering::SeqCst);
                tokio::spawn(close(Arc::downgrade(&session), tab));
            }
            verdict
        })
    })
}

/// Closes the new tab `tab` once the call that opened it is done; before it joined the tabs, the
/// adoption closes it instead.
async fn close(session: Weak<Session>, tab: String) {
    let Some(session) = session.upgrade() else {
        return;
    };
    let mut tabs = session.tabs.lock().await;
    let Some(index) = tabs.open.iter().position(|open| open.id == tab) else {
        return;
    };
    let closing = tabs.open.remove(index);
    if tabs.current.as_deref() == Some(tab.as_str()) {
        tabs.current = None;
    }
    session.forget_targets(&tab);
    session.note(&tabs, None);
    drop(tabs);
    closing.close().await;
}

impl Session {
    /// The verdict for the first document `url` of a tab a page opened.
    pub(super) async fn check_new_tab(&self, url: &str) -> Verdict {
        let mode = self.mode();
        if mode == PermissionMode::Plan {
            return Verdict::Refuse("plan_mode");
        }
        if let Verdict::Refuse(code) = self.verdict(url).await {
            return Verdict::Refuse(code);
        }
        // One question at a time: after waiting, the answer to another may already cover this one.
        let _asking = self.asking.lock().await;
        let site = match self.verdict(url).await {
            Verdict::Go if mode != PermissionMode::Manual => return Verdict::Go,
            Verdict::Go => None,
            Verdict::Ask(site) => Some(site),
            Verdict::Refuse(code) => return Verdict::Refuse(code),
        };
        let question = BrowserQuestion {
            kind: BrowserPromptKind::NewTab,
            agent: self.agent_name().await,
            site: Site::parse(url).ok().map(|site| site.to_string()),
            tool: None,
            label: Some(url.to_owned()),
            fields: Vec::new(),
            reason: None,
        };
        match self.ask_for_page(&question).await {
            Ok(answer) if answer.decision == BrowserDecision::Allow => {
                if let Some(site) = site {
                    self.allow_site(&site, None).await;
                }
                Verdict::Go
            }
            Ok(_) => Verdict::Refuse("user_declined"),
            Err(PromptEnd::Expired) => Verdict::Refuse("no_answer"),
            Err(_) => Verdict::Refuse("stopped"),
        }
    }
}
