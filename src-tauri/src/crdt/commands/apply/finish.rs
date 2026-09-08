//! `before_commit` completion work: pending-column recovery markers, the
//! per-space delete-log fan-out (still entirely vault's job — the crate is
//! deliberately space-agnostic, D-3), and the sync-backend push cursor.

use std::collections::HashSet;

use haex_crdt::{ApplyOutcome, RemoteChanges, SkipReason};
use rusqlite::{params, Transaction};
use serde_json::Value as JsonValue;

use crate::crdt::shared_space_trigger::SHARED_SPACE_DELETED_ROWS_TABLE;
use crate::database::error::DatabaseError;
use crate::table_names::TABLE_CRDT_PENDING_COLUMNS;

use super::delete_propagation::propagate_shared_space_deleted_rows_to_target_tables;

/// Delete-log rows for the per-space log arrive as ordinary column changes
/// into [`SHARED_SPACE_DELETED_ROWS_TABLE`]; collect their `id`s after
/// admission and writing.
///
/// Mirrors the crate's own `collect_inbound_delete_log_ids` fix for the
/// owner-domain log exactly, at vault's per-space layer: a replay that lost
/// LWW (`Stale`/`SupersededInBatch`) still propagates the already-admitted
/// tombstone, but a change this batch's policy or the core actually
/// *rejected* must never cause a target DELETE — propagating a rejected
/// "delete" claim would let a peer's claim that failed vault's own
/// registry/signature gate still delete a row it was never authorized to
/// touch.
pub(super) fn collect_inbound_shared_space_delete_log_ids(
    changes: &RemoteChanges,
    outcome: &ApplyOutcome,
) -> HashSet<String> {
    let rejected: HashSet<usize> = outcome
        .skipped
        .iter()
        .filter(|skipped| {
            !matches!(
                skipped.reason,
                SkipReason::Stale | SkipReason::SupersededInBatch
            )
        })
        .map(|skipped| skipped.input_index)
        .collect();

    let mut ids: HashSet<String> = HashSet::new();
    for (input_index, change) in changes.iter().enumerate() {
        if change.table_name != SHARED_SPACE_DELETED_ROWS_TABLE || rejected.contains(&input_index)
        {
            continue;
        }
        if let Ok(map) = serde_json::from_str::<serde_json::Map<String, JsonValue>>(&change.row_pks)
        {
            if let Some(JsonValue::String(id)) = map.get("id") {
                ids.insert(id.clone());
            }
        }
    }
    ids
}

/// Run vault's per-space delete-log propagation
/// (`propagate_shared_space_deleted_rows_to_target_tables`, register-gated,
/// D-3) for every non-rejected inbound shared-space delete-log id.
pub(super) fn propagate_shared_space_deletes(
    tx: &Transaction,
    changes: &RemoteChanges,
    outcome: &ApplyOutcome,
) -> Result<(), DatabaseError> {
    let ids = collect_inbound_shared_space_delete_log_ids(changes, outcome);
    if ids.is_empty() {
        return Ok(());
    }
    propagate_shared_space_deleted_rows_to_target_tables(tx, &ids)
}

/// Core-driven pending-column markers: a column genuinely unknown to the
/// local schema (`SkipReason::UnknownColumn`) — a rolling upgrade where a
/// newer peer's schema has extra columns this device hasn't migrated in
/// yet. Registry-driven ones (missing fresh `row_sig`) are handled directly
/// in `prepare_row`, at the point of decision, since that hook already has
/// `&Transaction` right there — no need to defer those to `before_commit`.
pub(super) fn write_pending_column_markers(
    tx: &Transaction,
    changes: &RemoteChanges,
    outcome: &ApplyOutcome,
) -> Result<(), DatabaseError> {
    for skipped in &outcome.skipped {
        if !matches!(skipped.reason, SkipReason::UnknownColumn) {
            continue;
        }
        let change = &changes[skipped.input_index];
        tx.execute(
            &format!(
                "INSERT OR IGNORE INTO {TABLE_CRDT_PENDING_COLUMNS} (table_name, column_name, row_pks) VALUES (?, ?, ?)"
            ),
            params![change.table_name, change.column_name, change.row_pks],
        )
        .map_err(DatabaseError::from)?;
    }
    Ok(())
}

