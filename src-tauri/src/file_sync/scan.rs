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
    pub a: Listing,
    pub b: Listing,
    pub exclusions: Exclusions,
}

/// Why a run cannot plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanError {
    /// A place is not reachable now (device off, network, folder missing); the rule waits.
    Unreachable { side: SyncRuleSide, reason: String },
    /// Something the user has to fix: credentials, access, an exclusion file.
    Failed { side: SyncRuleSide, reason: String },
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
    a: &dyn Place,
    b: &dyn Place,
    cancel: &CancellationToken,
) -> Result<Scanned, ScanError> {
    let (listed_a, listed_b) = tokio::join!(a.list_tree(cancel), b.list_tree(cancel));
    let listed_a = listed_a.map_err(|error| ScanError::of(SyncRuleSide::A, error))?;
    let listed_b = listed_b.map_err(|error| ScanError::of(SyncRuleSide::B, error))?;
    let text_a = ignore_text(a, SyncRuleSide::A).await?;
    let text_b = ignore_text(b, SyncRuleSide::B).await?;
    let exclusions =
        Exclusions::new(text_a.as_deref(), text_b.as_deref()).map_err(|error| match error {
            ignore::Error::TooLarge(too) => ScanError::Failed {
                side: too.side,
                reason: "the exclusion file is over its limits".to_owned(),
            },
            ignore::Error::Build(failed) => ScanError::Failed {
                side: failed.side,
                reason: "the exclusion file cannot be read as patterns".to_owned(),
            },
        })?;
    Ok(Scanned {
        a: listed_a,
        b: listed_b,
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
