// src-tauri/src/crdt/compaction_anchor.rs
//
// Vault-owned compaction-anchor advances that partner with the crate-side
// `haex_crdt::cleanup_deleted_rows`. Per design-decision D-3 the shared-space
// fan-out (per-space `haex_space_compaction_anchors` writes and the
// vault-only `haex_shared_space_deleted_rows` prune) stays in the consumer:
// the crate has no knowledge of vault-specific tables and its cleanup entry
// point only touches the owner-domain `haex_deleted_rows` delete-log.
//
// The two `advance_*` helpers below are ported verbatim from the retired
// `src-tauri/src/crdt/cleanup.rs` and preserve the original doc-comments —
// the notes about `.optional()` semantics, the partial-unique-index /
// `ON CONFLICT` interaction, and the CRDT trigger interaction all still
// apply because the underlying tables and triggers did not change.
//
// `prune_shared_space_delete_log_and_advance_anchors` is the new vault-side
// helper the crate's `before_prune` closure calls. It mirrors the crate's
// discipline exactly:
//   - only rows with a non-NULL HLC are prunable (NULL-HLC entries can't be
//     safely anchored, so leaving them until they're anchorable is correct);
//   - per-space max HLC advances that space's anchor BEFORE its delete-log
//     rows are deleted, so a crash between advance and delete leaves the
//     anchor at most as strict as the surviving entries, never looser
//     (ADR 0002 §6.5).

use rusqlite::{Connection, OptionalExtension, Transaction};

use haex_crdt::db::error::DatabaseError;

use crate::crdt::trigger::SHARED_SPACE_DELETED_ROWS_TABLE;

pub const OWNER_DELETE_LOG_ANCHOR_KEY: &str = "owner_delete_log_anchor";

// -----------------------------------------------------------------------
// Anchor advancement helpers (ADR 0002 §6.5).
//
// Retention prunes old entries from the two delete-logs; we synchronously
// advance a per-space anchor (haex_space_compaction_anchors) and a single
// owner-domain anchor (haex_vault_settings key='owner_delete_log_anchor').
// The push handler rejects incoming batches with hlc < anchor so a stale
// peer cannot resurrect a row whose delete-signal has been pruned.
// -----------------------------------------------------------------------

/// Advance the per-space compaction anchor for `space_id` to the max of its
/// existing value and `new_hlc`. Idempotent, monotonic (never regresses).
///
/// Writes to `haex_space_compaction_anchors` which is CRDT-synced; the
/// AFTER-INSERT trigger populates haex_column_hlcs_no_sync so other members converge
/// on the advance via normal sync.
pub fn advance_shared_space_anchor(
    conn: &Connection,
    space_id: &str,
    new_hlc: &str,
) -> Result<(), rusqlite::Error> {
    // Read the current value (if any) and compare via the numeric HLC
    // comparator — INSERT OR REPLACE with plain string MAX() would ignore
    // node-id disambiguation and could accept a lexicographically smaller
    // string that is actually newer.
    // Use `.optional()` (not `.ok()`) so only `QueryReturnedNoRows` collapses to
    // `None`. Any other rusqlite error (locked/busy DB, corruption, missing
    // column) propagates instead of being silently treated as "first write",
    // which would cause the UPSERT below to overwrite an existing higher anchor
    // with `new_hlc` — a regression of the anti-resurrection watermark.
    let current: Option<String> = conn
        .query_row(
            "SELECT min_valid_hlc FROM haex_space_compaction_anchors WHERE space_id = ?1",
            [space_id],
            |row| row.get(0),
        )
        .optional()?;
    let effective = match current.as_deref() {
        Some(cur) => {
            if haex_crdt::hlc_is_newer(new_hlc, cur) {
                new_hlc.to_string()
            } else {
                return Ok(()); // no regression — nothing to write
            }
        }
        None => new_hlc.to_string(),
    };

    // `min_valid_hlc` (the anti-resurrection watermark) and `haex_hlc_no_sync`
    // (this CRDT row's own timestamp) are semantically distinct: the
    // former is the max HLC pruned from the delete-log for this space;
    // the latter is a fresh local HLC stamping *this* update to the
    // anchor row. Stamp haex_hlc_no_sync via the `current_hlc()` UDF so the row
    // gets a real local HLC, and the CRDT AFTER-INSERT trigger populates
    // haex_column_hlcs_no_sync / haex_column_sigs_no_sync on top.
    conn.execute(
        "INSERT INTO haex_space_compaction_anchors (space_id, min_valid_hlc, haex_hlc_no_sync) \
         VALUES (?1, ?2, current_hlc()) \
         ON CONFLICT(space_id) DO UPDATE SET \
             min_valid_hlc = excluded.min_valid_hlc, \
             haex_hlc_no_sync = current_hlc()",
        rusqlite::params![space_id, &effective],
    )?;
    Ok(())
}