/// Update the server-sync push-cursor watermark for `backend_info` — the
/// last HLC this backend has been pushed. Unrelated to the local logical
/// clock, which the crate advances separately, only past what was actually
/// written this batch.
pub(super) fn update_backend_cursor(
    tx: &Transaction,
    backend_info: Option<(&str, &str)>,
) -> Result<(), DatabaseError> {
    let Some((backend_id, max_hlc)) = backend_info else {
        return Ok(());
    };
    eprintln!("[SYNC RUST] Updating last_push_hlc_timestamp_no_sync to {max_hlc}");
    tx.execute(
        "UPDATE haex_sync_backends SET last_push_hlc_timestamp_no_sync = ? WHERE id = ?",
        params![max_hlc, backend_id],
    )
    .map_err(DatabaseError::from)?;
    Ok(())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use haex_crdt::{ColumnChange, SkippedChange};
    use serde_json::json;

    fn change(table: &str, row_pks: &str) -> ColumnChange {
        ColumnChange {
            table_name: table.to_string(),
            row_pks: row_pks.to_string(),
            column_name: "table_name".to_string(),
            hlc_timestamp: "1/aaa".to_string(),
            value: json!("items"),
            device_id: String::new(),
            sig: None,
        }
    }

    #[test]
    fn policy_rejected_shared_delete_is_excluded() {
        let changes = vec![change(
            SHARED_SPACE_DELETED_ROWS_TABLE,
            r#"{"id":"del-1"}"#,
        )];
        let mut outcome = ApplyOutcome::default();
        outcome.skipped.push(SkippedChange {
            input_index: 0,
            reason: SkipReason::Policy,
        });
        let ids = collect_inbound_shared_space_delete_log_ids(&changes, &outcome);
        assert!(
            ids.is_empty(),
            "a policy-rejected shared delete claim must not propagate"
        );
    }

    #[test]
    fn stale_replay_of_an_admitted_shared_delete_still_propagates() {
        let changes = vec![change(
            SHARED_SPACE_DELETED_ROWS_TABLE,
            r#"{"id":"del-1"}"#,
        )];
        let mut outcome = ApplyOutcome::default();
        outcome.skipped.push(SkippedChange {
            input_index: 0,
            reason: SkipReason::Stale,
        });
        let ids = collect_inbound_shared_space_delete_log_ids(&changes, &outcome);
        assert!(
            ids.contains("del-1"),
            "a stale replay of an already-admitted delete must still propagate"
        );
    }

    #[test]
    fn superseded_in_batch_replay_still_propagates() {
        let changes = vec![change(
            SHARED_SPACE_DELETED_ROWS_TABLE,
            r#"{"id":"del-1"}"#,
        )];
        let mut outcome = ApplyOutcome::default();
        outcome.skipped.push(SkippedChange {
            input_index: 0,
            reason: SkipReason::SupersededInBatch,
        });
        let ids = collect_inbound_shared_space_delete_log_ids(&changes, &outcome);
        assert!(ids.contains("del-1"));
    }

    #[test]
    fn admitted_change_not_present_in_skipped_still_propagates() {
        let changes = vec![change(
            SHARED_SPACE_DELETED_ROWS_TABLE,
            r#"{"id":"del-1"}"#,
        )];
        let outcome = ApplyOutcome::default(); // nothing skipped: this change was applied
        let ids = collect_inbound_shared_space_delete_log_ids(&changes, &outcome);
        assert!(ids.contains("del-1"));
    }

    #[test]
    fn owner_domain_delete_log_changes_are_not_collected_here() {
        let changes = vec![change("haex_deleted_rows", r#"{"id":"del-1"}"#)];
        let outcome = ApplyOutcome::default();
        let ids = collect_inbound_shared_space_delete_log_ids(&changes, &outcome);
        assert!(
            ids.is_empty(),
            "the owner-domain delete-log is the crate's job now, not this collector's"
        );
    }

    fn setup_pending_columns_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE_CRDT_PENDING_COLUMNS} (
                 table_name TEXT NOT NULL,
                 column_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 PRIMARY KEY(table_name, column_name, row_pks)
             );"
        ))
        .unwrap();
        conn
    }

    #[test]
    fn write_pending_column_markers_records_unknown_column_skips() {
        let mut conn = setup_pending_columns_db();
        let changes = vec![change("devices", r#"{"id":"dev-1"}"#)];
        let mut outcome = ApplyOutcome::default();
        outcome.skipped.push(SkippedChange {
            input_index: 0,
            reason: SkipReason::UnknownColumn,
        });
        let tx = conn.transaction().unwrap();
        write_pending_column_markers(&tx, &changes, &outcome).unwrap();
        tx.commit().unwrap();

        let count: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM {TABLE_CRDT_PENDING_COLUMNS} \
                     WHERE table_name = 'devices' AND column_name = 'table_name' \
                     AND row_pks = '{{\"id\":\"dev-1\"}}'"
                ),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn write_pending_column_markers_ignores_other_skip_reasons() {
        let mut conn = setup_pending_columns_db();
        let changes = vec![change("devices", r#"{"id":"dev-1"}"#)];
        let mut outcome = ApplyOutcome::default();
        outcome.skipped.push(SkippedChange {
            input_index: 0,
            reason: SkipReason::Stale,
        });
        let tx = conn.transaction().unwrap();
        write_pending_column_markers(&tx, &changes, &outcome).unwrap();
        tx.commit().unwrap();

        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {TABLE_CRDT_PENDING_COLUMNS}"), [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "only UnknownColumn skips get a pending marker");
    }
}
