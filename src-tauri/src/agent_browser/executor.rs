//! The browser tools as native catalog actions (spec 065, ADR 0011, contracts/browser-tools.md).
//! One executor serves the built-in agent and the MCP access; who calls and in which approval mode
//! comes with each call. It checks the rights group `browser`, asks in Manuell before a tool that
//! changes something (the general call prompt is skipped through `confirmsItself`), keeps one
//! session per agent in [`BrowserSessions`] and starts the browser on the first call (FR-003). A
//! stop of the session ends the running call within a second (FR-016).
//!
//! This delivery brings opening, reading, screenshots, history, scrolling, links and waiting;
//! typing, clicking and submitting follow in PR D.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::calls::{
    browser_error, input, new_tab, not_allowed, read_tab, site_of, stopped, tab_error, unknown_tab,
    wait_for_load, wait_for_text, FollowLink, Navigate, OnTab, Open, ReadPage, Scroll, Wait,
};
use super::cdp::{page, Connection};
use super::error::BrowserError;
use super::prompts::{
    self, BrowserDecision, BrowserPromptKind, BrowserPrompts, BrowserQuestion, ClosedNotice,
};
use super::read;
use super::session::Session;
use super::sessions::{BrowserSessions, NoSession};
use super::sites::{check_document, Site, Verdict};
use super::tab::{PageInfo, TabError};
use super::{AgentBrowser, StartConfig};
use crate::caller::Caller;
use crate::chat::tools::native_action::{NativeCall, NativeError, NativeExecutor, NativeReply};
use crate::chat::tools::permission::PermissionMode;
use crate::chat::tools::ToolImage;
use crate::mcp_server::grants::{BrowserRights, SiteLevel};
use crate::prompt_park::PromptEnd;

/// The tools that only read.
pub const READING: &[&str] = &[
    "browser.page.read",
    "browser.page.screenshot",
    "browser.scroll",
    "browser.tabs.list",
    "browser.wait",
];

/// The tools that change something (every navigation counts, contracts/browser-tools.md); they ask
/// the user themselves.
pub const CHANGING: &[&str] = &[
    "browser.open",
    "browser.tabs.open",
    "browser.navigate",
    "browser.follow_link",
];

pub const BROWSER_ACTION_IDS: &[&str] = &[
    "browser.page.read",
    "browser.page.screenshot",
    "browser.scroll",
    "browser.tabs.list",
    "browser.wait",
    "browser.open",
    "browser.tabs.open",
    "browser.navigate",
    "browser.follow_link",
];

/// A screenshot is at most this wide (research R12).
const SCREENSHOT_WIDTH: f64 = 1280.0;

/// How long a stopped call may take to close its questions before it is dropped (FR-016: the
/// whole stop within a second).
const STOP_GRACE: Duration = Duration::from_millis(300);

/// What the executor needs from the app.
#[async_trait]
pub trait BrowserEnv: Send + Sync + 'static {
    /// The rights group `browser` of `caller` now; `None` when it is off.
    async fn rights(&self, caller: &Caller) -> Option<BrowserRights>;
    /// Adds `site` at `level` to the rights of `caller` ("Für diese Website merken").
    async fn remember(&self, caller: &Caller, site: &Site, level: SiteLevel) -> Result<(), String>;
    /// The name the dialogs show for `caller`.
    async fn agent_name(&self, caller: &Caller) -> String;
    /// The language of holzi's window, for the marker in the tabs.
    fn locale(&self) -> String;
    /// How to start the browser now.
    async fn start_config(&self) -> Result<StartConfig, BrowserError>;
}

/// The executor of `browser.*`.
pub struct BrowserTools {
    env: Arc<dyn BrowserEnv>,
    browser: AgentBrowser,
    prompts: BrowserPrompts,
    closed: ClosedNotice,
    sessions: BrowserSessions,
}

impl BrowserTools {
    pub fn new(
        env: Arc<dyn BrowserEnv>,
        browser: AgentBrowser,
        prompts: BrowserPrompts,
        closed: ClosedNotice,
        sessions: BrowserSessions,
    ) -> Self {
        Self {
            env,
            browser,
            prompts,
            closed,
            sessions,
        }
    }

    async fn session(&self, caller: &Caller) -> Result<Arc<Session>, NativeError> {
        let session = self
            .sessions
            .get_or_start(caller, |id, hooks| {
                Session::new(
                    id,
                    caller.clone(),
                    Arc::clone(&self.env),
                    self.prompts.clone(),
                    Arc::clone(&self.closed),
                    hooks,
                )
            })
            .map_err(|refused| match refused {
                NoSession::NotAnAgent => not_allowed(),
                NoSession::Stopped => stopped(),
            })?;
        // The sessions bar shows the agent's name from the start.
        session.agent_name().await;
        Ok(session)
    }

    /// Ends the session of `caller`: its questions end, its tabs go back to the user.
    pub async fn end_session(&self, caller: &Caller) {
        self.sessions.end(caller).await;
    }

