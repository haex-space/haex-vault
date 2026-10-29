//! Which rules holzi accepts (FR-002, FR-004, data-model.md): two different places, none a contact's
//! share or one of holzi's own places, no overlap with another rule of this device, a name and an
//! interval in range. A one-way rule `BToA` is stored as `AToB` with the places swapped.

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::model::{
    is_contact_share, SyncRule, SyncRuleDirection, SyncRuleDraft, SyncRulePlace, MAX_INTERVAL_SECS,
    MAX_NAME_CHARS, MIN_INTERVAL_SECS,
};
use crate::files::local::OwnPlaces;
use crate::files::SourceRef;

/// Why a rule was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRuleErrorCode {
    /// A place lies in, holds or equals a place of another rule of this device, or both places of
    /// the rule overlap.
    Overlap,
    /// A contact's share, one of holzi's own places, or not a folder path.
    InvalidPlace,
    InvalidInterval,
    InvalidName,
    /// A log depth outside 0 to 100.
    InvalidDepth,
    NotFound,
    ReadOnly,
    /// A place of the rule cannot be reached now.
    Unreachable,
    /// A place of the rule cannot be read: access, credentials, an exclusion file.
    PlaceError,
    /// An exclusion file over its limits.
    InvalidIgnore,
    /// The vault cannot be read or written now.
    Unavailable,
}

/// A rule error for the window: a code it translates, and a reason for the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRuleError {
    pub code: SyncRuleErrorCode,
    pub message: String,
}

impl SyncRuleError {
    pub fn new(code: SyncRuleErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// `draft` as it is stored, or why not. `others` are the other rules of this device (the rule
/// itself left out when it is changed).
pub fn validate(
    draft: SyncRuleDraft,
    others: &[SyncRule],
    own: &OwnPlaces,
) -> Result<SyncRuleDraft, SyncRuleError> {
    let name = draft.name.trim().to_owned();
    let chars = name.chars().count();
    if chars == 0 || chars > MAX_NAME_CHARS {
        return Err(SyncRuleError::new(
            SyncRuleErrorCode::InvalidName,
            format!("a rule name has 1 to {MAX_NAME_CHARS} characters"),
        ));
    }
    if !(MIN_INTERVAL_SECS..=MAX_INTERVAL_SECS).contains(&draft.interval_secs) {
        return Err(SyncRuleError::new(
            SyncRuleErrorCode::InvalidInterval,
            format!("the interval lies between {MIN_INTERVAL_SECS} and {MAX_INTERVAL_SECS} s"),
        ));
    }
    let place_a = checked_place(draft.place_a, own)?;
    let place_b = checked_place(draft.place_b, own)?;
    if overlap(&place_a, &place_b) {
        return Err(SyncRuleError::new(
            SyncRuleErrorCode::Overlap,
            "the two places of a rule overlap",
        ));
    }
    for other in others {
        for mine in [&place_a, &place_b] {
            if overlap(mine, &other.place_a) || overlap(mine, &other.place_b) {
                return Err(SyncRuleError::new(
                    SyncRuleErrorCode::Overlap,
                    format!("a place overlaps with the rule \"{}\"", other.name),
                ));
            }
        }
    }
    let (place_a, place_b, direction) = match draft.direction {
        SyncRuleDirection::BToA => (place_b, place_a, SyncRuleDirection::AToB),
        direction => (place_a, place_b, direction),
    };
    Ok(SyncRuleDraft {
        name,
        place_a,
        place_b,
        direction,
        ..draft
    })
}

/// The place with a normalised path: a device path absolute without a trailing `/`, a storage
/// path as a window path (`/Fotos`, or `/` for the root).
fn checked_place(place: SyncRulePlace, own: &OwnPlaces) -> Result<SyncRulePlace, SyncRuleError> {
    let invalid = |reason: &str| SyncRuleError::new(SyncRuleErrorCode::InvalidPlace, reason);
    if is_contact_share(&place.source) {
        return Err(invalid("a contact's share cannot be a place of a rule"));
    }
    if place.path.split(['/', '\\']).any(|part| part == "..") {
        return Err(invalid("a place path has no \"..\""));
    }
    let path = match &place.source {
        SourceRef::Device => {
            let path = Path::new(&place.path);
            if !path.is_absolute() {
                return Err(invalid("a device place needs an absolute path"));
            }
            let resolved = std::fs::canonicalize(path).ok();
            if own.touches(path, true)
                || resolved
                    .as_deref()
                    .is_some_and(|real| own.touches(real, true))
            {
                return Err(invalid("holzi's own places cannot be a place of a rule"));
            }
            path.components()
                .collect::<std::path::PathBuf>()
                .to_string_lossy()
                .into_owned()
        }
        SourceRef::Storage { .. } => {
            format!("/{}", place.path.trim_matches('/'))
        }
    };
    Ok(SyncRulePlace {
        source: place.source,
        path,
    })
}

/// Whether two places are the same folder or one lies in the other.
fn overlap(a: &SyncRulePlace, b: &SyncRulePlace) -> bool {
    if a.source != b.source {
        return false;
    }
    match a.source {
        SourceRef::Device => {
            let (a, b) = (Path::new(&a.path), Path::new(&b.path));
            a.starts_with(b) || b.starts_with(a)
        }
        SourceRef::Storage { .. } => {
            let folder = |path: &str| format!("{}/", path.trim_end_matches('/'));
            let (a, b) = (folder(&a.path), folder(&b.path));
            a.starts_with(&b) || b.starts_with(&a)
        }
    }
}

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
