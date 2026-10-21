//! The questions of the bridge to the user (spec 049, research R10, `contracts/tauri-commands.md`):
//! approval of a client (FR-008 to FR-010) and writing with a read-only approval (FR-015), parked
//! on [`crate::prompt_park`] for at most 120 s.
//! Only the window answers them; no client, extension or agent reaches the answer command (FR-013).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::declaration::BridgeAccess;
use crate::prompt_park::PromptPark;

/// How long a question waits for the user (FR-015, FR-030).
pub const PROMPT_TIMEOUT: Duration = Duration::from_secs(120);

/// Event a question goes out on.
pub const EVENT_PROMPT: &str = "browser-bridge-prompt";
/// Event that closes a question in the window: timed out, connection gone, bridge stopped.
pub const EVENT_PROMPT_CANCELLED: &str = "browser-bridge-prompt-cancelled";

/// The client a question is about. The name is the client's own claim (FR-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BridgeClientRef {
    pub client_id: String,
    pub client_name: String,
    pub origin: String,
}

/// What the client declared, in the terms of a grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BridgeDeclaredGrant {
    pub access: BridgeAccess,
    /// `null`: every entry.
    pub scope_tags: Option<Vec<String>>,
}

/// A question of the bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum BridgeQuestion {
    /// May this client connect, and with which grant (FR-008 to FR-010)?
    Approval(ApprovalQuestion),
    /// May a client with a read-only approval write this once or from now on (FR-015)?
    Write(WriteQuestion),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct WriteQuestion {
    pub client: BridgeClientRef,
    /// The method that writes, as the extension named it (`create-item`, `passkey-create`).
    pub method: String,
    /// The scope of the approval; `null`: every entry.
    pub scope_tags: Option<Vec<String>>,
    /// Whether allowing needs a default tag inside the scope (a tag scope without one).
    pub needs_default_tag: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ApprovalQuestion {
    pub client: BridgeClientRef,
    pub declared: BridgeDeclaredGrant,
    /// Whether the declaration includes writing; without it the window offers reading only.
    pub declares_write: bool,
}

/// The payload of [`EVENT_PROMPT`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BridgePromptEvent {
    pub prompt_id: String,
    /// Unix milliseconds after which the question counts as declined.
    #[ts(type = "number")]
    pub expires_at: i64,
    pub question: BridgeQuestion,
}

/// The payload of [`EVENT_PROMPT_CANCELLED`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BridgePromptCancelled {
    pub prompt_id: String,
}

/// The window's answer.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum BridgeAnswer {
    Approval(ApprovalAnswer),
    Write(WriteAnswer),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct WriteAnswer {
    pub allow: bool,
    /// With `allow` the approval reads and writes from now on; without it, a "no" is remembered.
    pub remember: bool,
    /// Required with `allow` when the question says so.
    #[serde(default)]
    pub default_tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ApprovalAnswer {
    pub allow: bool,
    /// "Erlaubnis merken" (FR-009).
    pub remember: bool,
    /// Required with `allow`.
    #[serde(default)]
    pub access: Option<BridgeAccess>,
    /// `null`: every entry.
    #[serde(default)]
    pub scope_tags: Option<Vec<String>>,
    #[serde(default)]
    pub default_tag: Option<String>,
}

/// The parked questions of the bridge; a Tauri managed state, cheap to clone.
pub type BridgePrompts = PromptPark<BridgeQuestion, BridgeAnswer>;

pub fn new_prompts() -> BridgePrompts {
    PromptPark::new(PROMPT_TIMEOUT)
}
