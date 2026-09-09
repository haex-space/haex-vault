use crate::crdt::shared_space_trigger::is_safe_identifier;
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
    ordered.sort_by(|a, b| haex_crdt::compare_hlc_strings(&a.0, &b.0));
    ordered
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
}