    /// Asks in Manuell before a tool that changes something.
    async fn confirm(
        &self,
        session: &Session,
        tool: &str,
        label: String,
        site: Option<String>,
        cancel: &CancellationToken,
    ) -> Result<(), NativeError> {
        let question = BrowserQuestion {
            kind: BrowserPromptKind::Action,
            agent: self.env.agent_name(&session.caller).await,
            site,
            tool: Some(tool.to_owned()),
            label: Some(label),
        };
        match prompts::ask(&self.prompts, &self.closed, &question, cancel).await {
            Ok(answer) if answer.decision == BrowserDecision::Allow => Ok(()),
            Ok(_) => Err(NativeError::new(
                "user_declined",
                "The user declined this action.",
            )),
            Err(PromptEnd::Expired) => Err(NativeError::new(
                "no_answer",
                "The user did not answer in time.",
            )),
            Err(_) => Err(NativeError::new(
                "unavailable",
                "Nobody could be asked; try again when holzi is open.",
            )),
        }
    }

    async fn connection(&self) -> Result<Arc<Connection>, NativeError> {
        let config = self.env.start_config().await;
        self.browser
            .ensure(move || config)
            .await
            .map_err(browser_error)
    }
}

#[async_trait]
impl NativeExecutor for BrowserTools {
    fn action_ids(&self) -> &'static [&'static str] {
        BROWSER_ACTION_IDS
    }

    fn confirming_action_ids(&self) -> &'static [&'static str] {
        CHANGING
    }

    /// Runs one tool; a stop of the session ends it within a second. Its questions hear the stop
    /// first and close, so no dialog stays behind.
    async fn run(
        &self,
        action_id: &str,
        input_value: Value,
        call: &NativeCall,
        cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        let session = self.session_for(action_id, call).await?;
        let ended = session.cancel.child_token();
        let forward = {
            let (cancel, ended) = (cancel.clone(), ended.clone());
            tokio::spawn(async move {
                tokio::select! {
                    _ = cancel.cancelled() => ended.cancel(),
                    _ = ended.cancelled() => {}
                }
            })
        };
        let serving = self.serve(&session, action_id, input_value, call, &ended);
        tokio::pin!(serving);
        let outcome = tokio::select! {
            outcome = &mut serving => outcome,
            _ = session.cancel.cancelled() => {
                let _ = tokio::time::timeout(STOP_GRACE, &mut serving).await;
                Err(stopped())
            }
        };
        forward.abort();
        outcome
    }
}

impl BrowserTools {
    /// The agent's session, when the call may use the browser at all.
    async fn session_for(
        &self,
        action_id: &str,
        call: &NativeCall,
    ) -> Result<Arc<Session>, NativeError> {
        if !crate::platform::capabilities().agent_browser {
            return Err(browser_error(BrowserError::Unsupported));
        }
        if self.env.rights(&call.caller).await.is_none() {
            return Err(not_allowed());
        }
        if call.mode == PermissionMode::Plan && CHANGING.contains(&action_id) {
            return Err(NativeError::new(
                "plan_mode",
                "The approval mode of this agent only lets reading actions run.",
            ));
        }
        self.session(&call.caller).await
    }

