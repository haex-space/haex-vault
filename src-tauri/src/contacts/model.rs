//! Row and answer types of the address book (spec 051, data-model.md, contracts/tauri-commands.md),
//! exported to the frontend with ts-rs (`src/types/bindings/`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::contacts::phone::{Account, IssueReason};
use crate::patch::Patch;

/// Where a contact came from (FR-018a). Set when the contact is created, never changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactOriginKind"
)]
#[serde(rename_all = "camelCase")]
pub enum OriginKind {
    /// Created in holzi, by the user or an agent.
    Holzi,
    /// Taken over from a phone's address book.
    Phone,
    /// Created from a business card.
    Card,
}

impl OriginKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OriginKind::Holzi => "holzi",
            OriginKind::Phone => "phone",
            OriginKind::Card => "card",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "holzi" => Some(OriginKind::Holzi),
            "phone" => Some(OriginKind::Phone),
            "card" => Some(OriginKind::Card),
            _ => None,
        }
    }
}

/// The origin of a contact: what, on which device, by whom, and from which phone account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactOrigin"
)]
#[serde(rename_all = "camelCase")]
pub struct Origin {
    pub kind: OriginKind,
    /// The `vault_device_uuid` of the device that created the contact.
    pub device: String,
    /// `user`, `builtin_agent`, `external_agent:<id>`, `extension:<id>` or `internal:<name>`.
    pub caller: String,
    /// For a contact from a phone: `account_type/account_name` (empty for the local account).
    pub account: Option<String>,
}

/// One value of a contact (FR-003, FR-007): a phone number, an address, a custom value, … The set
/// of kinds is open; `known` is false for a kind this version does not know, which is read-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactValue"
)]
#[serde(rename_all = "camelCase")]
pub struct Value {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub value: String,
    #[ts(type = "number")]
    pub position: i64,
    pub known: bool,
}

/// A group of contacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactGroup"
)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
}

/// The name, organization and note of a contact: the columns of `haex_contacts` a user edits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    optional_fields,
    rename = "ContactHead"
)]
#[serde(rename_all = "camelCase")]
pub struct Head {
    pub display_name: Option<String>,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub middle_name: Option<String>,
    pub name_prefix: Option<String>,
    pub name_suffix: Option<String>,
    pub nickname: Option<String>,
    pub organization: Option<String>,
    pub job_title: Option<String>,
    pub note: Option<String>,
}

/// A contact with everything the window shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub id: String,
    /// The display name as stored, or derived from the name parts when none is stored.
    pub name: String,
    pub head: Head,
    /// Hash of the photo in `haex_contacts_binaries`.
    pub photo: Option<String>,
    pub origin: Origin,
    pub values: Vec<Value>,
    pub groups: Vec<Group>,
    /// The phones the contact goes to (FR-018b).
    pub targets: Vec<ContactTarget>,
    /// Changes that did not reach a phone (FR-025).
    pub issues: Vec<ContactIssue>,
    /// The confirmed holzi addresses, from the contact's Visitenkarte (FR-008).
    pub card: Option<crate::contacts::card::received::ContactCardInfo>,
    pub created_at: String,
    pub updated_at: String,
}

/// A phone a contact goes to and the account it goes into there (FR-018b).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactTarget {
    /// The phone's `vault_device_uuid`.
    pub device: String,
    /// Empty for the local account.
    pub account_type: String,
    pub account_name: String,
}

/// A change of a contact that did not reach a phone, and why (FR-025).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactIssue {
    pub device: String,
    pub reason: IssueReason,
    pub at: String,
}

/// A phone a new contact goes to (FR-018c); without an account it goes into the phone's target
/// account.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct TargetInput {
    pub device: String,
    pub account: Option<Account>,
}

/// What the list shows of a contact and searches in (research R11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactHeader {
    pub id: String,
    pub name: String,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub organization: Option<String>,
    pub phones: Vec<String>,
    pub emails: Vec<String>,
    /// Group ids.
    pub groups: Vec<String>,
    pub photo: Option<String>,
    pub origin_kind: OriginKind,
    /// The phones the contact goes to.
    pub targets: Vec<String>,
    /// Why a change did not reach a phone, if one did not.
    pub issue: Option<IssueReason>,
    /// The contact has confirmed holzi addresses (a Visitenkarte).
    pub has_card: bool,
}

/// A value for a new contact or a new value of an existing one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactValueInput"
)]
#[serde(rename_all = "camelCase")]
pub struct ValueInput {
    pub kind: String,
    pub label: String,
    pub value: String,
}

/// A new contact.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct ContactInput {
    #[serde(default)]
    pub head: Head,
    #[serde(default)]
    pub values: Vec<ValueInput>,
    /// Group ids.
    #[serde(default)]
    pub groups: Vec<String>,
    /// The phones it goes to; none keeps it only in holzi (FR-018c).
    #[serde(default)]
    pub targets: Vec<TargetInput>,
}

/// A value to change (`id` set) or add (`id` absent) in a partial update.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    optional_fields,
    rename = "ContactValueUpsert"
)]
#[serde(rename_all = "camelCase")]
pub struct ValueUpsert {
    pub id: Option<String>,
    pub kind: String,
    pub label: String,
    pub value: String,
    #[ts(type = "number", optional)]
    pub position: Option<i64>,
}

/// The values of a partial update: only the rows named here are written (research R3).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    optional_fields,
    rename = "ContactValuesPatch"
)]
#[serde(rename_all = "camelCase")]
pub struct ValuesPatch {
    #[serde(default)]
    pub upsert: Vec<ValueUpsert>,
    #[serde(default)]
    pub remove: Vec<String>,
}

/// A partial update of a contact: an absent field keeps its value, `null` clears it.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct ContactPatch {
    #[serde(default)]
    #[ts(type = "string | null", optional)]
    pub display_name: Patch<String>,
    #[serde(default)]
    #[ts(type = "string | null", optional)]
    pub given_name: Patch<String>,
    #[serde(default)]
    #[ts(type = "string | null", optional)]
    pub family_name: Patch<String>,
    #[serde(default)]
    #[ts(type = "string | null", optional)]
    pub middle_name: Patch<String>,
    #[serde(default)]
    #[ts(type = "string | null", optional)]
    pub name_prefix: Patch<String>,
    #[serde(default)]
    #[ts(type = "string | null", optional)]
    pub name_suffix: Patch<String>,
    #[serde(default)]
    #[ts(type = "string | null", optional)]
    pub nickname: Patch<String>,
    #[serde(default)]
    #[ts(type = "string | null", optional)]
    pub organization: Patch<String>,
    #[serde(default)]
    #[ts(type = "string | null", optional)]
    pub job_title: Patch<String>,
    #[serde(default)]
    #[ts(type = "string | null", optional)]
    pub note: Patch<String>,
    /// The value rows to write; absent writes none.
    pub values: Option<ValuesPatch>,
    /// Group ids; replaces the memberships when present.
    pub groups: Option<Vec<String>>,
}

impl ContactPatch {
    /// The head columns of the patch with their column names, in table order.
    pub(crate) fn head_columns(&self) -> [(&'static str, &Patch<String>); 10] {
        [
            ("display_name", &self.display_name),
            ("given_name", &self.given_name),
            ("family_name", &self.family_name),
            ("middle_name", &self.middle_name),
            ("name_prefix", &self.name_prefix),
            ("name_suffix", &self.name_suffix),
            ("nickname", &self.nickname),
            ("organization", &self.organization),
            ("job_title", &self.job_title),
            ("note", &self.note),
        ]
    }
}
