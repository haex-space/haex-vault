//! The questions of the agent browser (spec 065, research R8, contracts/tauri-commands.md): they
//! park on a [`PromptPark`] for 5 minutes and appear in the window as one dialog with kinds, as the
//! call prompt of 021 does. This delivery asks about websites (FR-007) and, in Manuell, about
//! actions that change something (contracts/browser-tools.md "Freigabe"); submitting, credentials,
//! taking over and downloads add their kinds later.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::prompt_park::{PromptEnd, PromptPark};

/// How long a question waits for an answer.
pub const PROMPT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Event a question goes out on.
pub const EVENT_PROMPT: &str = "agent-browser-prompt";
/// Event that closes a dialog nobody waits for any more.
pub const EVENT_PROMPT_CLOSED: &str = "agent-browser-prompt-closed";

/// What the user is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum BrowserPromptKind {
    /// May the agent open this website?
    Site,
    /// May the agent do this (Manuell, a tool that changes something)?
    Action,
}

/// What the dialog shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BrowserQuestion {
    pub kind: BrowserPromptKind,
    /// The agent's stated name, never checked.
    pub agent: String,
    /// The website concerned.
    pub site: Option<String>,
    /// The tool (`browser.open`) for [`BrowserPromptKind::Action`].
    pub tool: Option<String>,
    /// What the action targets, as text for the dialog (an address).
    pub label: Option<String>,
}

/// The user's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum BrowserDecision {
    Allow,
    Deny,
}

/// What an allowed website becomes beyond this session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum Remember {
    SiteRead,
    SiteOperate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BrowserAnswer {
    pub decision: BrowserDecision,
    #[serde(default)]
    #[ts(optional)]
    pub remember: Option<Remember>,
}

/// The payload of [`EVENT_PROMPT`].
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BrowserPromptEvent {
    pub request_id: String,
    /// Unix milliseconds.
    pub expires_at: i64,
    pub question: BrowserQuestion,
}

/// Why a dialog closed without the user's answer, or that it was answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum BrowserPromptClosedReason {
    Answered,
    Timeout,
    Cancelled,
}

/// The payload of [`EVENT_PROMPT_CLOSED`].
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BrowserPromptClosed {
    pub request_id: String,
    pub reason: BrowserPromptClosedReason,
}

pub type BrowserPrompts = PromptPark<BrowserQuestion, BrowserAnswer>;

pub fn new_browser_prompts() -> BrowserPrompts {
    PromptPark::new(PROMPT_TIMEOUT)
}

/// Tells the window that a dialog closed.
pub type ClosedNotice = std::sync::Arc<dyn Fn(BrowserPromptClosed) + Send + Sync>;

/// Asks `question` and tells `closed` how the dialog ended.
pub async fn ask(
    prompts: &BrowserPrompts,
    closed: &ClosedNotice,
    question: &BrowserQuestion,
    cancel: &CancellationToken,
) -> Result<BrowserAnswer, PromptEnd> {
    let mut request_id = String::new();
    let answer = prompts
        .ask(question, cancel, |id| request_id = id.to_owned())
        .await;
    let reason = match &answer {
        Ok(_) => BrowserPromptClosedReason::Answered,
        Err(PromptEnd::Expired) => BrowserPromptClosedReason::Timeout,
        Err(_) => BrowserPromptClosedReason::Cancelled,
    };
    if !request_id.is_empty() {
        closed(BrowserPromptClosed { request_id, reason });
    }
    answer
}
