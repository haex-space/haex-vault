//! The parts of a browser tool call (spec 065, contracts/browser-tools.md): the inputs, the errors
//! an agent sees, and the steps on a tab that several tools share.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use super::cdp::Connection;
use super::error::BrowserError;
use super::read::{self, page_result};
use super::session::{Session, Tabs, MAX_TABS};
use super::sites::Site;
use super::tab::{PageInfo, Tab, TabError, LOAD_TIMEOUT};
use crate::chat::tools::native_action::{NativeError, NativeReply};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Open {
    pub(super) url: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OnTab {
    pub(super) tab: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadPage {
    pub(super) tab: String,
    #[serde(default)]
    pub(super) from: usize,
    pub(super) section: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Navigate {
    pub(super) tab: String,
    pub(super) to: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Scroll {
    pub(super) tab: String,
    pub(super) direction: String,
    pub(super) element: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FollowLink {
    pub(super) tab: String,
    pub(super) element: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Wait {
    pub(super) tab: String,
    #[serde(rename = "for")]
    pub(super) until: String,
    pub(super) text: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Click {
    pub(super) tab: String,
    pub(super) element: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TypeText {
    pub(super) tab: String,
    pub(super) element: u32,
    pub(super) text: String,
    #[serde(default)]
    pub(super) submit: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Select {
    pub(super) tab: String,
    pub(super) element: u32,
    pub(super) value: String,
}

pub(super) fn input<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, NativeError> {
    serde_json::from_value(value)
        .map_err(|error| NativeError::invalid_input(None, error.to_string()))
}

pub(super) fn not_allowed() -> NativeError {
    NativeError::new("not_allowed", "The browser is not allowed for this agent.")
}

pub(super) fn stopped() -> NativeError {
    NativeError::new(
        "stopped",
        "The user stopped this browser session. Do not continue in the browser unless the user \
         asks again.",
    )
}

pub(super) fn browser_error(error: BrowserError) -> NativeError {
    match error {
        BrowserError::Protocol(message) if message == "element_gone" => NativeError::new(
            "element_gone",
            "This element number no longer exists; read the page again.",
        ),
        other => NativeError::new(other.code(), other.to_string()),
    }
}

pub(super) fn tab_error(error: TabError) -> NativeError {
    match error {
        TabError::Refused("blocked_address") => NativeError::new(
            "blocked_address",
            "This address can never be opened by an agent.",
        ),
        TabError::Refused("user_declined") => NativeError::new(
            "user_declined",
            "The user declined to send this to the website.",
        ),
        TabError::Refused("no_answer") => NativeError::new(
            "no_answer",
            "The user did not answer in time; nothing was sent.",
        ),
        TabError::Refused("plan_mode") => NativeError::new(
            "plan_mode",
            "The approval mode of this agent lets nothing be sent to a website.",
        ),
        TabError::Refused("stopped") => stopped(),
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

pub(super) fn unknown_tab(tab: &str) -> NativeError {
    NativeError::invalid_input(Some("tab"), format!("no agent tab {tab}"))
}

/// The website of `url` as text; empty for `about:blank`.
pub(super) fn site_of(url: &str) -> String {
    Site::parse(url)
        .map(|site| site.to_string())
        .unwrap_or_default()
}

/// Opens a new tab in `tabs`, at most [`MAX_TABS`].
pub(super) async fn new_tab(
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
    let tab = Tab::open(
        connection,
        id.clone(),
        session.guard(id.clone()),
        session.marker().await,
    )
    .await
    .map_err(browser_error)?;
    tabs.open.push(tab);
    Ok(id)
}

/// The page result of `tab`.
pub(super) async fn read_tab(
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
    tab.set_page(PageInfo {
        site: site.clone(),
        title: raw.title.clone(),
    });
    Ok(NativeReply {
        result: page_result(&tab.id, &site, raw, from),
        images: Vec::new(),
        target: Some(site),
    })
}

/// Waits up to [`LOAD_TIMEOUT`] until the page shows `wanted`.
pub(super) async fn wait_for_text(tab: &Tab, wanted: &str) -> Result<(), NativeError> {
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
pub(super) async fn wait_for_load(tab: &Tab) -> Result<(), NativeError> {
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
