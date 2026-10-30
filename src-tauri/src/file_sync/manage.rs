//! What the window shows of a rule without running it: the preview of its plan (FR-007), which
//! changes nothing, and the exclusion file of the source with the fixed exclusions and the count of
//! what they leave out (FR-041, FR-042).

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use super::ignore::{self, Exclusions, FIXED, IGNORE_FILE, MAX_BYTES};
use super::model::{SyncRule, SyncRuleSide};
use super::place::Place;
use super::plan::{excluded_count, Deviation, Hashes, Step};
use super::run::plan_for;
use super::scan::{scan, ScanError};
use super::status::SyncRuleIgnoreLine;
use super::store::StateRow;
use crate::files::{FilesError, FilesErrorCode};

/// What a step of a plan does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRulePlanAction {
    Folder,
    Copy,
    Move,
    ConflictCopy,
    Adopt,
    Delete,
    /// Waits for the user (delete mode "ask").
    Pending,
    /// Stays on purpose (delete mode "keep").
    Keep,
    /// The target of a one-way rule differs and is left as it is.
    Deviation,
}

/// A step of the preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRulePlanStep {
    pub action: SyncRulePlanAction,
    pub path: String,
    /// Where it happens; none for a file kept on purpose.
    pub side: Option<SyncRuleSide>,
    /// The target of a move, the name of a conflict copy, what changed at a one-way target.
    pub detail: Option<String>,
}

/// The preview of a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRulePlan {
    pub steps: Vec<SyncRulePlanStep>,
    /// Files the run compares by reading their content (FR-012a); their steps are not in the list.
    pub hash_reads: u32,
    /// Files and folders the exclusions leave out (FR-042).
    pub ignored: u32,
}

/// The exclusion file of a rule as the editor shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRuleIgnore {
    /// The file in the source; empty when there is none.
    pub content: String,
    pub writable: bool,
    /// Why the source cannot be written.
    pub reason: Option<String>,
    pub fixed: Vec<String>,
    /// Files and folders of both places the exclusions leave out now.
    pub ignored_now: u32,
    /// Lines of the file in the source that are left out.
    pub line_errors: Vec<SyncRuleIgnoreLine>,
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// What a run of `rule` would do now, from the state `state`; nothing is changed. Hashes are not
/// read: their files are counted instead.
pub async fn preview(
    rule: &SyncRule,
    a: &dyn Place,
    b: &dyn Place,
    state: &[StateRow],
    device_name: &str,
    today: &str,
) -> Result<SyncRulePlan, ScanError> {
    let scanned = scan(a, b, &CancellationToken::new()).await?;
    let plan = plan_for(device_name, today, rule, &scanned, state, &Hashes::new());
    Ok(SyncRulePlan {
        steps: plan.steps.iter().flat_map(view).collect(),
        hash_reads: count(plan.hash_reads.len()),
        ignored: count(plan.ignored),
    })
}

fn step(
    action: SyncRulePlanAction,
    side: Option<SyncRuleSide>,
    path: &str,
    detail: Option<&str>,
) -> SyncRulePlanStep {
    SyncRulePlanStep {
        action,
        path: path.to_owned(),
        side,
        detail: detail.map(str::to_owned),
    }
}

fn view(planned: &Step) -> Vec<SyncRulePlanStep> {
    use SyncRulePlanAction as Action;
    match planned {
        Step::Folder { side, path } => vec![step(Action::Folder, Some(*side), path, None)],
        Step::Move { side, from, to } => vec![step(Action::Move, Some(*side), from, Some(to))],
        Step::Aside { side, path, to } => {
            vec![step(Action::ConflictCopy, Some(*side), path, Some(to))]
        }
        Step::Copy { from, path, .. } => vec![step(Action::Copy, Some(from.other()), path, None)],
        Step::Conflict { path, keep, aside } => {
            vec![step(
                Action::ConflictCopy,
                Some(keep.other()),
                path,
                Some(aside),
            )]
        }
        Step::MergeIgnore { aside_a, aside_b } => vec![
            step(
                Action::ConflictCopy,
                Some(SyncRuleSide::A),
                IGNORE_FILE,
                Some(aside_a),
            ),
            step(
                Action::ConflictCopy,
                Some(SyncRuleSide::B),
                IGNORE_FILE,
                Some(aside_b),
            ),
        ],
        Step::Adopt { path, .. } => vec![step(Action::Adopt, None, path, None)],
        Step::Delete { side, path } => vec![step(Action::Delete, Some(*side), path, None)],
        Step::Pending { side, path } => vec![step(Action::Pending, Some(*side), path, None)],
        Step::Keep { path } => vec![step(Action::Keep, None, path, None)],
        Step::Report { path, what } => {
            let what = match what {
                Deviation::Changed => "changed",
                Deviation::Deleted => "deleted",
                Deviation::Added => "added",
            };
            vec![step(
                Action::Deviation,
                Some(SyncRuleSide::B),
                path,
                Some(what),
            )]
        }
        Step::Forget { .. } => Vec::new(),
    }
}

/// The exclusion file of the source, the fixed exclusions, and what both leave out of `a` and `b` now.
/// `read_only` is why the source cannot be written, if it cannot.
pub async fn ignore_get(
    a: &dyn Place,
    b: &dyn Place,
    read_only: Option<String>,
) -> Result<SyncRuleIgnore, ScanError> {
    let scanned = scan(a, b, &CancellationToken::new()).await?;
    let content = a
        .read_text(IGNORE_FILE, MAX_BYTES)
        .await
        .map_err(|error| ScanError::Failed {
            side: SyncRuleSide::A,
            reason: error.message,
        })?
        .unwrap_or_default();
    let line_errors = Exclusions::new(Some(&content), None)
        .map(|own| own.errors)
        .unwrap_or_default()
        .into_iter()
        .map(|line| SyncRuleIgnoreLine {
            line: count(line.line),
            text: line.text,
        })
        .collect();
    Ok(SyncRuleIgnore {
        content,
        writable: read_only.is_none(),
        reason: read_only,
        fixed: FIXED.iter().map(|line| (*line).to_owned()).collect(),
        ignored_now: count(excluded_count(&scanned.exclusions, &scanned.a, &scanned.b)),
        line_errors,
    })
}

/// Writes the exclusion file of the source (FR-041); one over the limits is refused.
pub async fn ignore_set(a: &dyn Place, content: &str) -> Result<(), FilesError> {
    if !ignore::fits(content) {
        return Err(FilesError::new(
            FilesErrorCode::TooLarge,
            "the exclusion file is over its limits",
        ));
    }
    a.write_text(IGNORE_FILE, content).await
}

#[cfg(test)]
#[path = "manage_tests.rs"]
mod tests;
