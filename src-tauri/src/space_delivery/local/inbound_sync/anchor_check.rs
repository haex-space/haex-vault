//! Compaction-anchor gate for inbound SyncPush (ADR 0002 §6.5, Task 9).
//!
//! The retention job periodically prunes old entries from
//! `haex_shared_space_deleted_rows` and advances the per-space
//! `haex_space_compaction_anchors.min_valid_hlc` to the max HLC of pruned
//! entries. A peer who was offline long enough that its local outbound queue
//! contains changes older than the anchor would resurrect rows whose delete-
//! signal has been pruned — nobody would know to re-emit the delete.
//!
//! This module rejects any inbound push carrying at least one change with
//! `hlc_timestamp < anchor` for the target space, so the peer knows to
//! discard its stale outbound queue and refresh-pull.

use crate::crdt::hlc::compare_hlc_strings;
use crate::crdt::scanner::LocalColumnChange;
use crate::database::DbConnection;

/// A rejection carrying enough context for the client to trigger a refresh
/// pull for the affected space. The `anchor` is the minimum-valid HLC that
/// the peer's outbound queue must clear before another push is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BelowCompactionAnchor {
    pub space_id: String,
    pub anchor: String,
    /// Smallest offending HLC observed in the batch. Useful for logs.
    pub offending_hlc: String,
    /// Count of changes below anchor. Rest of the batch is also rejected
    /// (a partial apply would produce a half-torn state on the peer).
    pub below_anchor_count: usize,
}

impl std::fmt::Display for BelowCompactionAnchor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BelowCompactionAnchor: space={} anchor={} smallest_hlc={} below_anchor_count={}",
            self.space_id, self.anchor, self.offending_hlc, self.below_anchor_count
        )
    }
}

/// Look up the compaction anchor for `space_id`. Returns `None` when no
/// anchor row exists for this space (retention has never pruned entries).
///
/// Missing-table tolerance is `#[cfg(test)]`-gated so unit-test fixtures
/// that pre-date migration 0013 can skip the anchor table. In production,
/// a genuine missing anchor table is a hard error — silently returning
/// `Ok(None)` would disable the anti-resurrection gate.
pub fn load_anchor(db: &DbConnection, space_id: &str) -> Result<Option<String>, String> {
    let guard = db.0.lock().map_err(|e| format!("lock poison: {e}"))?;
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database connection is closed".to_string())?;
    conn.query_row(
        "SELECT min_valid_hlc FROM haex_space_compaction_anchors WHERE space_id = ?1",
        [space_id],
        |row| row.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        #[cfg(test)]
        rusqlite::Error::SqliteFailure(_, Some(ref msg))
            if msg.contains("no such table: haex_space_compaction_anchors") =>
        {
            Ok(None)
        }
        other => Err(format!("anchor lookup failed: {other}")),
    })
}

