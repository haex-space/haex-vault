//! `browser.takeover.request` (spec 065, FR-018, contracts/browser-tools.md): the agent asks the
//! user to take over, for a captcha, a second factor or a payment in a provider's window. holzi
//! shows the request with the agent's reason as its own claim; with the user's yes the tab comes
//! to the front, the user has the browser, and the call waits until the user gives it back. When
//! the user has the browser already, the call just waits.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::calls::{input, unknown_tab};
use super::executor::BrowserTools;
use super::prompts::{BrowserPromptKind, BrowserQuestion};
use super::session::{Front, Session};
use super::sessions::BrowserPause;
use crate::chat::tools::native_action::{NativeError, NativeReply};

/// The longest reason an agent may give.
const MAX_REASON: usize = 200;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TakeoverRequest {
    tab: String,
    reason: String,
}

/// Runs `browser.takeover.request`; `returned: true` once the user gave the browser back.
pub(super) async fn request(
    tools: &BrowserTools,
    session: &Arc<Session>,
    input_value: Value,
    cancel: &CancellationToken,
) -> Result<NativeReply, NativeError> {
    let TakeoverRequest { tab, reason } = input(input_value)?;
    let reason = reason.trim().to_owned();
    if reason.is_empty() || reason.chars().count() > MAX_REASON {
        return Err(NativeError::invalid_input(
            Some("reason"),
            "why the user should take over, at most 200 characters",
        ));
    }
    let connection = tools.connection().await?;
    let site = {
        let tabs = session.tabs_of(&connection).await;
        tabs.get(&tab).ok_or_else(|| unknown_tab(&tab))?.page().site
    };
    if !session.is_paused() {
        let question = BrowserQuestion {
            kind: BrowserPromptKind::Takeover,
            agent: String::new(),
            site: (!site.is_empty()).then_some(site),
            tool: None,
            label: None,
            fields: Vec::new(),
            reason: Some(reason.clone()),
            bytes: None,
        };
        tools.ask(session, question, cancel).await?;
        session
            .take_over(BrowserPause::Request { reason }, Front::Tab(tab.clone()))
            .await;
    }
    session.until_returned().await;
    Ok(json!({ "tab": tab, "returned": true }).into())
}
