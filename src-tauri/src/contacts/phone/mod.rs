//! The sync of the address book with the phone's address book (spec 051 US2, research R5 to R9).
//!
//! Kotlin only reads and writes rows of the Contacts Provider; every rule lies here, behind the
//! [`PhoneBook`] trait, so the desktop tests run it against a fake. On/off and the target account
//! are device settings in `preferences`; the state of the run (`review`, `active`, `paused`) is a
//! row of `haex_contacts_phone_runs`, which exists only while the sync is on. A run reads the
//! phone's index, compares, writes the phone and then holzi in one transaction; it never holds the
//! vault while it talks to the phone.

pub mod android;
mod apply;
mod gather;
mod guard;
mod head;
mod index;
mod jobs;
mod labels;
mod mapping;
mod matching;
mod merge;
mod reconcile;
mod run;
mod sides;
pub mod state;
mod store;
#[cfg(test)]
pub(crate) mod test_rig;
#[cfg(test)]
pub(crate) mod test_support;
pub mod trigger;
mod write;

use serde::{Deserialize, Serialize};
use tauri_plugin_holzi_android::contacts::{ApplyResult, Op, PhoneAccount, RawData, RawIndex};
use ts_rs::TS;

use crate::error::Result;

pub use run::{review, run, Mode};
pub use store::{Issue, IssueReason};

/// The phone's address book as the run sees it: the commands of contracts/android-plugin.md.
/// Every call blocks; the run calls them on a blocking thread.
pub trait PhoneBook: Send + Sync {
    /// Whether holzi may read and write the address book now (without asking).
    fn permitted(&self) -> Result<bool>;
    fn accounts(&self) -> Result<Vec<PhoneAccount>>;
    /// The raw contacts with an id above `after_id`, at most `limit`, by id.
    fn index(&self, after_id: i64, limit: u32) -> Result<Vec<RawIndex>>;
    /// The data rows of at most [`MAX_READ`] raw contacts.
    fn read(&self, ids: &[i64]) -> Result<Vec<RawData>>;
    /// Applies at most [`write::MAX_OPS`] ops as one batch.
    fn apply(&self, ops: &[Op]) -> Result<ApplyResult>;
    /// The photo of a raw contact as JPEG (or another image format the phone stored).
    fn photo(&self, id: i64) -> Result<Option<Vec<u8>>>;
}

/// Most raw contacts per read.
pub const MAX_READ: usize = 200;

/// An account of the phone, as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneAccount"
)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// Empty for the local account.
    pub account_type: String,
    pub name: String,
    #[serde(default)]
    pub writable: bool,
    #[serde(default)]
    pub is_default: bool,
}

impl From<PhoneAccount> for Account {
    fn from(account: PhoneAccount) -> Self {
        Account {
            account_type: account.account_type,
            name: account.name,
            writable: account.writable,
            is_default: account.is_default,
        }
    }
}

/// The numbers of a run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneCounts"
)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    /// Contacts new in the address book.
    pub imported: u32,
    /// Contacts of the phone merged into existing ones.
    pub merged: u32,
    /// Contacts the phone changed in the address book.
    pub updated: u32,
    /// Contacts written to the phone.
    pub written: u32,
    /// Contacts deleted, on either side.
    pub deleted: u32,
}

/// Why a run stopped and waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhonePaused"
)]
#[serde(rename_all = "snake_case")]
pub enum Paused {
    Permission,
    MassDelete,
}

impl Paused {
    pub fn as_str(self) -> &'static str {
        match self {
            Paused::Permission => "permission",
            Paused::MassDelete => "mass_delete",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "permission" => Some(Paused::Permission),
            "mass_delete" => Some(Paused::MassDelete),
            _ => None,
        }
    }
}

/// The deletions a run held back (FR-016).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneMassDelete"
)]
#[serde(rename_all = "camelCase")]
pub struct MassDelete {
    /// Contacts deleted on the phone that would go from the address book.
    pub from_phone: u32,
    /// Contacts deleted in holzi that would go from the phone.
    pub from_holzi: u32,
    /// Contacts linked on this phone.
    pub linked: u32,
}

/// What a run did, or why it stopped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneRunSummary"
)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub counts: Counts,
    pub paused: Option<Paused>,
    pub mass_delete: Option<MassDelete>,
}

/// A contact as an uncertain pair shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhonePairSide"
)]
#[serde(rename_all = "camelCase")]
pub struct PairSide {
    pub name: String,
    pub phones: Vec<String>,
    pub emails: Vec<String>,
}

/// A contact of the phone and a contact of the address book that may be the same person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhonePair"
)]
#[serde(rename_all = "camelCase")]
pub struct Pair {
    pub id: String,
    pub phone: PairSide,
    pub contact_id: String,
    pub contact: PairSide,
}

/// The overview before the first run (FR-019).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneReview"
)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPlan {
    /// Contacts of the phone that become new contacts (uncertain pairs left apart included).
    pub to_address_book: u32,
    /// Contacts with this phone as a target that are not on it yet.
    pub to_phone: u32,
    /// Contacts of the phone merged into a contact of the address book.
    pub merged: u32,
    pub uncertain: Vec<Pair>,
}

/// The person's decision on an uncertain pair; a pair without one stays apart.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhonePairDecision"
)]
#[serde(rename_all = "camelCase")]
pub struct PairDecision {
    pub pair_id: String,
    pub merge: bool,
}

/// The person's decision on held-back deletions (FR-016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneMassDecision"
)]
#[serde(rename_all = "camelCase")]
pub enum MassDecision {
    /// Delete on the other side as well.
    Delete,
    /// Keep: back into the address book, or written to the phone again.
    Keep,
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