/// Rejection check: returns `Some(BelowCompactionAnchor)` iff at least one
/// change carries an HLC strictly older than the anchor. `Ok(None)` means
/// the batch is safe to apply w.r.t. anti-resurrection.
///
/// Passing an empty batch or a space with no anchor yields `Ok(None)`.
pub fn check_batch_against_anchor(
    db: &DbConnection,
    space_id: &str,
    changes: &[LocalColumnChange],
) -> Result<Option<BelowCompactionAnchor>, String> {
    if changes.is_empty() {
        return Ok(None);
    }
    let anchor = match load_anchor(db, space_id)? {
        Some(a) => a,
        None => return Ok(None),
    };

    let mut smallest: Option<&str> = None;
    let mut count = 0usize;
    for change in changes {
        if compare_hlc_strings(change.hlc_timestamp.as_str(), anchor.as_str())
            == std::cmp::Ordering::Less
        {
            count += 1;
            smallest = Some(match smallest {
                Some(cur) => {
                    if compare_hlc_strings(change.hlc_timestamp.as_str(), cur)
                        == std::cmp::Ordering::Less
                    {
                        change.hlc_timestamp.as_str()
                    } else {
                        cur
                    }
                }
                None => change.hlc_timestamp.as_str(),
            });
        }
    }

    Ok(smallest.map(|s| BelowCompactionAnchor {
        space_id: space_id.to_string(),
        anchor,
        offending_hlc: s.to_string(),
        below_anchor_count: count,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DbConnection;
    use rusqlite::Connection;
    use serde_json::Value as JsonValue;
    use std::sync::{Arc, Mutex};

    fn setup_db_with_anchor(space_id: &str, anchor_hlc: &str) -> DbConnection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE haex_space_compaction_anchors (
                 space_id TEXT PRIMARY KEY NOT NULL,
                 min_valid_hlc TEXT NOT NULL DEFAULT '0'
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_space_compaction_anchors (space_id, min_valid_hlc) VALUES (?1, ?2)",
            rusqlite::params![space_id, anchor_hlc],
        )
        .unwrap();
        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    fn setup_db_no_anchor_row(space_id_stub: &str) -> DbConnection {
        let _ = space_id_stub;
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE haex_space_compaction_anchors (
                 space_id TEXT PRIMARY KEY NOT NULL,
                 min_valid_hlc TEXT NOT NULL DEFAULT '0'
             );",
        )
        .unwrap();
        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    fn change_at_hlc(hlc: &str) -> LocalColumnChange {
        LocalColumnChange {
            table_name: "ext_notes_items".to_string(),
            row_pks: r#"{"id":"r1"}"#.to_string(),
            column_name: "body".to_string(),
            hlc_timestamp: hlc.to_string(),
            value: JsonValue::String("hello".to_string()),
            device_id: "dev-1".to_string(),
        }
    }

    #[test]
    fn no_anchor_present_accepts_any_batch() {
        let db = setup_db_no_anchor_row("SPACE_X");
        let out = check_batch_against_anchor(&db, "SPACE_X", &[change_at_hlc("5/aabb")]).unwrap();
        assert!(out.is_none(), "no anchor row → nothing to check against");
    }

    #[test]
    fn empty_batch_is_accepted_even_with_anchor() {
        let db = setup_db_with_anchor("SPACE_X", "100/aabb");
        let out = check_batch_against_anchor(&db, "SPACE_X", &[]).unwrap();
        assert!(out.is_none(), "empty batch is always safe");
    }

    #[test]
    fn batch_entirely_above_anchor_accepted() {
        let db = setup_db_with_anchor("SPACE_X", "100/aabb");
        let out = check_batch_against_anchor(
            &db,
            "SPACE_X",
            &[change_at_hlc("101/aabb"), change_at_hlc("200/aabb")],
        )
        .unwrap();
        assert!(out.is_none());
    }

    #[test]
    fn batch_with_change_below_anchor_rejected() {
        let db = setup_db_with_anchor("SPACE_X", "100/aabb");
        let out = check_batch_against_anchor(
            &db,
            "SPACE_X",
            &[change_at_hlc("50/aabb"), change_at_hlc("200/aabb")],
        )
        .unwrap();
        let rej = out.expect("must reject");
        assert_eq!(rej.space_id, "SPACE_X");
        assert_eq!(rej.anchor, "100/aabb");
        assert_eq!(rej.offending_hlc, "50/aabb");
        assert_eq!(rej.below_anchor_count, 1);
    }

    #[test]
    fn rejection_reports_the_smallest_offending_hlc() {
        let db = setup_db_with_anchor("SPACE_X", "100/aabb");
        let out = check_batch_against_anchor(
            &db,
            "SPACE_X",
            &[
                change_at_hlc("50/aabb"),
                change_at_hlc("20/aabb"),
                change_at_hlc("80/aabb"),
                change_at_hlc("200/aabb"),
            ],
        )
        .unwrap();
        let rej = out.expect("must reject");
        assert_eq!(rej.offending_hlc, "20/aabb");
        assert_eq!(rej.below_anchor_count, 3);
    }
}
