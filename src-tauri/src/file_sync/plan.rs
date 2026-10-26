//! The plan of a run (research R3): what has to happen so both places match the rule, built from
//! both listings and the state of the last success without touching a place. Every decision of a
//! run is made here, so the table of the spec is the table of the tests.
//!
//! The steps come in the order they run: folders, moves, copies and conflict copies, then
//! deletions, so an abort never costs data before the copy is there.

use std::collections::BTreeMap;

use super::ignore::Exclusions;
use super::model::{SyncRule, SyncRuleSide};
use super::place::Listing;
use super::store::StateRow;

/// Hashes a run already knows, by side and path (research R5).
pub type Hashes = BTreeMap<(SyncRuleSide, String), String>;

/// Why the target of a one-way rule differs from the state (FR-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deviation {
    /// Changed at the target since the last success.
    Changed,
    /// New at the target, unknown to the rule.
    Added,
}

/// One step of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Creates the folder `path` on `side`.
    Folder { side: SyncRuleSide, path: String },
    /// Moves a file on `side` from `from` to `to`, as it moved on the other side (FR-009).
    Move {
        side: SyncRuleSide,
        from: String,
        to: String,
    },
    /// Copies `path` from `from` to the other side; `replaces` when a file is there.
    Copy {
        from: SyncRuleSide,
        path: String,
        replaces: bool,
    },
    /// Both sides changed `path` (FR-012): the version on `aside` moves to `aside_path`, then both
    /// versions are copied to the other side.
    Conflict {
        path: String,
        aside: SyncRuleSide,
        aside_path: String,
    },
    /// Equal on both sides; only the state is written (FR-012a).
    Adopt { path: String, hash: String },
    /// Deleted on the other side; mirrored into a trash (FR-019).
    Delete { side: SyncRuleSide, path: String },
    /// Deleted on the other side; waits for the user (FR-021).
    Pending { side: SyncRuleSide, path: String },
    /// Deleted on the other side; stays on purpose (FR-020).
    Keep { path: String },
    /// The target of a one-way rule differs; it is left as it is (FR-010).
    Report { path: String, what: Deviation },
}

/// Everything a run would do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<Step>,
    /// Files whose hash decides a step; the run reads them and builds the plan again.
    pub hash_reads: Vec<(SyncRuleSide, String)>,
    /// Files and folders the exclusions leave out (FR-042).
    pub ignored: usize,
}

/// The plan for `rule` from the listings of A and B and the state of the last success.
pub fn build(
    _rule: &SyncRule,
    a: &Listing,
    b: &Listing,
    _state: &[StateRow],
    exclusions: &Exclusions,
    _hashes: &Hashes,
) -> Plan {
    let ignored = [a, b]
        .iter()
        .flat_map(|listing| {
            listing
                .files
                .keys()
                .map(|path| (path, false))
                .chain(listing.folders.iter().map(|path| (path, true)))
        })
        .filter(|(path, is_dir)| exclusions.excludes(path, *is_dir))
        .count();
    Plan {
        ignored,
        ..Plan::default()
    }
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
