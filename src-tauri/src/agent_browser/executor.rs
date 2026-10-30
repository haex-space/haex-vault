//! The browser tools as native catalog actions (spec 065, ADR 0011, contracts/browser-tools.md).
//! One executor serves the built-in agent and the MCP access; who calls and in which approval mode
//! comes with each call. It checks the rights group `browser`, asks in Manuell before a tool that
//! changes something (the general call prompt is skipped through `confirmsItself`), keeps one
//! session per agent and starts the browser on the first call (FR-003).
//!
//! This delivery brings opening, reading, screenshots, history, scrolling, links and waiting;
//! typing, clicking and submitting follow in PR D.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::cdp::{page, Connection};
use super::error::BrowserError;
use super::prompts::{
    self, BrowserDecision, BrowserPromptKind, BrowserPrompts, BrowserQuestion, ClosedNotice,
};
use super::read::{self, page_result};
use super::session::{Session, Tabs, MAX_TABS};
use super::sites::{check_document, Site, Verdict};
use super::tab::{Tab, TabError, LOAD_TIMEOUT};
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

/// What the executor needs from the app.
#[async_trait]
pub trait BrowserEnv: Send + Sync + 'static {
    /// The rights group `browser` of `caller` now; `None` when it is off.
    async fn rights(&self, caller: &Caller) -> Option<BrowserRights>;
    /// Adds `site` at `level` to the rights of `caller` ("Für diese Website merken").
    async fn remember(&self, caller: &Caller, site: &Site, level: SiteLevel) -> Result<(), String>;
    /// The name the dialogs show for `caller`.
    async fn agent_name(&self, caller: &Caller) -> String;
    /// How to start the browser now.
    async fn start_config(&self) -> Result<StartConfig, BrowserError>;
}

/// The executor of `browser.*`.
pub struct BrowserTools {
    env: Arc<dyn BrowserEnv>,
    browser: AgentBrowser,
    prompts: BrowserPrompts,
    closed: ClosedNotice,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

impl BrowserTools {
    pub fn new(
        env: Arc<dyn BrowserEnv>,
        browser: AgentBrowser,
        prompts: BrowserPrompts,
        closed: ClosedNotice,
    ) -> Self {
        Self {
            env,
            browser,
            prompts,
            closed,
            sessions: Mutex::default(),
        }
    }

    fn session(&self, caller: &Caller) -> Result<Arc<Session>, NativeError> {
        let key = match caller {
            Caller::BuiltinAgent => "builtin".to_owned(),
            Caller::ExternalAgent { id } => format!("agent:{id}"),
            _ => return Err(not_allowed()),
        };
        let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(Arc::clone(sessions.entry(key).or_insert_with(|| {
            Session::new(
                caller.clone(),
                Arc::clone(&self.env),
                self.prompts.clone(),
                Arc::clone(&self.closed),
            )
        })))
    }

