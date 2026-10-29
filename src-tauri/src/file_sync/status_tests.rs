//! The state of a rule from switch, control loop and last result, and what the last run found.

use super::*;
use crate::file_sync::execute::Counts;
use crate::file_sync::model::{SyncRuleDeleteMode, SyncRuleDirection, SyncRulePlace};
use crate::files::SourceRef;

fn rule(enabled: bool, outcome: Option<SyncRuleOutcome>) -> SyncRule {
    let place = |path: &str| SyncRulePlace {
        source: SourceRef::Device,
        path: path.to_owned(),
    };
    SyncRule {
        id: "r".to_owned(),
        name: "r".to_owned(),
        place_a: place("/a"),
        place_b: place("/b"),
        direction: SyncRuleDirection::AToB,
        delete_mode: SyncRuleDeleteMode::Keep,
        interval_secs: 900,
        enabled,
        allow_decrypted: false,
        created_at: String::new(),
        updated_at: String::new(),
        last_run_at: None,
        last_outcome: outcome,
        last_error: Some("target: gone".to_owned()),
    }
}

#[test]
fn the_state_follows_switch_run_and_result() {
    let state =
        |enabled, outcome, running| status_of(&rule(enabled, outcome), running, None, 0).state;
    assert_eq!(state(true, None, false), SyncRuleState::Active);
    assert_eq!(
        state(true, Some(SyncRuleOutcome::Done), false),
        SyncRuleState::Active
    );
    assert_eq!(
        state(true, Some(SyncRuleOutcome::Partial), false),
        SyncRuleState::Active
    );
    assert_eq!(
        state(true, Some(SyncRuleOutcome::Waiting), false),
        SyncRuleState::Waiting
    );
    assert_eq!(
        state(true, Some(SyncRuleOutcome::Error), false),
        SyncRuleState::Error
    );
    assert_eq!(
        state(false, Some(SyncRuleOutcome::Error), false),
        SyncRuleState::Paused
    );
    assert_eq!(
        state(true, Some(SyncRuleOutcome::Error), true),
        SyncRuleState::Running
    );
}

#[test]
fn the_last_run_fills_errors_and_counters() {
    let report = RunReport {
        outcome: SyncRuleOutcome::Partial,
        error: None,
        counts: Counts {
            conflicts: 2,
            deviations: 1,
            ..Counts::default()
        },
        failures: vec![("x.txt".to_owned(), "locked".to_owned())],
        skipped: Vec::new(),
        ignore_errors: vec![crate::file_sync::ignore::LineError {
            side: crate::file_sync::model::SyncRuleSide::A,
            line: 3,
            text: "[z-a".to_owned(),
        }],
        ignored: 7,
        trashed: Vec::new(),
        conflict_copies: vec!["x (Konflikt Laptop 2026-10-29).txt".to_owned()],
        items: Vec::new(),
    };
    let status = status_of(
        &rule(true, Some(SyncRuleOutcome::Partial)),
        false,
        Some(&report),
        4,
    );
    assert_eq!(status.pending_deletes, 4);
    assert_eq!(status.conflicts, 2);
    assert_eq!(
        status.conflict_copies,
        ["x (Konflikt Laptop 2026-10-29).txt"]
    );
    assert_eq!(status.deviations, 1);
    assert_eq!(status.ignored, 7);
    assert_eq!(status.errors[0].path, "x.txt");
    assert_eq!(status.ignore_errors[0].line, 3);
    assert_eq!(status.reason.as_deref(), Some("target: gone"));
}
