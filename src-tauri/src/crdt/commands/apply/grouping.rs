use crate::crdt::trigger::is_safe_identifier;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

use super::RemoteColumnChange;

/// Groups a flat list of column changes into transaction-HLC groups and
/// returns them sorted ascending by HLC. All writes issued inside the same
/// sender-side transaction share a timestamp, so `hlc_timestamp` is the
/// semantic grouping key — there is no separate batch id anymore.
pub(crate) fn group_by_transaction_hlc(
    changes: Vec<RemoteColumnChange>,
) -> Vec<(String, Vec<RemoteColumnChange>)> {
    let mut groups: HashMap<String, Vec<RemoteColumnChange>> = HashMap::new();
    for change in changes {
        groups
            .entry(change.hlc_timestamp.clone())
            .or_default()
            .push(change);
    }

    let mut ordered: Vec<(String, Vec<RemoteColumnChange>)> = groups.into_iter().collect();
    ordered.sort_by(|a, b| crate::crdt::hlc::compare_hlc_strings(&a.0, &b.0));
    ordered
}

/// Groups column changes by `(table, row_pks)` and returns rows in ascending
/// order of their earliest HLC timestamp.
///
/// The naive shape — collect changes into a `HashMap<(table, row_pks), …>`
/// and iterate it — discards the careful HLC ordering established by
/// `group_by_transaction_hlc`: HashMap iteration is unordered. When a remote
/// batch contains rows from multiple transactions (e.g. parent inserted at
/// HLC1, child inserted at HLC2 referencing it), HashMap iteration may apply
/// the child first. FK constraints are disabled during apply so that is not
/// itself a hard error, but the apply order then no longer reflects the
/// causal order the sender intended, and any future logic that observes the
/// per-row apply sequence will see nondeterministic results.
///
/// This helper preserves the per-row grouping but sorts the resulting rows
/// by `min(hlc_timestamp)` so the iteration order is deterministic and
/// follows the same causal order as `group_by_transaction_hlc`.
pub(crate) fn group_row_changes_in_hlc_order(
    changes: impl IntoIterator<Item = RemoteColumnChange>,
) -> Vec<((String, String), Vec<RemoteColumnChange>)> {
    let mut map: HashMap<(String, String), Vec<RemoteColumnChange>> = HashMap::new();
    for change in changes {
        map.entry((change.table_name.clone(), change.row_pks.clone()))
            .or_default()
            .push(change);
    }
    let mut entries: Vec<((String, String), Vec<RemoteColumnChange>)> = map.into_iter().collect();
    entries.sort_by(|a, b| {
        let a_min = crate::crdt::hlc::hlc_min(a.1.iter().map(|c| c.hlc_timestamp.as_str()));
        let b_min = crate::crdt::hlc::hlc_min(b.1.iter().map(|c| c.hlc_timestamp.as_str()));
        let primary = match (a_min, b_min) {
            (Some(am), Some(bm)) => crate::crdt::hlc::compare_hlc_strings(am, bm),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };
        // Tie-break on the group key so equal-min-HLC rows have a stable
        // order across runs (HashMap iteration order is otherwise the only
        // signal left).
        primary.then_with(|| a.0.cmp(&b.0))
    });
    entries
}

