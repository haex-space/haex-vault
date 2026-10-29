//! The status of a rule for the window (research R9, FR-025): its state from the switch, the
//! control loop and the last result, and what the last run of this session found. The window shows
//! "holzi nicht aktiv" (`inactive`) itself when the service does not run.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::model::{SyncRule, SyncRuleOutcome};
use super::run::RunReport;

/// Where a rule stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRuleState {
    Active,
    Running,
    /// A place is not reachable; the rule tries again (FR-024).
    Waiting,
    Error,
    Paused,
    /// holzi or the vault was not active; only the window sets it.
    Inactive,
}

/// A file a run could not handle, with the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRuleFileError {
    pub path: String,
    pub reason: String,
}

/// A line of an exclusion file that is left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRuleIgnoreLine {
    pub line: u32,
    pub text: String,
}

/// The status of a rule (contracts/tauri-commands.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRuleStatus {
    pub state: SyncRuleState,
    pub reason: Option<String>,
    pub pending_deletes: u32,
    pub conflicts: u32,
    /// The conflict copies of the last run, to find them (Story 3, scenario 4).
    pub conflict_copies: Vec<String>,
    pub errors: Vec<SyncRuleFileError>,
    pub ignored: u32,
    pub ignore_errors: Vec<SyncRuleIgnoreLine>,
    pub deviations: u32,
}

/// A rule with its status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRuleView {
    pub rule: SyncRule,
    pub status: SyncRuleStatus,
}

/// The status of `rule`: `running` while a run is on, `last` the report of this session's last run,
/// `pending` the deletions waiting for the user.
pub fn status_of(
    rule: &SyncRule,
    running: bool,
    last: Option<&RunReport>,
    pending: u32,
) -> SyncRuleStatus {
    let state = if running {
        SyncRuleState::Running
    } else if !rule.enabled {
        SyncRuleState::Paused
    } else {
        match rule.last_outcome {
            Some(SyncRuleOutcome::Waiting) => SyncRuleState::Waiting,
            Some(SyncRuleOutcome::Error) => SyncRuleState::Error,
            _ => SyncRuleState::Active,
        }
    };
    let count = |n: u64| u32::try_from(n).unwrap_or(u32::MAX);
    SyncRuleStatus {
        state,
        reason: rule.last_error.clone(),
        pending_deletes: pending,
        conflicts: last.map_or(0, |report| count(report.counts.conflicts)),
        conflict_copies: last
            .map(|report| report.conflict_copies.clone())
            .unwrap_or_default(),
        errors: last
            .map(|report| {
                report
                    .failures
                    .iter()
                    .map(|(path, reason)| SyncRuleFileError {
                        path: path.clone(),
                        reason: reason.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        ignored: last.map_or(0, |report| {
            u32::try_from(report.ignored).unwrap_or(u32::MAX)
        }),
        ignore_errors: last
            .map(|report| {
                report
                    .ignore_errors
                    .iter()
                    .map(|line| SyncRuleIgnoreLine {
                        line: u32::try_from(line.line).unwrap_or(u32::MAX),
                        text: line.text.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        deviations: last.map_or(0, |report| count(report.counts.deviations)),
    }
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
