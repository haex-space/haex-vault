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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
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

/// What the person chose in the dialog.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
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
