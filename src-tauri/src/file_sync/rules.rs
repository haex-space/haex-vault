//! Which rules holzi accepts (FR-002, FR-004, data-model.md): two different places, none a contact's
//! share or one of holzi's own places, no overlap with another rule of this device, a name and an
//! interval in range.

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::model::{
    is_contact_share, SyncRule, SyncRuleDeleteMode, SyncRuleDraft, SyncRulePlace,
    SyncRuleSourceKind, MAX_INTERVAL_SECS, MAX_NAME_CHARS, MIN_INTERVAL_SECS,
};
use super::place_file::split_file_path;
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
    /// A contact's share, one of holzi's own places, or not a folder path; for a file rule a source that
    /// is not a regular file.
    InvalidPlace,
    InvalidInterval,
    InvalidName,
    /// A log depth outside 0 to 100.
    InvalidDepth,
    NotFound,
    ReadOnly,
    /// A place of the rule cannot be reached now.
    Unreachable,
    /// A file rule finds no file at its source path (spec 074).
    InvalidSource,
    /// The action does not apply to a file rule: its exclusions (spec 074 FR-010).
    FileRule,
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
    let kind = draft.kind();
    let source = checked_place(draft.source, own)?;
    let target = checked_place(draft.target, own)?;
    if kind == SyncRuleSourceKind::File {
        check_file_source(&source)?;
    }
    let mine = claims(&source, &target, kind);
    if mine.len() == 2 && overlap(&mine[0], &mine[1]) {
        return Err(SyncRuleError::new(
            SyncRuleErrorCode::Overlap,
            "the two places of a rule overlap",
        ));
    }
    for other in others {
        let theirs = claims(&other.source, &other.target, other.source_kind);
        if mine
            .iter()
            .any(|claim| theirs.iter().any(|their| overlap(claim, their)))
        {
            return Err(SyncRuleError::new(
                SyncRuleErrorCode::Overlap,
                format!("a place overlaps with the rule \"{}\"", other.name),
            ));
        }
    }
    // A file rule deletes nothing, so it has no delete mode (spec 074 FR-004).
    let delete_mode = if kind == SyncRuleSourceKind::File {
        SyncRuleDeleteMode::Keep
    } else {
        draft.delete_mode
    };
    Ok(SyncRuleDraft {
        name,
        source,
        target,
        source_kind: Some(kind),
        delete_mode,
        ..draft
    })
}

/// What a rule claims, for overlap (spec 074 research R6): a folder rule its two folders; a file rule the
/// source file and the copy in the target folder (the folder joined with the file's name), so several
/// file rules can fill one target folder.
fn claims(
    source: &SyncRulePlace,
    target: &SyncRulePlace,
    kind: SyncRuleSourceKind,
) -> Vec<SyncRulePlace> {
    if kind == SyncRuleSourceKind::Folder {
        return vec![source.clone(), target.clone()];
    }
    let Some((_, name)) = split_file_path(&source.location, &source.path) else {
        return vec![source.clone()];
    };
    let copy = match target.location {
        SourceRef::Device => Path::new(&target.path)
            .join(&name)
            .to_string_lossy()
            .into_owned(),
        SourceRef::Storage { .. } => format!("{}/{name}", target.path.trim_end_matches('/')),
    };
    vec![
        source.clone(),
        SyncRulePlace {
            location: target.location.clone(),
            path: copy,
        },
    ]
}

/// The source of a file rule names a file, and on this device is not a folder or a link (a link is not
/// followed, spec 061 FR-018). A path that is not there is let through: the file may lie in an encrypted
/// folder, which the file system does not show under that name, and a file that is gone is the invalid
/// source a run reports.
fn check_file_source(source: &SyncRulePlace) -> Result<(), SyncRuleError> {
    let invalid = |reason: &str| SyncRuleError::new(SyncRuleErrorCode::InvalidPlace, reason);
    if split_file_path(&source.location, &source.path).is_none() {
        return Err(invalid("the source of a file rule needs a file name"));
    }
    if source.location == SourceRef::Device {
        if let Ok(meta) = std::fs::symlink_metadata(&source.path) {
            if !meta.is_file() {
                return Err(invalid(
                    "the source of a file rule is a regular file, not a folder or a link",
                ));
            }
        }
    }
    Ok(())
}

/// The place with a normalised path: a device path absolute without a trailing `/`, a storage
/// path as a window path (`/Fotos`, or `/` for the root).
fn checked_place(place: SyncRulePlace, own: &OwnPlaces) -> Result<SyncRulePlace, SyncRuleError> {
    let invalid = |reason: &str| SyncRuleError::new(SyncRuleErrorCode::InvalidPlace, reason);
    if is_contact_share(&place.location) {
        return Err(invalid("a contact's share cannot be a place of a rule"));
    }
    if place.path.split(['/', '\\']).any(|part| part == "..") {
        return Err(invalid("a place path has no \"..\""));
    }
    let path = match &place.location {
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
        location: place.location,
        path,
    })
}

/// Whether two places are the same folder or one lies in the other.
fn overlap(a: &SyncRulePlace, b: &SyncRulePlace) -> bool {
    if a.location != b.location {
        return false;
    }
    match a.location {
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
