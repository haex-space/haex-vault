//! Progress per origin device (spec 024, FR-019, research R4).
//!
//! Every cell carries the node id of the device that wrote it in its HLC;
//! that node id is the writer's `vault_device_uuid`, the origin. A device's
//! progress for an origin is the highest HLC up to which it has applied, or
//! rejected under research R5, every change of that origin. Progress only
//! grows, and only after the changes it covers are committed, so every
//! device holds a gap-free prefix of each origin's history and the next
//! pull, from whichever device, starts right after it.
//!
//! For itself a device is complete up to the newest cell it wrote into a
//! synced table ([`crate::sync::replica::Replica::progress`]). That is not
//! haex-crdt's persisted HLC: every write transaction advances that one,
//! also one that only touched device-local tables, and advertising it would
//! make the devices pull from each other without end. A stored row for the
//! own origin only exists when a peer delivered own changes this file did
//! not have, as after restoring an older copy.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use haex_crdt::rusqlite::params;
use haex_crdt::{compare_hlc_strings, hlc_origin, CrdtTransaction};
use uuid::Uuid;

use crate::storage::query::Query;

/// Progress per origin: origin → highest HLC covered. A missing origin
/// means "nothing".
pub type Vector = BTreeMap<Uuid, String>;

/// The origin of an HLC: the `vault_device_uuid` haex-crdt built its node id
/// from.
pub fn origin_of(hlc: &str) -> Option<Uuid> {
    hlc_origin(hlc)
}

/// Whether `hlc` lies beyond `cursor`; everything lies beyond "nothing".
pub fn is_beyond(hlc: &str, cursor: Option<&String>) -> bool {
    cursor.is_none_or(|cursor| compare_hlc_strings(hlc, cursor) == Ordering::Greater)
}

/// The stored progress rows.
pub fn stored(q: &mut impl Query) -> haex_crdt::Result<Vector> {
    let rows: Vec<(String, String)> = q.query_map(
        "SELECT origin, max_hlc FROM sync_progress_no_sync",
        &[],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(rows
        .into_iter()
        .filter_map(|(origin, max_hlc)| Some((Uuid::parse_str(&origin).ok()?, max_hlc)))
        .collect())
}

/// Raises the stored progress to `updates`, per origin, never lowering it.
/// Runs inside the caller's transaction, after the changes it covers are
/// committed.
pub fn advance(tx: &mut CrdtTransaction<'_>, updates: &Vector) -> haex_crdt::Result<()> {
    for (origin, hlc) in updates {
        let origin = origin.to_string();
        let stored: Option<String> = tx.query_row(
            "SELECT max_hlc FROM sync_progress_no_sync WHERE origin = ?1",
            params![origin],
            |r| r.get(0),
        )?;
        if !is_beyond(hlc, stored.as_ref()) {
            continue;
        }
        tx.execute(
            "INSERT INTO sync_progress_no_sync (origin, max_hlc) VALUES (?1, ?2) \
             ON CONFLICT(origin) DO UPDATE SET max_hlc = excluded.max_hlc",
            params![origin, hlc],
        )?;
    }
    Ok(())
}

/// Whether `theirs` covers any origin further than `ours`: then there is
/// something to pull from them.
pub fn has_more(theirs: &Vector, ours: &Vector) -> bool {
    theirs
        .iter()
        .any(|(origin, hlc)| is_beyond(hlc, ours.get(origin)))
}

/// Raises `vector[origin]` to `hlc` if that is further.
pub fn raise(vector: &mut Vector, origin: Uuid, hlc: String) {
    if is_beyond(&hlc, vector.get(&origin)) {
        vector.insert(origin, hlc);
    }
}

/// The cursor right before `hlc` of the same origin, so a pull from it brings `hlc` again;
/// `None` when nothing lies before it.
pub fn just_below(hlc: &str) -> Option<String> {
    let (time, node) = hlc.split_once('/')?;
    let before = time.parse::<u64>().ok()?.checked_sub(1)?;
    Some(format!("{before}/{node}"))
}

/// Raises `current` to `hlc` if that is further.
pub fn raise_option(current: &mut Option<String>, hlc: String) {
    if is_beyond(&hlc, current.as_ref()) {
        *current = Some(hlc);
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
