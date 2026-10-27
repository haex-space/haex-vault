//! The rows of sync rules (data-model.md): the rules of one device, the state of each file after the
//! last success, and the deletions waiting for a confirmation. Every query names the device, so a
//! vault copied to another device sees none of them (ADR 0018).

use haex_crdt::rusqlite::{params, Row};
use haex_crdt::CrdtTransaction;

use super::model::{
    parse, source_of, source_text, stored, SyncRule, SyncRuleDraft, SyncRuleOutcome, SyncRulePlace,
    SyncRuleSide,
};
use crate::error::Result;
use crate::storage::query::Query;

const RULE_COLUMNS: &str = "id, name, enabled, place_a_source, place_a_path, place_b_source, \
     place_b_path, direction, delete_mode, interval_secs, created_at, updated_at, last_run_at, \
     last_outcome, last_error, allow_decrypted";

/// Size and modification time of a file on one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    pub size: u64,
    pub mtime_ms: i64,
}

/// What a rule knows of one file from its last success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateRow {
    /// Relative to the place, `/` between names.
    pub path: String,
    pub a: Option<Seen>,
    pub b: Option<Seen>,
    /// The delete mode "keep" left it on purpose; it is not copied back.
    pub kept: bool,
    /// SHA-256 of the content (hex), once known.
    pub hash: Option<String>,
}

impl StateRow {
    /// What the row knows of `side`.
    pub fn seen(&self, side: SyncRuleSide) -> Option<Seen> {
        match side {
            SyncRuleSide::A => self.a,
            SyncRuleSide::B => self.b,
        }
    }
}

/// A deletion waiting for the user (delete mode "ask").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRow {
    pub path: String,
    /// The side the file would be deleted on.
    pub side: SyncRuleSide,
    pub found_run_id: String,
}

/// Stores a new rule of `device`. The draft is checked by [`super::rules::validate`] before.
pub fn insert_rule(
    tx: &mut CrdtTransaction<'_>,
    device: &str,
    id: &str,
    draft: &SyncRuleDraft,
    now: &str,
) -> Result<()> {
    tx.execute(
        "INSERT INTO haex_sync_rules_no_sync (id, vault_device_uuid, name, enabled, \
         place_a_source, place_a_path, place_b_source, place_b_path, direction, delete_mode, \
         interval_secs, created_at, updated_at, allow_decrypted) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13)",
        params![
            id,
            device,
            draft.name,
            i64::from(draft.enabled),
            source_text(&draft.place_a.source),
            draft.place_a.path,
            source_text(&draft.place_b.source),
            draft.place_b.path,
            stored(&draft.direction),
            stored(&draft.delete_mode),
            i64::from(draft.interval_secs),
            now,
            i64::from(draft.allow_decrypted)
        ],
    )?;
    Ok(())
}

/// Replaces what the user set on a rule; returns whether it exists. A change of places or
/// direction drops the state and the pending deletions (data-model.md).
pub fn update_rule(
    tx: &mut CrdtTransaction<'_>,
    device: &str,
    id: &str,
    draft: &SyncRuleDraft,
    now: &str,
) -> Result<bool> {
    let Some(before) = rule(tx, device, id)? else {
        return Ok(false);
    };
    tx.execute(
        "UPDATE haex_sync_rules_no_sync SET name = ?3, enabled = ?4, place_a_source = ?5, \
         place_a_path = ?6, place_b_source = ?7, place_b_path = ?8, direction = ?9, \
         delete_mode = ?10, interval_secs = ?11, updated_at = ?12, allow_decrypted = ?13 \
         WHERE id = ?1 AND vault_device_uuid = ?2",
        params![
            id,
            device,
            draft.name,
            i64::from(draft.enabled),
            source_text(&draft.place_a.source),
            draft.place_a.path,
            source_text(&draft.place_b.source),
            draft.place_b.path,
            stored(&draft.direction),
            stored(&draft.delete_mode),
            i64::from(draft.interval_secs),
            now,
            i64::from(draft.allow_decrypted)
        ],
    )?;
    let moved = before.place_a != draft.place_a
        || before.place_b != draft.place_b
        || before.direction != draft.direction;
    if moved {
        clear_state(tx, device, id)?;
    }
    Ok(true)
}