/// Build a `WHERE …` clause that matches a row by its CRDT primary-key map.
///
/// Returns `Some((where_clause, params))` if every PK column name is a safe
/// identifier; returns `None` if **any** column name fails the safety check.
/// Skipping individual columns is wrong: with a partial WHERE the resulting
/// DELETE matches *more* than the intended row (potentially every row if
/// every column was unsafe). All-or-nothing is the only correct stance.
pub(crate) fn build_pk_where_from_map(
    row_pks: &serde_json::Map<String, JsonValue>,
) -> Option<(String, Vec<JsonValue>)> {
    if row_pks.is_empty() {
        return None;
    }
    let mut where_parts: Vec<String> = Vec::with_capacity(row_pks.len());
    let mut values: Vec<JsonValue> = Vec::with_capacity(row_pks.len());
    for (col_name, value) in row_pks {
        if !is_safe_identifier(col_name) {
            return None;
        }
        match value {
            JsonValue::Null => {
                where_parts.push(format!("\"{}\" IS NULL", col_name));
            }
            _ => {
                where_parts.push(format!("\"{}\" = ?", col_name));
                values.push(value.clone());
            }
        }
    }
    Some((where_parts.join(" AND "), values))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(table: &str, pk: &str, col: &str, hlc: &str) -> RemoteColumnChange {
        RemoteColumnChange {
            table_name: table.to_string(),
            row_pks: pk.to_string(),
            column_name: col.to_string(),
            hlc_timestamp: hlc.to_string(),
            decrypted_value: JsonValue::Null,
            sig: None,
        }
    }

    // HLC strings sort lexicographically when same length; use fixed-width
    // numeric prefixes so the relative order is unambiguous.
    const HLC1: &str = "1/abcdef";
    const HLC2: &str = "2/abcdef";
    const HLC3: &str = "3/abcdef";
    const HLC4: &str = "4/abcdef";

    #[test]
    fn helper_emits_rows_in_ascending_min_hlc_order() {
        // Construct three rows whose earliest HLCs are HLC1, HLC2, HLC3 —
        // but feed them in reverse order so HashMap insertion order is
        // visibly wrong. The helper must still produce HLC1 → HLC2 → HLC3.
        let changes = vec![
            change("t", r#"{"id":"c"}"#, "col", HLC3),
            change("t", r#"{"id":"b"}"#, "col", HLC2),
            change("t", r#"{"id":"a"}"#, "col", HLC1),
        ];

        let ordered = group_row_changes_in_hlc_order(changes);

        let keys: Vec<&str> = ordered.iter().map(|(k, _)| k.1.as_str()).collect();
        assert_eq!(
            keys,
            vec![r#"{"id":"a"}"#, r#"{"id":"b"}"#, r#"{"id":"c"}"#],
            "rows must be ordered by ascending min(hlc), regardless of input order"
        );
    }

    #[test]
    fn helper_uses_min_hlc_per_row_for_ordering() {
        // Row A has changes at HLC1 + HLC4; Row B has a single change at
        // HLC2. min(A) = HLC1 < min(B) = HLC2, so A must come before B
        // even though A also contains the latest timestamp in the batch.
        let changes = vec![
            change("t", r#"{"id":"a"}"#, "col1", HLC4),
            change("t", r#"{"id":"b"}"#, "col", HLC2),
            change("t", r#"{"id":"a"}"#, "col2", HLC1),
        ];

        let ordered = group_row_changes_in_hlc_order(changes);

        assert_eq!(ordered.len(), 2, "rows must be grouped per (table, pk)");
        assert_eq!(
            ordered[0].0 .1, r#"{"id":"a"}"#,
            "row A (min HLC = HLC1) must come before row B (min HLC = HLC2)"
        );
        assert_eq!(ordered[0].1.len(), 2, "row A must keep both of its changes");
        assert_eq!(ordered[1].0 .1, r#"{"id":"b"}"#);
    }

    // ------------------------------------------------------------------
    // build_pk_where_from_map: all-or-nothing safety
    // ------------------------------------------------------------------

    fn pk_map(pairs: &[(&str, JsonValue)]) -> serde_json::Map<String, JsonValue> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn pk_where_returns_none_for_empty_map() {
        let empty = serde_json::Map::<String, JsonValue>::new();
        assert!(build_pk_where_from_map(&empty).is_none());
    }

    #[test]
    fn pk_where_handles_safe_identifiers_with_values() {
        let map = pk_map(&[
            ("id", JsonValue::String("x".into())),
            ("group_id", JsonValue::String("g".into())),
        ]);
        let (clause, values) = build_pk_where_from_map(&map).expect("safe");
        assert!(clause.contains("\"id\" = ?"));
        assert!(clause.contains("\"group_id\" = ?"));
        assert!(clause.contains(" AND "));
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn pk_where_uses_is_null_for_null_values() {
        let map = pk_map(&[
            ("id", JsonValue::String("x".into())),
            ("optional", JsonValue::Null),
        ]);
        let (clause, values) = build_pk_where_from_map(&map).expect("safe");
        assert!(clause.contains("\"optional\" IS NULL"));
        // NULL columns do not contribute to the bound parameter list.
        assert_eq!(values.len(), 1);
    }

    #[test]
    fn pk_where_returns_none_when_any_column_is_unsafe() {
        // Bug-fix probe: previously the loop did `continue` on the unsafe
        // column, building a WHERE from the *remaining* columns. The
        // resulting DELETE would match every row that shares those
        // remaining values — potentially every row when every column is
        // unsafe. All-or-nothing is the only safe stance.
        let map = pk_map(&[
            ("id", JsonValue::String("x".into())),
            ("evil; DROP TABLE", JsonValue::String("y".into())),
        ]);
        assert!(
            build_pk_where_from_map(&map).is_none(),
            "row with any unsafe PK column must produce no WHERE clause — \
             building a partial clause from the other columns would match \
             more rows than intended"
        );
    }

    #[test]
    fn pk_where_returns_none_when_only_unsafe_columns() {
        let map = pk_map(&[("evil; --", JsonValue::String("y".into()))]);
        assert!(build_pk_where_from_map(&map).is_none());
    }

    #[test]
    fn helper_is_deterministic_across_input_orderings() {
        // A direct probe for the bug: build a batch large enough that a
        // plain HashMap iteration order is nearly guaranteed to differ
        // between insertion orderings. The helper must always produce
        // the same sequence regardless of how changes are reshuffled.
        let baseline_changes: Vec<RemoteColumnChange> = (0..16)
            .map(|i| {
                let hlc = format!("{}/abcdef", i);
                change("t", &format!(r#"{{"id":"r{}"}}"#, i), "c", &hlc)
            })
            .collect();

        let baseline = group_row_changes_in_hlc_order(baseline_changes);
        let baseline_keys: Vec<String> = baseline.iter().map(|(k, _)| k.1.clone()).collect();

        // Reverse input order and re-run.
        let reversed: Vec<RemoteColumnChange> = (0..16)
            .rev()
            .map(|i| {
                let hlc = format!("{}/abcdef", i);
                change("t", &format!(r#"{{"id":"r{}"}}"#, i), "c", &hlc)
            })
            .collect();
        let reversed_out = group_row_changes_in_hlc_order(reversed);
        let reversed_keys: Vec<String> = reversed_out.iter().map(|(k, _)| k.1.clone()).collect();

        assert_eq!(
            baseline_keys, reversed_keys,
            "iteration order must be deterministic and HLC-driven, not \
             dependent on the order changes were collected from the batch"
        );

        // Sanity: the row order matches ascending HLC numeric order.
        // (Cannot use lexicographic compare on the row keys themselves
        // because "r10" < "r2" lexically while HLC says otherwise.)
        let baseline_min_hlcs: Vec<&str> = baseline
            .iter()
            .map(|(_, list)| {
                crate::crdt::hlc::hlc_min(list.iter().map(|c| c.hlc_timestamp.as_str())).unwrap()
            })
            .collect();
        for window in baseline_min_hlcs.windows(2) {
            assert!(
                crate::crdt::hlc::compare_hlc_strings(window[0], window[1])
                    != std::cmp::Ordering::Greater,
                "consecutive rows must be in non-decreasing HLC order"
            );
        }
    }
}
