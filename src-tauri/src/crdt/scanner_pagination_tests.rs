// Tests for `paginate_changes`, the pure HLC-group pagination helper.
//
// HLC == one source transaction. A page packs WHOLE transaction-HLC groups
// (ascending) until a byte budget would be exceeded, never splitting a group
// across a page boundary. The ≥1 rule guarantees forward progress: if the very
// first group alone exceeds the budget, it is emitted alone (with has_more) so
// the wire still carries it — capped uphill by MAX_CRDT_TRANSACTION_BYTES.
//
// Budgets are tiny and injected; no test allocates anywhere near 100 MB.

use super::{paginate_changes, LocalColumnChange};

/// Build a `LocalColumnChange` at a given HLC. `value` length is the lever the
/// tests pull to control the serialized group size against a tiny budget.
fn change_at(hlc: &str, column: &str, value: &str) -> LocalColumnChange {
    LocalColumnChange {
        table_name: "haex_passwords".to_string(),
        row_pks: "{\"id\":\"row-1\"}".to_string(),
        column_name: column.to_string(),
        hlc_timestamp: hlc.to_string(),
        value: serde_json::Value::String(value.to_string()),
        device_id: "dev".to_string(),
        sig: None,
    }
}

/// HLC strings are `<time>/<node_hex>`; numeric comparison on the time part, so
/// A < B < C are genuinely ascending.
const HLC_A: &str = "100/aa";
const HLC_B: &str = "200/aa";
const HLC_C: &str = "300/aa";

fn hlcs(changes: &[LocalColumnChange]) -> Vec<String> {
    changes.iter().map(|c| c.hlc_timestamp.clone()).collect()
}

/// Serialized size of one group (single-element here) — used to derive exact
/// budget boundaries rather than guessing byte counts.
fn group_size(changes: &[LocalColumnChange]) -> usize {
    serde_json::to_vec(&changes).map(|v| v.len()).unwrap()
}

#[test]
fn empty_input_returns_empty_no_more() {
    let (page, has_more) = paginate_changes(vec![], 1024);
    assert!(page.is_empty(), "no changes to page");
    assert!(!has_more, "empty input never reports more");
}

#[test]
fn all_groups_fit_returns_all_no_more() {
    let changes = vec![
        change_at(HLC_A, "a", "x"),
        change_at(HLC_B, "b", "y"),
        change_at(HLC_C, "c", "z"),
    ];
    let (page, has_more) = paginate_changes(changes, 10 * 1024);
    assert_eq!(
        hlcs(&page),
        vec![HLC_A, HLC_B, HLC_C],
        "a generous budget fits everything"
    );
    assert!(!has_more, "everything fit, so no more pages");
}

#[test]
fn budget_cuts_after_first_group() {
    let g_a = vec![change_at(HLC_A, "a", "x")];
    let g_b = vec![change_at(HLC_B, "b", "y")];
    // Budget admits the first group but not a second of the same size.
    let budget = group_size(&g_a) + group_size(&g_b) - 1;

    let changes = vec![change_at(HLC_A, "a", "x"), change_at(HLC_B, "b", "y")];
    let (page, has_more) = paginate_changes(changes, budget);
    assert_eq!(
        hlcs(&page),
        vec![HLC_A],
        "only the first group fits under the budget"
    );
    assert!(
        has_more,
        "the deferred second group means more pages follow"
    );
}

#[test]
fn single_oversized_group_is_emitted_alone_with_more() {
    // First group's size alone exceeds the budget AND a later group follows.
    // The ≥1 rule emits the oversized first group anyway (so it can ever
    // traverse the wire), then STOPS — deferring the later group → has_more.
    let g_a = vec![change_at(HLC_A, "a", "big-value")];
    let budget = group_size(&g_a) - 1;

    let changes = vec![
        change_at(HLC_A, "a", "big-value"),
        change_at(HLC_B, "b", "y"),
    ];
    let (page, has_more) = paginate_changes(changes, budget);
    assert_eq!(
        hlcs(&page),
        vec![HLC_A],
        "an oversized first group is included anyway (≥1 rule), alone"
    );
    assert!(
        has_more,
        "the later group is deferred, so more pages follow"
    );
}

#[test]
fn single_oversized_group_alone_no_more_when_nothing_follows() {
    // Variant of the ≥1 rule: the oversized first group is the ONLY group.
    // It is emitted, and since nothing follows, has_more is false.
    let g_a = vec![
        change_at(HLC_A, "a", "big-value"),
        change_at(HLC_A, "b", "v2"),
    ];
    let budget = group_size(&g_a) - 1;

    let changes = vec![
        change_at(HLC_A, "a", "big-value"),
        change_at(HLC_A, "b", "v2"),
    ];
    let (page, has_more) = paginate_changes(changes, budget);
    assert_eq!(
        hlcs(&page),
        vec![HLC_A, HLC_A],
        "the whole (over-budget) group goes out — never split"
    );
    assert!(
        !has_more,
        "no later groups exist, so this is the final page"
    );
}

#[test]
fn a_group_is_never_split_across_pages() {
    // Group B has two changes; the budget admits group A and then cannot fit
    // BOTH of B's changes. B must defer ENTIRELY (not partially) to the next
    // page — a transaction is atomic.
    let g_a = vec![change_at(HLC_A, "a", "x")];
    let g_b = vec![change_at(HLC_B, "b1", "y"), change_at(HLC_B, "b2", "z")];
    // Budget fits A plus only ONE of B's two changes — but B is atomic.
    let budget = group_size(&g_a) + (group_size(&g_b) / 2);

    let changes = vec![
        change_at(HLC_A, "a", "x"),
        change_at(HLC_B, "b1", "y"),
        change_at(HLC_B, "b2", "z"),
    ];
    let (page, has_more) = paginate_changes(changes, budget);
    assert_eq!(
        hlcs(&page),
        vec![HLC_A],
        "group B is held back whole, not partially packed"
    );
    assert!(has_more, "group B is deferred to a later page");
}

#[test]
fn page_preserves_hlc_order() {
    // Input arrives out of order; the page must come back ascending by HLC.
    let changes = vec![
        change_at(HLC_C, "c", "z"),
        change_at(HLC_A, "a", "x"),
        change_at(HLC_B, "b", "y"),
    ];
    let (page, has_more) = paginate_changes(changes, 10 * 1024);
    assert_eq!(
        hlcs(&page),
        vec![HLC_A, HLC_B, HLC_C],
        "the flattened page is in ascending HLC order regardless of input order"
    );
    assert!(!has_more);
}