    /// Ends the session of `caller`: its tabs close, its questions end.
    pub async fn end_session(&self, caller: &Caller) {
        let session = {
            let key = match caller {
                Caller::BuiltinAgent => "builtin".to_owned(),
                Caller::ExternalAgent { id } => format!("agent:{id}"),
                _ => return,
            };
            self.sessions
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&key)
        };
        if let Some(session) = session {
            session.end().await;
        }
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Open {
    url: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OnTab {
    tab: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadPage {
    tab: String,
    #[serde(default)]
    from: usize,
    section: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Navigate {
    tab: String,
    to: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scroll {
    tab: String,
    direction: String,
    element: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FollowLink {
    tab: String,
    element: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wait {
    tab: String,
    #[serde(rename = "for")]
    until: String,
    text: Option<String>,
}

fn input<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, NativeError> {
    serde_json::from_value(value)
        .map_err(|error| NativeError::invalid_input(None, error.to_string()))
}

fn not_allowed() -> NativeError {
    NativeError::new("not_allowed", "The browser is not allowed for this agent.")
}

fn browser_error(error: BrowserError) -> NativeError {
    match error {
        BrowserError::Protocol(message) if message == "element_gone" => NativeError::new(
            "element_gone",
            "This element number no longer exists; read the page again.",
        ),
        other => NativeError::new(other.code(), other.to_string()),
    }
}

fn tab_error(error: TabError) -> NativeError {
    match error {
        TabError::Refused("blocked_address") => NativeError::new(
            "blocked_address",
            "This address can never be opened by an agent.",
        ),
        TabError::Refused(_) => NativeError::new(
            "site_not_allowed",
            "This website is not allowed for this agent.",
        ),
        TabError::Timeout => {
            NativeError::new("page_timeout", "The page did not load within 30 seconds.")
        }
        TabError::Browser(error) => browser_error(error),
    }
}

fn unknown_tab(tab: &str) -> NativeError {
    NativeError::invalid_input(Some("tab"), format!("no agent tab {tab}"))
}

/// The website of `url` as text; empty for `about:blank`.
fn site_of(url: &str) -> String {
    Site::parse(url)
        .map(|site| site.to_string())
        .unwrap_or_default()
}

#[async_trait]
impl NativeExecutor for BrowserTools {
    fn action_ids(&self) -> &'static [&'static str] {
        BROWSER_ACTION_IDS
    }

    fn confirming_action_ids(&self) -> &'static [&'static str] {
        CHANGING
    }

    async fn run(
        &self,
        action_id: &str,
        input_value: Value,
        call: &NativeCall,
        cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        if !crate::platform::capabilities().agent_browser {
            return Err(browser_error(BrowserError::Unsupported));
        }
        let Some(rights) = self.env.rights(&call.caller).await else {
            return Err(not_allowed());
        };
        if call.mode == PermissionMode::Plan && CHANGING.contains(&action_id) {
            return Err(NativeError::new(
                "plan_mode",
                "The approval mode of this agent only lets reading actions run.",
            ));
        }
        let session = self.session(&call.caller)?;
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
                        &session,
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
            self.confirm(&session, action_id, input_value.to_string(), None, cancel)
                .await?;
        }

        let connection = self.connection().await?;
        let mut tabs = session.tabs_of(&connection).await;
        tabs.last_action = Some(action_id.to_owned());
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
                    None => new_tab(&connection, &session, &mut tabs).await?,
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
        Ok(reply)
    }
}

/// Opens a new tab in `tabs`, at most [`MAX_TABS`].
async fn new_tab(
    connection: &Arc<Connection>,
    session: &Arc<Session>,
    tabs: &mut Tabs,
) -> Result<String, NativeError> {
    if tabs.open.len() >= MAX_TABS {
        return Err(NativeError::new(
            "too_many_tabs",
            "An agent has at most 5 tabs; close one first.",
        ));
    }
    let id = tabs.next_id();
    let tab = Tab::open(connection, id.clone(), session.guard())
        .await
        .map_err(browser_error)?;
    tabs.open.push(tab);
    Ok(id)
}

/// The page result of `tab`.
async fn read_tab(
    connection: &Connection,
    tab: &Tab,
    from: usize,
    section: Option<u32>,
) -> Result<NativeReply, NativeError> {
    let raw = read::raw(connection, &tab.session, section)
        .await
        .map_err(browser_error)?;
    if raw.gone {
        return Err(browser_error(BrowserError::Protocol(
            "element_gone".to_owned(),
        )));
    }
    let site = site_of(&raw.url);
    Ok(NativeReply {
        result: page_result(&tab.id, &site, raw, from),
        images: Vec::new(),
        target: Some(site),
    })
}

/// Waits up to [`LOAD_TIMEOUT`] until the page shows `wanted`.
async fn wait_for_text(tab: &Tab, wanted: &str) -> Result<(), NativeError> {
    let found = tokio::time::timeout(LOAD_TIMEOUT, async {
        loop {
            if tab.text().await.is_ok_and(|text| text.contains(wanted)) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await;
    found
        .map_err(|_| NativeError::new("page_timeout", "The text did not appear within 30 seconds."))
}

/// Waits up to [`LOAD_TIMEOUT`] until the main document is complete.
async fn wait_for_load(tab: &Tab) -> Result<(), NativeError> {
    let loaded = tokio::time::timeout(LOAD_TIMEOUT, async {
        loop {
            if tab.ready().await.unwrap_or(false) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await;
    loaded.map_err(|_| {
        NativeError::new(
            "page_timeout",
            "The page did not finish loading within 30 seconds.",
        )
    })?;
    Ok(())
}
