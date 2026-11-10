//! What a run sees of both places before it plans (research R3): both listings, read side by side,
//! and the exclusions from both `.haex-ignore` files. A place that cannot be read stops the run
//! before anything is planned (FR-022); an exclusion file over the limits or one that cannot be
//! read does too, since leaving it out would sync what the user excluded (FR-038).

use tokio_util::sync::CancellationToken;

use super::ignore::{self, Exclusions, IGNORE_FILE, MAX_BYTES};
use super::model::SyncRuleSide;
use super::place::{Listing, Place};
use crate::files::{FilesError, FilesErrorCode};

/// Both places as a run sees them.
pub struct Scanned {
    pub source: Listing,
    pub target: Listing,
    pub exclusions: Exclusions,
}

/// Why a run cannot plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanError {
    /// A place is not reachable now (device off, network, folder missing); the rule waits.
    Unreachable { side: SyncRuleSide, reason: String },
    /// Something the user has to fix: credentials, access, an exclusion file.
    Failed { side: SyncRuleSide, reason: String },
    /// A file rule finds no file at its source path (spec 074); `reason` is the key the window
    /// translates (`noFile`, `folderGone`). Nothing is copied or deleted.
    InvalidSource { reason: String },
}

impl ScanError {
    fn of(side: SyncRuleSide, error: FilesError) -> Self {
        match error.code {
            FilesErrorCode::Unreachable => Self::Unreachable {
                side,
                reason: error.message,
            },
            _ => Self::Failed {
                side,
                reason: error.message,
            },
        }
    }
}

/// Lists both places and reads their exclusion files.
pub async fn scan(
    source: &dyn Place,
    target: &dyn Place,
    cancel: &CancellationToken,
) -> Result<Scanned, ScanError> {
    let (listed_source, listed_target) =
        tokio::join!(source.list_tree(cancel), target.list_tree(cancel));
    let listed_source =
        listed_source.map_err(|error| ScanError::of(SyncRuleSide::Source, error))?;
    let listed_target =
        listed_target.map_err(|error| ScanError::of(SyncRuleSide::Target, error))?;
    let text_source = ignore_text(source, SyncRuleSide::Source).await?;
    let text_target = ignore_text(target, SyncRuleSide::Target).await?;
    let exclusions = Exclusions::new(text_source.as_deref(), text_target.as_deref()).map_err(
        |error| match error {
            ignore::Error::TooLarge(too) => ScanError::Failed {
                side: too.side,
                reason: "the exclusion file is over its limits".to_owned(),
            },
            ignore::Error::Build(failed) => ScanError::Failed {
                side: failed.side,
                reason: "the exclusion file cannot be read as patterns".to_owned(),
            },
        },
    )?;
    Ok(Scanned {
        source: listed_source,
        target: listed_target,
        exclusions,
    })
}

async fn ignore_text(place: &dyn Place, side: SyncRuleSide) -> Result<Option<String>, ScanError> {
    match place.read_text(IGNORE_FILE, MAX_BYTES).await {
        Ok(text) => Ok(text),
        Err(error) if error.code == FilesErrorCode::TooLarge => Err(ScanError::Failed {
            side,
            reason: "the exclusion file is over its limits".to_owned(),
        }),
        Err(error) => Err(ScanError::of(side, error)),
    }
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
