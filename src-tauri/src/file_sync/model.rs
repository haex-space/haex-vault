//! What a sync rule is (spec 061, data-model.md): its two places, direction, delete mode and
//! interval, and the result of its last run. The texts stored in the vault are the serde names.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::files::share::SHARE_PREFIX;
use crate::files::SourceRef;

/// The fewest seconds between two runs of a remote place.
pub const MIN_INTERVAL_SECS: u32 = 60;
/// The most seconds between two runs.
pub const MAX_INTERVAL_SECS: u32 = 86_400;
/// The interval of a new rule (FR-005).
pub const DEFAULT_INTERVAL_SECS: u32 = 900;
/// The longest rule name in characters.
pub const MAX_NAME_CHARS: usize = 80;

/// Which way a rule copies (FR-001): one way from the source (`place_a`) to the target (`place_b`),
/// or both ways.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRuleDirection {
    AToB,
    Both,
}

/// What a deletion on one side does to the other (FR-019 to FR-021).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRuleDeleteMode {
    Mirror,
    Keep,
    Ask,
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRuleOutcome {
    /// Every step done.
    Done,
    /// Done, but some files failed and come again next run (FR-023).
    Partial,
    /// A place was not reachable; nothing was changed (FR-022, FR-024).
    Waiting,
    /// The rule cannot run until it is changed or fixed.
    Error,
    /// The vault closed or the user stopped it (FR-026).
    Cancelled,
}

/// One of the two places of a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRuleSide {
    A,
    B,
}

impl SyncRuleSide {
    /// The other side.
    pub fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// A folder of a file browser source as the window names it (research R1): the source and the
/// window path, also through an encrypted folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRulePlace {
    pub source: SourceRef,
    pub path: String,
}

/// What the user sets on a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRuleDraft {
    pub name: String,
    /// The source; its `.haex-ignore` holds the exclusions.
    pub place_a: SyncRulePlace,
    /// The target.
    pub place_b: SyncRulePlace,
    pub direction: SyncRuleDirection,
    pub delete_mode: SyncRuleDeleteMode,
    pub interval_secs: u32,
    pub enabled: bool,
    /// The user agreed that files of an encrypted source are stored decrypted in the target
    /// (FR-030a).
    pub allow_decrypted: bool,
}

/// A stored rule with the result of its last run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRule {
    pub id: String,
    pub name: String,
    pub place_a: SyncRulePlace,
    pub place_b: SyncRulePlace,
    pub direction: SyncRuleDirection,
    pub delete_mode: SyncRuleDeleteMode,
    pub interval_secs: u32,
    pub enabled: bool,
    pub allow_decrypted: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_run_at: Option<String>,
    pub last_outcome: Option<SyncRuleOutcome>,
    pub last_error: Option<String>,
}

impl SyncRule {
    /// What the user set on it.
    pub fn draft(&self) -> SyncRuleDraft {
        SyncRuleDraft {
            name: self.name.clone(),
            place_a: self.place_a.clone(),
            place_b: self.place_b.clone(),
            direction: self.direction,
            delete_mode: self.delete_mode,
            interval_secs: self.interval_secs,
            enabled: self.enabled,
            allow_decrypted: self.allow_decrypted,
        }
    }

    /// The place of `side`.
    pub fn place(&self, side: SyncRuleSide) -> &SyncRulePlace {
        match side {
            SyncRuleSide::A => &self.place_a,
            SyncRuleSide::B => &self.place_b,
        }
    }
}

/// The text a value is stored as: its serde name.
pub fn stored<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(text)) => text,
        _ => unreachable!("the stored enums serialize to strings"),
    }
}

/// The value of a stored text; `None` for one a newer version wrote.
pub fn parse<T: serde::de::DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(text.to_owned())).ok()
}

/// The text a source is stored as: `device`, or `storage:<id>`.
pub fn source_text(source: &SourceRef) -> String {
    match source {
        SourceRef::Device => "device".to_owned(),
        SourceRef::Storage { storage_id } => format!("storage:{storage_id}"),
    }
}

/// The source a stored text names; `None` for one this version does not know.
pub fn source_of(text: &str) -> Option<SourceRef> {
    if text == "device" {
        return Some(SourceRef::Device);
    }
    let storage_id = text.strip_prefix("storage:")?;
    (!storage_id.is_empty()).then(|| SourceRef::Storage {
        storage_id: storage_id.to_owned(),
    })
}

/// Whether `source` is a contact's share, which is never a place (FR-002).
pub fn is_contact_share(source: &SourceRef) -> bool {
    matches!(source, SourceRef::Storage { storage_id } if storage_id.starts_with(SHARE_PREFIX))
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
