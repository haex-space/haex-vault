//! The questions of the bridge to the user (spec 049, research R10, `contracts/tauri-commands.md`):
//! approval of a client (FR-008 to FR-010), writing with a read-only approval (FR-015) and the
//! presence confirmation of a passkey ceremony (FR-030, FR-031, FR-033), parked on
//! [`crate::prompt_park`] for at most 120 s.
//! Only the window answers them; no client, extension or agent reaches the answer command (FR-013).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::declaration::BridgeAccess;
use crate::instances::passphrase::Passphrase;
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
    /// Is the user here for this one passkey ceremony (FR-030, FR-031)?
    Presence(PresenceQuestion),
}

/// The passkey ceremony a presence confirmation is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum PresenceCeremony {
    Create,
    Get,
}

/// What the user may pick: for a create the new or an existing entry, for a get the passkey.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum PresenceChoice {
    NewEntry,
    Entry {
        #[serde(rename = "itemId")]
        item_id: String,
        title: String,
        url: Option<String>,
    },
    Passkey {
        #[serde(rename = "passkeyId")]
        passkey_id: String,
        #[serde(rename = "userName")]
        user_name: Option<String>,
        #[serde(rename = "itemTitle")]
        item_title: Option<String>,
    },
}

impl PresenceChoice {
    /// Whether `other` names the same choice; the window sends the choice back as it got it.
    pub fn same_as(&self, other: &PresenceChoice) -> bool {
        match (self, other) {
            (Self::NewEntry, Self::NewEntry) => true,
            (Self::Entry { item_id: a, .. }, Self::Entry { item_id: b, .. }) => a == b,
            (Self::Passkey { passkey_id: a, .. }, Self::Passkey { passkey_id: b, .. }) => a == b,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct PresenceQuestion {
    pub client: BridgeClientRef,
    pub ceremony: PresenceCeremony,
    pub rp_id: String,
    pub rp_name: Option<String>,
    /// The page's origin, checked against the relying party before anyone is asked (FR-032).
    pub origin: String,
    pub user_name: Option<String>,
    /// The relying party requires user verification: the vault password is asked too (FR-033).
    pub require_uv: bool,
    pub choices: Vec<PresenceChoice>,
    /// SHA-256 of the challenge, hex: the confirmation holds for this one request only (FR-031).
    pub challenge_hash: String,
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

/// The window's answer. Neither `Clone` nor comparable: it may carry the vault password.
#[derive(Debug, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum BridgeAnswer {
    Approval(ApprovalAnswer),
    Write(WriteAnswer),
    Presence(PresenceAnswer),
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct PresenceAnswer {
    pub confirm: bool,
    /// Required with `confirm`: one of the question's choices.
    #[serde(default)]
    pub choice: Option<PresenceChoice>,
    /// Required exactly when the question has `requireUv`; erased on drop, never logged.
    #[serde(default)]
    #[ts(type = "string | null")]
    pub passphrase: Option<Passphrase>,
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