/// Advance the owner-domain delete-log anchor stored in
/// `haex_vault_settings` (key=`owner_delete_log_anchor`, device_id NULL).
///
/// Same monotonic max-wins semantic as the per-space variant, but stored in
/// vault_settings rather than a dedicated table — there's exactly one owner
/// anchor per vault so a full table would be overkill.
///
/// **Concurrency contract.** SQLite treats NULL as distinct in unique
/// indexes, so the composite `(key, device_id)` index does NOT enforce
/// single-row-ness for `device_id IS NULL`. Migration 0013 adds the partial
/// unique index `idx_haex_vault_settings_owner_key ON (key) WHERE device_id
/// IS NULL` explicitly so the `ON CONFLICT` clause below can target a
/// deterministic slot and two concurrent cleanup passes cannot each insert
/// a duplicate owner-anchor row.
pub fn advance_owner_delete_log_anchor(
    conn: &Connection,
    new_hlc: &str,
) -> Result<(), rusqlite::Error> {
    // See `advance_shared_space_anchor` for the rationale behind `.optional()`
    // instead of `.ok()`: real read errors must propagate rather than be
    // conflated with "no anchor yet" and silently regress the watermark.
    let current: Option<String> = conn
        .query_row(
            "SELECT value FROM haex_vault_settings \
             WHERE key = ?1 AND device_id IS NULL",
            [OWNER_DELETE_LOG_ANCHOR_KEY],
            |row| row.get(0),
        )
        .optional()?;
    let effective = match current.as_deref() {
        Some(cur) => {
            if haex_crdt::hlc_is_newer(new_hlc, cur) {
                new_hlc.to_string()
            } else {
                return Ok(());
            }
        }
        None => new_hlc.to_string(),
    };

    // Single atomic UPSERT targeting the partial unique index added by
    // migration 0013 (`idx_haex_vault_settings_owner_key ON (key) WHERE
    // device_id IS NULL`). Without the partial index, two concurrent
    // cleanup calls could both see `updated == 0` on the pre-check above
    // and both `INSERT`, leaving duplicate owner-anchor rows that break
    // the monotonic-watermark contract.
    conn.execute(
        "INSERT INTO haex_vault_settings (id, key, value, device_id) \
         VALUES (lower(hex(randomblob(16))), ?1, ?2, NULL) \
         ON CONFLICT (key) WHERE device_id IS NULL \
         DO UPDATE SET value = excluded.value",
        rusqlite::params![OWNER_DELETE_LOG_ANCHOR_KEY, &effective],
    )?;
    Ok(())
}

// -----------------------------------------------------------------------
// Shared-space delete-log prune + anchor fan-out.
// -----------------------------------------------------------------------

