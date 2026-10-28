//! The types of the share dialog (spec 060, data-model.md §2): what it offers for a contact, what
//! the person chose, and what comes out. Nothing here is stored or synced; the choice is never
//! kept (FR-010).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One detail of the contact as a row of the dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactShareRow {
    /// The id of the value row of the contact.
    pub id: String,
    pub kind: String,
    pub label: String,
    pub value: String,
}

/// The head fields the contact has, each its own row; the name parts always go along.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct ContactShareHead {
    pub display_name: Option<String>,
    pub nickname: Option<String>,
    pub organization: Option<String>,
    pub job_title: Option<String>,
    pub note: Option<String>,
}

/// A device of the own vault a card can name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct ContactShareDevice {
    /// The device key, hex; what a selection names.
    pub device_pubkey: String,
    /// The `vault_device_uuid`, to find the device's name among the vault's devices.
    pub vault_device_uuid: String,
    pub iroh_relay: Option<String>,
}

/// The holzi addresses of a share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ContactShareHolzi {
    /// A contact without confirmed holzi addresses.
    None,
    /// A foreign contact's confirmed addresses: one choice, the original card goes unchanged.
    Card {
        npub: String,
        devices: u32,
        relays: u32,
        issued_at: String,
    },
    /// The own vault: a card signed for this share with the devices and relays chosen. Without the
    /// vault secret (a linked device) no card can go and the lists are empty.
    Own {
        can_sign: bool,
        npub: String,
        devices: Vec<ContactShareDevice>,
        relays: Vec<String>,
    },
}

/// What the dialog shows for a contact, everything chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactShareOptions {
    pub rows: Vec<ContactShareRow>,
    pub head: ContactShareHead,
    pub holzi: ContactShareHolzi,
    /// The name as the receiver sees it (FR-011); it always goes along.
    pub name: String,
    /// Whether there is anything to share at all with everything chosen (FR-017).
    pub can_share: bool,
}

/// What the person chose about the holzi addresses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ContactShareHolziChoice {
    None,
    /// The stored original card of a foreign contact.
    Card,
    /// A fresh card of the own vault with these devices (keys, hex) and Nostr-Relays.
    Own {
        devices: Vec<String>,
        relays: Vec<String>,
    },
}

/// What the person chose in the dialog (and what an agent proposes, spec 060 FR-030).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactShareSelection {
    /// Ids of the value rows that go.
    pub values: Vec<String>,
    pub display_name: bool,
    pub nickname: bool,
    pub organization: bool,
    pub job_title: bool,
    pub note: bool,
    pub holzi: ContactShareHolziChoice,
}

/// The text of a share, which is link, clipboard text and QR code at once (FR-018).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct ContactShareOutput {
    pub link: String,
    /// The QR code as SVG; only when the text fits one (FR-019).
    pub qr_svg: Option<String>,
    /// Characters of the text.
    pub size: u32,
}

// --- Reading a share in (spec 060 US4, data-model.md §2) ---

/// Where a share goes: a new contact or one that exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ContactShareTarget {
    New,
    Contact { id: String },
}

/// Why a contact is offered as the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum ContactShareReason {
    /// It has the vault identity of the share already (spec 051 FR-013).
    Vault,
    /// It has the same phone number or e-mail address; only a hint.
    Hint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactShareCandidate {
    pub id: String,
    pub name: String,
    pub reason: ContactShareReason,
}

/// What the share says about the vault identity of the target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ContactShareIdentity {
    /// The share carries no card.
    None,
    /// The share would confirm a vault identity for the first time.
    New,
    /// The target's confirmed vault identity is the share's.
    Matches,
    /// The target has another confirmed vault identity; only a new contact can take the share.
    Mismatch { existing_npub: String },
}

/// What an item of the share is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ContactShareItemKind {
    /// One of `prefix`, `given`, `middle`, `family`, `suffix`.
    NamePart {
        field: String,
    },
    DisplayName,
    Nickname,
    Organization,
    JobTitle,
    Note,
    Value,
    HolziAddresses,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum ContactShareState {
    /// The target does not have it.
    New,
    /// The target has it already; there is nothing to choose.
    Same,
    /// A field the target has once and filled, with another text.
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum ContactShareChoice {
    Add,
    Update,
    Skip,
    Keep,
    /// Only for the addresses of an old `holzi:card:` text.
    Replace,
}

/// A value of the target an `Update` can replace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactShareExisting {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub value: String,
}

/// The confirmed holzi addresses of a share against what the target has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactShareAddresses {
    pub npub: String,
    pub devices: u32,
    pub relays: u32,
    pub new_devices: u32,
    pub new_relays: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct ContactShareItem {
    /// The place of the item in the preview; a decision names it.
    pub id: u32,
    pub item: ContactShareItemKind,
    /// The kind of a value item.
    pub value_kind: Option<String>,
    pub label: String,
    /// What the share says.
    pub value: String,
    pub state: ContactShareState,
    /// What the person may choose; empty when there is nothing to choose.
    pub choices: Vec<ContactShareChoice>,
    pub default: Option<ContactShareChoice>,
    /// The target's values of the same kind, for `Update`.
    pub existing: Vec<ContactShareExisting>,
    /// What a single field of the target says now, for a conflict.
    pub current: Option<String>,
    /// Only for the holzi addresses.
    pub addresses: Option<ContactShareAddresses>,
}

/// What a share shows before anything is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct ContactSharePreview {
    pub name: String,
    pub npub: Option<String>,
    /// The vault identity, hex; what blocking the vault names.
    pub vault_pubkey: Option<String>,
    /// The name is the `npub` of the vault: the sender gave no name.
    pub name_is_npub: bool,
    pub target: ContactShareTarget,
    /// The name of the target, if it is a contact.
    pub target_name: Option<String>,
    /// Opaque; `contacts_share_accept` gives it back for a target that exists.
    pub target_revision: Option<String>,
    pub candidates: Vec<ContactShareCandidate>,
    pub identity: ContactShareIdentity,
    pub items: Vec<ContactShareItem>,
    /// There are holzi addresses to reach the contact by (FR-017a).
    pub reachable: bool,
    /// Details left out because they did not validate.
    pub skipped: u32,
    pub blocked: bool,
    /// The share is an old `holzi:card:` text.
    pub legacy_card: bool,
}

/// What the person chose for one item.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct ContactShareDecision {
    pub item: u32,
    pub choice: ContactShareChoice,
    /// For `Update` of a value: the value of the target to replace.
    pub value_id: Option<String>,
}
