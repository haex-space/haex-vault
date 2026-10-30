//! Replaying parked groups once their tables and columns exist (spec 017, research R10). Part of
//! [`super`].

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::PoisonError;

use haex_crdt::rusqlite::params;
use haex_crdt::{compare_hlc_strings, ColumnChange, Database};

use super::{refetch, sort, Context, Sorted, OUTDATED_ORIGIN, PARKED_TABLE};
use crate::storage::query::{self, Query};

/// What [`replay_ready`] did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Replayed {
    pub groups: usize,
    pub tables: BTreeSet<String>,
}

/// Applies, per extension in HLC order, the parked groups whose tables and columns now exist, and
/// deletes them; an extension's groups stop at the first that still cannot apply. The ready groups
/// of an extension apply together, so a row whose cells came split across them is written whole;
/// one that still fails (a row completed only by a group not ready yet) stays parked and the other
/// extensions go on. Waits for running migrations, so no group lands between two of them. Safe to
/// run concurrently: applying a group twice changes nothing.
/// `stop` is asked before each extension; a closing vault ends the replay there (the rest
/// stays parked for the next run).
pub fn replay_ready(db: &Database, stop: &dyn Fn() -> bool) -> haex_crdt::Result<Replayed> {
    let _migrations = crate::extensions::sql::migrate::applying()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let mut replayed = Replayed::default();
    // Extensions whose ready groups failed to apply: nothing changes for them in this run, so
    // the next round does not read and apply them again.
    let mut failed: HashSet<String> = HashSet::new();
    loop {
        let context = query::read(db, |r| Context::read(r, db.device_id()))?;
        // Up to the parking limit per extension: each group's changes are read when it is tried.
        let rows: Vec<(i64, String, String)> = query::read(db, |r| {
            r.query_map(
                &format!("SELECT id, extension_prefix, hlc FROM {PARKED_TABLE}"),
                &[],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
        })?;
        let mut by_prefix: HashMap<String, Vec<(i64, String)>> = HashMap::new();
        for (id, prefix, hlc) in rows {
            by_prefix.entry(prefix).or_default().push((id, hlc));
        }
        let mut progressed = false;
        for (prefix, mut groups) in by_prefix {
            if stop() {
                return Ok(replayed);
            }
            if failed.contains(&prefix) {
                continue;
            }
            groups.sort_by(|a, b| compare_hlc_strings(&a.1, &b.1));
            let mut ready: Vec<i64> = Vec::new();
            let mut columns: Vec<ColumnChange> = Vec::new();
            for (id, hlc) in groups {
                let blob: Option<Vec<u8>> = query::read(db, |r| {
                    r.query_row(
                        &format!("SELECT group_blob FROM {PARKED_TABLE} WHERE id = ?1"),
                        params![id],
                        |row| row.get(0),
                    )
                })?;
                // Gone meanwhile: replayed by a concurrent run, or discarded.
                let Some(blob) = blob else {
                    continue;
                };
                let group: Vec<ColumnChange> = serde_json::from_slice(&blob)
                    .map_err(|e| haex_crdt::Error::consumer(format!("parked group: {e}")))?;
                // Another extension's earlier parked group comes first.
                let waits = |other: &str| {
                    other != prefix
                        && context.parked.get(other).is_some_and(|(_, earliest)| {
                            compare_hlc_strings(earliest, &hlc) == Ordering::Less
                        })
                };
                match sort(&context, &hlc, group, waits) {
                    // Written by a device with an older version, and a migration applied here
                    // since dropped or renamed what it writes: it can never apply, fetch it again.
                    Ok(Sorted::Park(parked)) if parked.reason == OUTDATED_ORIGIN => {
                        if db.write(|tx| refetch::mark(tx, id))? {
                            log::info!("sync: parked group {hlc} of {prefix} is fetched again");
                        }
                        break;
                    }
                    // Still missing something, or behind another extension's earlier group: the
                    // globally earliest waiting group never waits, so this cannot cycle.
                    Ok(Sorted::Park(_)) => break,
                    Ok(Sorted::Apply(group)) => {
                        ready.push(id);
                        columns.extend(group);
                    }
                    Err(error) => {
                        log::warn!("sync: parked group {hlc} of {prefix} cannot apply: {error}");
                        break;
                    }
                }
            }
            if ready.is_empty() {
                continue;
            }
            // ponytail: one transaction for all ready groups of an extension, up to its parking
            // limit. Upgrade path: settle-style steps that never cut a row.
            let tables: Vec<String> = columns
                .iter()
                .filter_map(super::super::changed_table)
                .collect();
            if !columns.is_empty() {
                match db.apply_remote_changes(columns) {
                    Ok(outcome) => {
                        super::super::report_unknown_columns(&outcome);
                    }
                    Err(error) => {
                        log::warn!("sync: replaying the parked groups of {prefix} failed: {error}");
                        failed.insert(prefix);
                        continue;
                    }
                }
            }
            db.write(|tx| {
                for id in &ready {
                    tx.execute(
                        &format!("DELETE FROM {PARKED_TABLE} WHERE id = ?1"),
                        params![id],
                    )?;
                }
                Ok(())
            })?;
            replayed.tables.extend(tables);
            replayed.groups += ready.len();
            progressed = true;
        }
        if !progressed {
            if replayed.groups > 0 {
                log::info!("sync: replayed {} parked groups", replayed.groups);
            }
            return Ok(replayed);
        }
    }
}