/// Pauses or resumes a rule; returns whether it exists.
pub fn set_enabled(
    tx: &mut CrdtTransaction<'_>,
    device: &str,
    id: &str,
    enabled: bool,
    now: &str,
) -> Result<bool> {
    let changed = tx.execute(
        "UPDATE haex_sync_rules_no_sync SET enabled = ?3, updated_at = ?4 \
         WHERE id = ?1 AND vault_device_uuid = ?2",
        params![id, device, i64::from(enabled), now],
    )?;
    Ok(changed > 0)
}

/// Deletes a rule with its state, pending deletions and log; returns whether it existed. The
/// files of both places stay (FR-006).
pub fn delete_rule(tx: &mut CrdtTransaction<'_>, device: &str, id: &str) -> Result<bool> {
    let changed = tx.execute(
        "DELETE FROM haex_sync_rules_no_sync WHERE id = ?1 AND vault_device_uuid = ?2",
        params![id, device],
    )?;
    Ok(changed > 0)
}

/// Notes how the last run of a rule ended.
pub fn set_result(
    tx: &mut CrdtTransaction<'_>,
    device: &str,
    id: &str,
    at: &str,
    outcome: SyncRuleOutcome,
    error: Option<&str>,
) -> Result<()> {
    tx.execute(
        "UPDATE haex_sync_rules_no_sync SET last_run_at = ?3, last_outcome = ?4, last_error = ?5 \
         WHERE id = ?1 AND vault_device_uuid = ?2",
        params![id, device, at, stored(&outcome), error],
    )?;
    Ok(())
}

/// The rules of `device`, by name.
pub fn rules(q: &mut impl Query, device: &str) -> Result<Vec<SyncRule>> {
    Ok(q.query_map(
        &format!(
            "SELECT {RULE_COLUMNS} FROM haex_sync_rules_no_sync WHERE vault_device_uuid = ?1 \
             ORDER BY name, id"
        ),
        &[&device],
        rule_of_row,
    )?
    .into_iter()
    .flatten()
    .collect())
}

/// One rule of `device`.
pub fn rule(q: &mut impl Query, device: &str, id: &str) -> Result<Option<SyncRule>> {
    Ok(q.query_row(
        &format!(
            "SELECT {RULE_COLUMNS} FROM haex_sync_rules_no_sync \
             WHERE id = ?1 AND vault_device_uuid = ?2"
        ),
        &[&id, &device],
        rule_of_row,
    )?
    .flatten())
}

/// The state of every file of a rule.
pub fn state(q: &mut impl Query, device: &str, rule_id: &str) -> Result<Vec<StateRow>> {
    Ok(q.query_map(
        "SELECT path, a_size, a_mtime_ms, b_size, b_mtime_ms, kept, hash \
         FROM haex_sync_rule_files_no_sync WHERE rule_id = ?1 AND vault_device_uuid = ?2 \
         ORDER BY path",
        &[&rule_id, &device],
        |r| {
            Ok(StateRow {
                path: r.get(0)?,
                a: seen_of(r.get(1)?, r.get(2)?),
                b: seen_of(r.get(3)?, r.get(4)?),
                kept: r.get::<_, i64>(5)? != 0,
                hash: r.get(6)?,
            })
        },
    )?)
}

/// Writes the state of one file after its step succeeded (FR-015).
pub fn put_state(
    tx: &mut CrdtTransaction<'_>,
    device: &str,
    rule_id: &str,
    row: &StateRow,
) -> Result<()> {
    let size = |seen: Option<Seen>| seen.map(|s| i64::try_from(s.size).unwrap_or(i64::MAX));
    tx.execute(
        "INSERT INTO haex_sync_rule_files_no_sync (rule_id, vault_device_uuid, path, a_size, \
         a_mtime_ms, b_size, b_mtime_ms, kept, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
         ON CONFLICT(rule_id, path) DO UPDATE SET a_size = excluded.a_size, \
         a_mtime_ms = excluded.a_mtime_ms, b_size = excluded.b_size, \
         b_mtime_ms = excluded.b_mtime_ms, kept = excluded.kept, hash = excluded.hash",
        params![
            rule_id,
            device,
            row.path,
            size(row.a),
            row.a.map(|s| s.mtime_ms),
            size(row.b),
            row.b.map(|s| s.mtime_ms),
            i64::from(row.kept),
            row.hash
        ],
    )?;
    Ok(())
}

/// Forgets one file of a rule.
pub fn remove_state(
    tx: &mut CrdtTransaction<'_>,
    device: &str,
    rule_id: &str,
    path: &str,
) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_sync_rule_files_no_sync \
         WHERE rule_id = ?1 AND vault_device_uuid = ?2 AND path = ?3",
        params![rule_id, device, path],
    )?;
    Ok(())
}

