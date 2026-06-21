// Tests for `split_complete_groups`, the pure helper that decides which pulled
// changes are safe to apply now versus held back until a later page confirms
// the trailing (possibly-incomplete) transaction.
//
// HLC == one source transaction. When the transport reports `has_more = true`,
// the highest-HLC group in the current page may be only partially delivered, so
// it is held back. With `has_more = false` the page is complete and everything
// applies.

use super::split_complete_groups;
use crate::crdt::commands::RemoteColumnChange;

/// Build a `RemoteColumnChange` with a given HLC. The other fields are
/// irrelevant to `split_complete_groups` (it partitions purely on
/// `hlc_timestamp`), so they get deterministic placeholders.
fn change_at(hlc: &str, column: &str) -> RemoteColumnChange {
    RemoteColumnChange {
        table_name: "haex_passwords".to_string(),
        row_pks: "{\"id\":\"row-1\"}".to_string(),
        column_name: column.to_string(),
        hlc_timestamp: hlc.to_string(),
        decrypted_value: serde_json::Value::Null,
    }
}

/// HLC strings are `<time>/<node_hex>`; comparison is numeric on the time
/// component, so A < B < C below are genuinely ascending.
const HLC_A: &str = "100/aa";
const HLC_B: &str = "200/aa";
const HLC_C: &str = "300/aa";

fn hlcs(changes: &[RemoteColumnChange]) -> Vec<String> {
    changes.iter().map(|c| c.hlc_timestamp.clone()).collect()
}

#[test]
fn empty_input_returns_empty() {
    let (to_apply, hold_back) = split_complete_groups(vec![], false);
    assert!(to_apply.is_empty(), "no changes to apply");
    assert!(hold_back.is_empty(), "nothing to hold back");

    // has_more = true on empty input is still empty/empty.
    let (to_apply, hold_back) = split_complete_groups(vec![], true);
    assert!(to_apply.is_empty());
    assert!(hold_back.is_empty());
}

#[test]
fn has_more_false_applies_everything() {
    let changes = vec![
        change_at(HLC_A, "a"),
        change_at(HLC_B, "b"),
        change_at(HLC_C, "c"),
    ];
    let (to_apply, hold_back) = split_complete_groups(changes, false);
    assert_eq!(
        hlcs(&to_apply),
        vec![HLC_A, HLC_B, HLC_C],
        "with has_more=false the whole page applies"
    );
    assert!(
        hold_back.is_empty(),
        "nothing is held back when the page is complete"
    );
}

#[test]
fn has_more_true_holds_back_trailing_hlc() {
    let changes = vec![
        change_at(HLC_A, "a"),
        change_at(HLC_B, "b"),
        change_at(HLC_C, "c"),
    ];
    let (to_apply, hold_back) = split_complete_groups(changes, true);
    assert_eq!(
        hlcs(&to_apply),
        vec![HLC_A, HLC_B],
        "changes strictly below max HLC apply"
    );
    assert_eq!(
        hlcs(&hold_back),
        vec![HLC_C],
        "the max-HLC (trailing) transaction is held back"
    );
}

#[test]
fn has_more_true_single_hlc_holds_everything() {
    let changes = vec![change_at(HLC_A, "a"), change_at(HLC_A, "b")];
    let (to_apply, hold_back) = split_complete_groups(changes, true);
    assert!(
        to_apply.is_empty(),
        "a single HLC equals the max, so nothing is strictly below it"
    );
    assert_eq!(
        hlcs(&hold_back),
        vec![HLC_A, HLC_A],
        "the entire single transaction is held back until confirmed"
    );
}

#[test]
fn multiple_changes_same_hlc_grouped_together() {
    // Two changes at the trailing HLC (C) plus an earlier one (A). With
    // has_more=true both C changes must be held back together — a transaction
    // is never split across the page boundary.
    let changes = vec![
        change_at(HLC_A, "a"),
        change_at(HLC_C, "c1"),
        change_at(HLC_C, "c2"),
    ];
    let (to_apply, hold_back) = split_complete_groups(changes, true);
    assert_eq!(
        hlcs(&to_apply),
        vec![HLC_A],
        "only the earlier, complete transaction applies"
    );
    assert_eq!(
        hlcs(&hold_back),
        vec![HLC_C, HLC_C],
        "both changes sharing the trailing HLC are held back together"
    );
}
