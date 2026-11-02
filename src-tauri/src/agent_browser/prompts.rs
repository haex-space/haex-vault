//! The questions of the agent browser (spec 065, research R8, contracts/tauri-commands.md): they
//! park on a [`PromptPark`] for 5 minutes and appear in the window as one dialog with kinds, as the
//! call prompt of 021 does. They ask about websites (FR-007), in Manuell about actions that change
//! something, and in Manuell and Auto before anything is submitted (contracts/browser-tools.md
//! "Freigabe", [`need`]), before a tab a page opened becomes an agent tab, and when an agent asks
//! the user to take over (FR-018), and before a download over 100 MB is kept (FR-031).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use super::submit::BrowserField;
use crate::chat::tools::permission::PermissionMode;
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
    /// May the agent send this to the website (FR-011, FR-012)?
    Submit,
    /// May a page of an agent tab open this new tab for the agent (research R9)?
    NewTab,
    /// The agent asks the user to take over (FR-018), with its reason.
    Takeover,
    /// May the agent keep a download larger than 100 MB (FR-031)?
    Download,
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
    /// What the action targets, as text for the dialog (an address, an element's name).
    pub label: Option<String>,
    /// What would be sent, secrets as `•••` ([`BrowserPromptKind::Submit`]); empty otherwise.
    pub fields: Vec<BrowserField>,
    /// Why the agent asks the user to take over ([`BrowserPromptKind::Takeover`]), its own words.
    pub reason: Option<String>,
    /// The size of a download in bytes, as far as known ([`BrowserPromptKind::Download`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub bytes: Option<u64>,
}

/// What a browser tool does, for the approval table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Read,
    /// Changes something without submitting (navigating, typing, clicking).
    Change,
    /// Submits something (research R5 step 1 or 2).
    Submit,
}

/// What a tool needs before it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    Run,
    Ask(BrowserPromptKind),
    Refuse(&'static str),
}

/// The approval table of contracts/browser-tools.md "Freigabe" for an allowed website. One action
/// asks once at most: the submit question replaces the action question.
pub fn need(mode: PermissionMode, effect: Effect) -> Need {
    match (effect, mode) {
        (Effect::Read, _) => Need::Run,
        (_, PermissionMode::Plan) => Need::Refuse("plan_mode"),
        (Effect::Change, PermissionMode::Manual) => Need::Ask(BrowserPromptKind::Action),
        (Effect::Change, _) => Need::Run,
        (Effect::Submit, _) => Need::Ask(BrowserPromptKind::Submit),
    }
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