/// Moves the state of a file to its new path after a move (research R4); a row already at `to`
/// is replaced.
pub fn rename_state(
    tx: &mut CrdtTransaction<'_>,
    device: &str,
    rule_id: &str,
    from: &str,
    to: &str,
) -> Result<()> {
    remove_state(tx, device, rule_id, to)?;
    tx.execute(
        "UPDATE haex_sync_rule_files_no_sync SET path = ?4 \
         WHERE rule_id = ?1 AND vault_device_uuid = ?2 AND path = ?3",
        params![rule_id, device, from, to],
    )?;
    Ok(())
}

/// Forgets the state and the pending deletions of a rule, so the next run is a first run.
pub fn clear_state(tx: &mut CrdtTransaction<'_>, device: &str, rule_id: &str) -> Result<()> {
    for table in [
        "haex_sync_rule_files_no_sync",
        "haex_sync_rule_pending_no_sync",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE rule_id = ?1 AND vault_device_uuid = ?2"),
            params![rule_id, device],
        )?;
    }
    Ok(())
}

/// The deletions of a rule waiting for the user.
pub fn pending(q: &mut impl Query, device: &str, rule_id: &str) -> Result<Vec<PendingRow>> {
    Ok(q.query_map(
        "SELECT path, side, found_run_id FROM haex_sync_rule_pending_no_sync \
         WHERE rule_id = ?1 AND vault_device_uuid = ?2 ORDER BY path",
        &[&rule_id, &device],
        |r| {
            let side = parse::<SyncRuleSide>(&r.get::<_, String>(1)?);
            let (path, found_run_id) = (r.get(0)?, r.get(2)?);
            Ok(side.map(|side| PendingRow {
                path,
                side,
                found_run_id,
            }))
        },
    )?
    .into_iter()
    .flatten()
    .collect())
}

/// Notes a deletion waiting for the user; a row for the same path is replaced.
pub fn put_pending(
    tx: &mut CrdtTransaction<'_>,
    device: &str,
    rule_id: &str,
    row: &PendingRow,
) -> Result<()> {
    tx.execute(
        "INSERT INTO haex_sync_rule_pending_no_sync (rule_id, vault_device_uuid, path, side, \
         found_run_id) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(rule_id, path) DO UPDATE SET \
         side = excluded.side, found_run_id = excluded.found_run_id",
        params![
            rule_id,
            device,
            row.path,
            stored(&row.side),
            row.found_run_id
        ],
    )?;
    Ok(())
}

/// Drops a waiting deletion.
pub fn remove_pending(
    tx: &mut CrdtTransaction<'_>,
    device: &str,
    rule_id: &str,
    path: &str,
) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_sync_rule_pending_no_sync \
         WHERE rule_id = ?1 AND vault_device_uuid = ?2 AND path = ?3",
        params![rule_id, device, path],
    )?;
    Ok(())
}

fn seen_of(size: Option<i64>, mtime_ms: Option<i64>) -> Option<Seen> {
    Some(Seen {
        size: u64::try_from(size?).ok()?,
        mtime_ms: mtime_ms?,
    })
}

/// A rule from its row; `None` for one a newer version wrote.
fn rule_of_row(r: &Row<'_>) -> haex_crdt::rusqlite::Result<Option<SyncRule>> {
    let place = |source: String, path: String| {
        source_of(&source).map(|source| SyncRulePlace { source, path })
    };
    let (Some(place_a), Some(place_b), Some(direction), Some(delete_mode)) = (
        place(r.get(3)?, r.get(4)?),
        place(r.get(5)?, r.get(6)?),
        parse(&r.get::<_, String>(7)?),
        parse(&r.get::<_, String>(8)?),
    ) else {
        return Ok(None);
    };
    Ok(Some(SyncRule {
        id: r.get(0)?,
        name: r.get(1)?,
        enabled: r.get::<_, i64>(2)? != 0,
        place_a,
        place_b,
        direction,
        delete_mode,
        interval_secs: u32::try_from(r.get::<_, i64>(9)?)
            .unwrap_or(super::model::MAX_INTERVAL_SECS),
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
        last_run_at: r.get(12)?,
        last_outcome: r
            .get::<_, Option<String>>(13)?
            .and_then(|text| parse(&text)),
        last_error: r.get(14)?,
        allow_decrypted: r.get::<_, i64>(15)? != 0,
    }))
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