    async fn serve(
        &self,
        session: &Arc<Session>,
        action_id: &str,
        input_value: Value,
        call: &NativeCall,
        cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        // The rights now: they may have changed while the call waited for its session.
        let Some(rights) = self.env.rights(&call.caller).await else {
            return Err(not_allowed());
        };
        session.set_mode(call.mode);
        let _lease = self.browser.lease();
        let manual = call.mode == PermissionMode::Manual;

        // A new address is checked before anything starts: a refused one never loads, and one that
        // will ask about its website does not ask about the action as well.
        let address = match action_id {
            "browser.open" | "browser.tabs.open" => Some(input::<Open>(input_value.clone())?.url),
            "browser.navigate" => {
                let to = input::<Navigate>(input_value.clone())?.to;
                (!matches!(to.as_str(), "back" | "forward" | "reload")).then_some(to)
            }
            _ => None,
        };
        if let Some(address) = &address {
            let verdict = check_document(
                address,
                &rights.sites,
                &session.once_allowed_sites(),
                call.mode != PermissionMode::Plan,
            );
            match verdict {
                Verdict::Refuse(code) => return Err(tab_error(TabError::Refused(code))),
                Verdict::Go if manual => {
                    self.confirm(
                        session,
                        action_id,
                        address.clone(),
                        Some(site_of(address)),
                        cancel,
                    )
                    .await?;
                }
                _ => {}
            }
        } else if manual && CHANGING.contains(&action_id) {
            self.confirm(session, action_id, input_value.to_string(), None, cancel)
                .await?;
        }

        let connection = self.connection().await?;
        let mut tabs = session.tabs_of(&connection).await;
        let reply = match action_id {
            "browser.open" | "browser.tabs.open" => {
                let url = address.unwrap_or_default();
                let reuse = action_id == "browser.open";
                let id = match tabs
                    .current
                    .clone()
                    .filter(|id| reuse && tabs.get(id).is_some())
                {
                    Some(id) => id,
                    None => new_tab(&connection, session, &mut tabs).await?,
                };
                tabs.current = Some(id.clone());
                let tab = tabs.get(&id).ok_or_else(|| unknown_tab(&id))?;
                tab.go(&url).await.map_err(tab_error)?;
                read_tab(&connection, tab, 0, None).await?
            }
            "browser.page.read" => {
                let ReadPage { tab, from, section } = input(input_value)?;
                let tab = tabs.get(&tab).ok_or_else(|| unknown_tab(&tab))?;
                read_tab(&connection, tab, from, section).await?
            }
            "browser.page.screenshot" => {
                let OnTab { tab } = input(input_value)?;
                let tab = tabs.get(&tab).ok_or_else(|| unknown_tab(&tab))?;
                let jpeg = page::screenshot(&connection, &tab.session, SCREENSHOT_WIDTH)
                    .await
                    .map_err(browser_error)?;
                let raw = read::raw(&connection, &tab.session, None)
                    .await
                    .map_err(browser_error)?;
                let site = site_of(&raw.url);
                tab.set_page(PageInfo {
                    site: site.clone(),
                    title: raw.title.clone(),
                });
                NativeReply {
                    result: json!({
                        "tab": tab.id,
                        "untrusted_page_content": { "site": site, "url": raw.url, "title": raw.title },
                        "note": read::NOTE,
                    }),
                    images: vec![ToolImage {
                        name: format!("{}.jpg", tab.id),
                        media_type: "image/jpeg".to_owned(),
                        bytes: jpeg,
                    }],
                    target: Some(site),
                }
            }
            "browser.navigate" => {
                let Navigate { tab, to } = input(input_value)?;
                let tab = tabs.get(&tab).ok_or_else(|| unknown_tab(&tab))?;
                match to.as_str() {
                    "back" => tab.step(true).await,
                    "forward" => tab.step(false).await,
                    "reload" => tab.reload().await,
                    url => tab.go(url).await,
                }
                .map_err(tab_error)?;
                read_tab(&connection, tab, 0, None).await?
            }
            "browser.follow_link" => {
                let FollowLink { tab, element } = input(input_value)?;
                let tab = tabs.get(&tab).ok_or_else(|| unknown_tab(&tab))?;
                let link = read::script(&connection, &tab.session, &json!({ "hrefOf": element }))
                    .await
                    .map_err(browser_error)?;
                let Some(href) = link["href"].as_str() else {
                    return Err(browser_error(BrowserError::Protocol(
                        "element_gone".to_owned(),
                    )));
                };
                if let Verdict::Refuse(code) = session.check(href).await {
                    return Err(tab_error(TabError::Refused(code)));
                }
                tab.go(href).await.map_err(tab_error)?;
                read_tab(&connection, tab, 0, None).await?
            }
            "browser.scroll" => {
                let Scroll {
                    tab,
                    direction,
                    element,
                } = input(input_value)?;
                let down = match direction.as_str() {
                    "down" => true,
                    "up" => false,
                    _ => return Err(NativeError::invalid_input(Some("direction"), "up or down")),
                };
                let tab = tabs.get(&tab).ok_or_else(|| unknown_tab(&tab))?;
                tab.scroll(down, element).await.map_err(browser_error)?;
                read_tab(&connection, tab, 0, None).await?
            }
            "browser.tabs.list" => {
                let mut list = Vec::new();
                for tab in &tabs.open {
                    let raw = read::raw(&connection, &tab.session, None)
                        .await
                        .unwrap_or_default();
                    list.push(
                        json!({ "tab": tab.id, "site": site_of(&raw.url), "title": raw.title }),
                    );
                }
                json!({ "untrusted_page_content": { "tabs": list }, "note": read::NOTE }).into()
            }
            "browser.wait" => {
                let Wait { tab, until, text } = input(input_value)?;
                let tab = tabs.get(&tab).ok_or_else(|| unknown_tab(&tab))?;
                if until == "text" {
                    let wanted = text.filter(|text| !text.trim().is_empty()).ok_or_else(|| {
                        NativeError::invalid_input(Some("text"), "the text to wait for")
                    })?;
                    wait_for_text(tab, &wanted).await?;
                } else if until == "load" {
                    wait_for_load(tab).await?;
                } else {
                    return Err(NativeError::invalid_input(Some("for"), "load or text"));
                }
                read_tab(&connection, tab, 0, None).await?
            }
            _ => return Err(NativeError::new("not_allowed", "unknown browser tool")),
        };
        let site = reply.target.clone().unwrap_or_default();
        session.note(&tabs, Some((action_id, site)));
        Ok(reply)
    }
}
