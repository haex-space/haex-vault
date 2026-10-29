//! The questions to the user (spec 021): the registration dialog (US1, FR-006, FR-009, FR-011) and
//! the call prompt (US3, FR-021 to FR-023). An unknown agent that connects asks holzi's window, and
//! the answer carries its rights and approval mode; at most one dialog per slug and three in all
//! may be open, a further unknown agent is turned away without a dialog. A call asks when the
//! agent's approval mode says so. Both park on `prompt_park.rs` for five minutes.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::grants::Rights;
use super::registry::{AgentMode, Transport};
use crate::chat::tools::action_tool::{ActionEffect, ActionTitles, AgentActionDef};
use crate::chat::tools::permission::{decide_for_tool, Decision};
use crate::prompt_park::PromptPark;

/// How long a registration dialog waits (data-model.md, "Registrierungsdialog").
pub const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// How many registration dialogs may be open at once (FR-011).
pub const MAX_OPEN_REGISTRATIONS: usize = 3;

/// Event a registration question goes out on (contracts/tauri-commands.md).
pub const EVENT_REGISTRATION_REQUEST: &str = "mcp-registration-request";
/// Event a call prompt goes out on.
pub const EVENT_CALL_REQUEST: &str = "mcp-call-request";
/// Event that closes a dialog nobody waits for any more.
pub const EVENT_PROMPT_CLOSED: &str = "mcp-prompt-closed";
/// Event the settings reload on.
pub const EVENT_AGENTS_CHANGED: &str = "mcp-agents-changed";

/// What the dialog shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct RegistrationQuestion {
    pub slug: String,
    /// The agent's own claim (`clientInfo.name`), never checked.
    pub client_name: String,
    pub client_version: Option<String>,
    pub transport: Transport,
    /// Preset: reading in "holzi bedienen", nothing else (FR-009).
    pub rights: Rights,
    pub mode: AgentMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum RegistrationDecision {
    Register,
    Deny,
}

/// The window's answer (`mcp_registration_answer`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct RegistrationAnswer {
    pub decision: RegistrationDecision,
    /// For a denial: block the agent until the user lifts it.
    #[serde(default)]
    pub remember: bool,
    pub rights: Rights,
    pub mode: AgentMode,
}

/// The payload of [`EVENT_REGISTRATION_REQUEST`].
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct RegistrationPromptEvent {
    pub request_id: String,
    /// Unix milliseconds.
    pub expires_at: i64,
    pub question: RegistrationQuestion,
}

/// Why a dialog closed without the window answering it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum PromptClosedReason {
    Answered,
    Timeout,
    Cancelled,
}

/// The payload of [`EVENT_PROMPT_CLOSED`].
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct PromptClosed {
    pub request_id: String,
    pub reason: PromptClosedReason,
}

pub type RegistrationPrompts = PromptPark<RegistrationQuestion, RegistrationAnswer>;

pub fn new_registration_prompts() -> RegistrationPrompts {
    PromptPark::new(REGISTRATION_TIMEOUT)
}

/// How long a call prompt waits (FR-023).
pub const CALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// The scope whose actions hand out a secret; they ask in every approval mode (FR-021).
pub const SECRET_SCOPE: &str = "passwords.secret";

/// Whether a call runs, asks first, or is refused (FR-021): the approval mode of the agent decides
/// as for the built-in agent (ADR-0006), and a secret always asks.
pub fn call_gate(mode: AgentMode, def: &AgentActionDef, confirms_itself: bool) -> Decision {
    if def.scope == SECRET_SCOPE {
        return Decision::Ask;
    }
    // An action that asks the user itself (`confirmsItself`) is not asked about twice; Plan still
    // refuses it unless it only reads (spec 065 research R8).
    decide_for_tool(
        mode.permission_mode(),
        def.effect.risk_class(),
        confirms_itself,
    )
}

/// The localized names of an action, for the prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CallTitles {
    pub de: String,
    pub en: String,
}

impl From<&ActionTitles> for CallTitles {
    fn from(titles: &ActionTitles) -> Self {
        Self {
            de: titles.de.clone(),
            en: titles.en.clone(),
        }
    }
}

/// What a call prompt shows (FR-022).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CallQuestion {
    pub agent_id: String,
    /// The agent's own claim, never checked.
    pub client_name: String,
    pub action_id: String,
    pub titles: CallTitles,
    pub effect: CallEffect,
    pub target: Option<String>,
    /// The inputs without secrets (`redact.rs`).
    #[ts(type = "Record<string, unknown>")]
    pub input: serde_json::Value,
    /// The answer hands a secret to the agent and its provider.
    pub secret: bool,
}

/// The effect of the called action, as the prompt names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum CallEffect {
    Read,
    Write,
    Destructive,
}

impl From<ActionEffect> for CallEffect {
    fn from(effect: ActionEffect) -> Self {
        match effect {
            ActionEffect::Read => Self::Read,
            ActionEffect::Write => Self::Write,
            ActionEffect::Destructive => Self::Destructive,
        }
    }
}

/// The window's answer to a call prompt (`mcp_call_answer`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum CallDecision {
    Allow,
    Deny,
}

/// The payload of [`EVENT_CALL_REQUEST`].
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct CallPromptEvent {
    pub request_id: String,
    /// Unix milliseconds.
    pub expires_at: i64,
    pub question: CallQuestion,
}

pub type CallPrompts = PromptPark<CallQuestion, CallDecision>;

pub fn new_call_prompts() -> CallPrompts {
    PromptPark::new(CALL_TIMEOUT)
}

/// Keeps the count of open registration dialogs (FR-011). Cheap to clone.
#[derive(Clone, Default)]
pub struct RegistrationGate {
    open: Arc<Mutex<HashSet<String>>>,
}

/// One open slot; dropping it frees the slot.
pub struct GateTicket {
    gate: RegistrationGate,
    slug: String,
}

impl RegistrationGate {
    /// A slot for `slug`, or `None` when one is open for it or all are taken.
    pub fn try_open(&self, slug: &str) -> Option<GateTicket> {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        if open.len() >= MAX_OPEN_REGISTRATIONS || open.contains(slug) {
            return None;
        }
        open.insert(slug.to_owned());
        Some(GateTicket {
            gate: self.clone(),
            slug: slug.to_owned(),
        })
    }

    pub fn open_count(&self) -> usize {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

impl Drop for GateTicket {
    fn drop(&mut self) {
        self.gate
            .open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.slug);
    }
}
