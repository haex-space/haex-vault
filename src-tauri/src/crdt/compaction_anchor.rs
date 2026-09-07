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

use haex_crdt::table_names::TABLE_CRDT_CONFIGS;
use rusqlite::{Connection, OptionalExtension, Transaction};
use std::str::FromStr;
use uhlc::Timestamp;

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
/// AFTER-INSERT trigger populates haex_column_hlcs_no_trigger so other members converge
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

    // `min_valid_hlc` (the anti-resurrection watermark) and `haex_hlc_no_trigger`
    // (this CRDT row's own timestamp) are semantically distinct: the
    // former is the max HLC pruned from the delete-log for this space;
    // the latter is a fresh local HLC stamping *this* update to the
    // anchor row. Stamp haex_hlc_no_trigger via the `current_hlc()` UDF so the row
    // gets a real local HLC, and the CRDT AFTER-INSERT trigger populates
    // haex_column_hlcs_no_trigger / haex_column_sigs_no_trigger on top.
    conn.execute(
        "INSERT INTO haex_space_compaction_anchors (space_id, min_valid_hlc, haex_hlc_no_trigger) \
         VALUES (?1, ?2, current_hlc()) \
         ON CONFLICT(space_id) DO UPDATE SET \
             min_valid_hlc = excluded.min_valid_hlc, \
             haex_hlc_no_trigger = current_hlc()",
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
/// * only rows with non-NULL `haex_hlc_no_trigger` are prunable — NULL-HLC
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
) -> Result<(), DatabaseError> {
    match policy {
        haex_crdt::RetentionPolicy::All => {
            let mut per_space_stmt = tx.prepare(&format!(
                "SELECT space_id, haex_hlc_no_trigger FROM \"{SHARED_SPACE_DELETED_ROWS_TABLE}\" \
                 WHERE haex_hlc_no_trigger IS NOT NULL"
            ))?;
            let per_space_maxes = collect_per_space_max(per_space_stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?)?;
            drop(per_space_stmt);
            for (space_id, hlc) in &per_space_maxes {
                advance_shared_space_anchor(&*tx, space_id, hlc)?;
            }

            tx.execute(
                &format!(
                    "DELETE FROM \"{SHARED_SPACE_DELETED_ROWS_TABLE}\" \
                     WHERE haex_hlc_no_trigger IS NOT NULL"
                ),
                [],
            )?;
        }
        haex_crdt::RetentionPolicy::TimeBasedDays { days } => {
            let Some(cutoff) = compute_cutoff_hlc_num_from_config(&*tx, days)? else {
                return Ok(());
            };

            let mut per_space_stmt = tx.prepare(&format!(
                "SELECT space_id, haex_hlc_no_trigger FROM \"{SHARED_SPACE_DELETED_ROWS_TABLE}\" \
                 WHERE haex_hlc_no_trigger IS NOT NULL \
                   AND CAST(substr(haex_hlc_no_trigger, 1, instr(haex_hlc_no_trigger, '/') - 1) AS INTEGER) < ?1"
            ))?;
            let per_space_maxes =
                collect_per_space_max(per_space_stmt.query_map([cutoff], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?)?;
            drop(per_space_stmt);
            for (space_id, hlc) in &per_space_maxes {
                advance_shared_space_anchor(&*tx, space_id, hlc)?;
            }

            tx.execute(
                &format!(
                    "DELETE FROM \"{SHARED_SPACE_DELETED_ROWS_TABLE}\" \
                     WHERE haex_hlc_no_trigger IS NOT NULL \
                       AND CAST(substr(haex_hlc_no_trigger, 1, instr(haex_hlc_no_trigger, '/') - 1) AS INTEGER) < ?1"
                ),
                [cutoff],
            )?;
        }
    }
    Ok(())
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

/// Read the current HLC from `haex_crdt_configs_no_sync` and reduce it by
/// `days` to produce the SQLite-signed cutoff for time-based pruning.
///
/// Mirrors the crate's private `compute_cutoff` (see
/// `haex_crdt::crdt::cleanup::compute_cutoff`) so the vault-side pass sees
/// the same cutoff as the crate-side pass and both DELETEs stay in lockstep.
///
/// Returns `None` when no HLC is recorded yet (fresh vault) or when the
/// cutoff would overflow `i64` — SQLite stores integers signed 64-bit, so an
/// `as i64` cast on `u64 > i64::MAX` would wrap negative and silently skew
/// the comparison.
fn compute_cutoff_hlc_num_from_config(
    tx: &Transaction,
    days: u32,
) -> Result<Option<i64>, DatabaseError> {
    let current_hlc_str: Option<String> = tx
        .query_row(
            &format!("SELECT value FROM {TABLE_CRDT_CONFIGS} WHERE key = ?1 AND type = 'hlc'"),
            ["hlc_timestamp"],
            |row| row.get(0),
        )
        .optional()?;
    let Some(current_hlc_str) = current_hlc_str else {
        return Ok(None);
    };

    let current_timestamp =
        Timestamp::from_str(&current_hlc_str).map_err(|e| DatabaseError::HlcError {
            reason: format!(
                "compaction_anchor: invalid HLC in config '{current_hlc_str}': {e:?}"
            ),
        })?;

    let ns_per_day: u64 = 24 * 60 * 60 * 1_000_000_000;
    let retention_ns = u64::from(days).saturating_mul(ns_per_day);
    let cutoff = current_timestamp
        .get_time()
        .as_u64()
        .saturating_sub(retention_ns);
    Ok(i64::try_from(cutoff).ok())
}

#[cfg(test)]
#[path = "compaction_anchor_tests.rs"]
mod anchor_tests;
