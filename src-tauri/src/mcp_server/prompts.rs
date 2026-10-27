//! The registration dialog (spec 021 US1, FR-006, FR-009, FR-011): an unknown agent that connects
//! asks holzi's window, and the answer carries its rights and approval mode. The question parks on
//! `prompt_park.rs` for five minutes. At most one dialog per slug and three in all may be open; a
//! further unknown agent is turned away without a dialog.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::grants::Rights;
use super::registry::{AgentMode, Transport};
use crate::prompt_park::PromptPark;

/// How long a registration dialog waits (data-model.md, "Registrierungsdialog").
pub const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// How many registration dialogs may be open at once (FR-011).
pub const MAX_OPEN_REGISTRATIONS: usize = 3;

/// Event a registration question goes out on (contracts/tauri-commands.md).
pub const EVENT_REGISTRATION_REQUEST: &str = "mcp-registration-request";
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