/// Prune old rows from `haex_shared_space_deleted_rows` and advance each
/// affected space's compaction anchor to the max HLC of its pruned entries.
///
/// Meant to run from inside the `before_prune` closure of
/// [`haex_crdt::cleanup_deleted_rows`], sharing that call's transaction so
/// everything commits atomically. Symmetric to what the crate does for
/// `haex_deleted_rows`:
///
/// * only rows with non-NULL `haex_hlc_no_sync` are prunable — NULL-HLC
///   entries stay because they can't be safely anchored (they're anomalies);
/// * per-space anchor advances happen BEFORE that space's rows are deleted,
///   so a crash between advance and delete leaves the anchor at most as
///   strict as the surviving entries, never looser (ADR 0002 §6.5);
/// * time-based policy reuses the same cutoff formula as the crate
///   (`compute_cutoff` in `haex_crdt::crdt::cleanup`): the current HLC is
///   read from `haex_crdt_configs_no_sync` (`key='hlc_timestamp'`,
///   `type='hlc'`) and reduced by `days`; if no HLC has been recorded yet
///   or the cutoff would overflow `i64`, this pass is a no-op.
pub fn prune_shared_space_delete_log_and_advance_anchors(
    tx: &Transaction,
    policy: haex_crdt::RetentionPolicy,
) -> Result<usize, DatabaseError> {
    match policy {
        haex_crdt::RetentionPolicy::All => {
            let mut per_space_stmt = tx.prepare(&format!(
                "SELECT space_id, haex_hlc_no_sync FROM \"{SHARED_SPACE_DELETED_ROWS_TABLE}\" \
                 WHERE haex_hlc_no_sync IS NOT NULL"
            ))?;
            let per_space_maxes = collect_per_space_max(per_space_stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?)?;
            drop(per_space_stmt);
            for (space_id, hlc) in &per_space_maxes {
                advance_shared_space_anchor(&*tx, space_id, hlc)?;
            }

            let rows_deleted = tx.execute(
                &format!(
                    "DELETE FROM \"{SHARED_SPACE_DELETED_ROWS_TABLE}\" \
                     WHERE haex_hlc_no_sync IS NOT NULL"
                ),
                [],
            )?;
            Ok(rows_deleted)
        }
        haex_crdt::RetentionPolicy::TimeBasedDays { .. } => {
            // Reuse the crate's cutoff so shared-space DELETE stays in
            // lockstep with the crate's owner-domain DELETE — one formula,
            // one source of truth. See haex_crdt::compute_cutoff.
            let Some(cutoff) = haex_crdt::compute_cutoff(&*tx, policy)? else {
                return Ok(0);
            };

            let mut per_space_stmt = tx.prepare(&format!(
                "SELECT space_id, haex_hlc_no_sync FROM \"{SHARED_SPACE_DELETED_ROWS_TABLE}\" \
                 WHERE haex_hlc_no_sync IS NOT NULL \
                   AND CAST(substr(haex_hlc_no_sync, 1, instr(haex_hlc_no_sync, '/') - 1) AS INTEGER) < ?1"
            ))?;
            let per_space_maxes =
                collect_per_space_max(per_space_stmt.query_map([cutoff], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?)?;
            drop(per_space_stmt);
            for (space_id, hlc) in &per_space_maxes {
                advance_shared_space_anchor(&*tx, space_id, hlc)?;
            }

            let rows_deleted = tx.execute(
                &format!(
                    "DELETE FROM \"{SHARED_SPACE_DELETED_ROWS_TABLE}\" \
                     WHERE haex_hlc_no_sync IS NOT NULL \
                       AND CAST(substr(haex_hlc_no_sync, 1, instr(haex_hlc_no_sync, '/') - 1) AS INTEGER) < ?1"
                ),
                [cutoff],
            )?;
            Ok(rows_deleted)
        }
    }
}

/// Collapse `(space_id, hlc)` rows into `space_id -> max_hlc`, comparing
/// HLC values numerically via `haex_crdt::hlc_is_newer` so node-id
/// disambiguation is honored.
fn collect_per_space_max<I>(
    rows: I,
) -> Result<std::collections::HashMap<String, String>, DatabaseError>
where
    I: Iterator<Item = Result<(String, String), rusqlite::Error>>,
{
    let mut per_space_maxes: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for r in rows {
        let (space_id, hlc) = r?;
        per_space_maxes
            .entry(space_id)
            .and_modify(|cur| {
                if haex_crdt::hlc_is_newer(&hlc, cur) {
                    *cur = hlc.clone();
                }
            })
            .or_insert(hlc);
    }
    Ok(per_space_maxes)
}

// Cutoff computation lives in the crate (`haex_crdt::compute_cutoff`); this
// module used to duplicate it, which risked silent drift if the crate ever
// changed its formula. The single source of truth is now the crate.

#[cfg(test)]
#[path = "compaction_anchor_tests.rs"]
mod anchor_tests;
