//! Migration tests for `0053_sync_rules_source_target` (spec 075): a vault from before it keeps every row
//! of the five sync rule tables, with the old A as source and the old B as target, the direction and the
//! sides under their new names, and the rows in their old order; the migration runs once; a fresh vault
//! has the new columns only.

// These tests read `sqlite_master` and write rows directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_sync_rules::TABLES;
use super::migrations_sync_rules_tests::fill_before_0053;
use super::migrations_tests::{column_names, device_of, migration_source_before, open};
use crate::file_sync::ignore::Exclusions;
use crate::file_sync::model::{SyncRuleDirection, SyncRulePlace, SyncRuleSide};
use crate::file_sync::place::{Facts, Listing};
use crate::file_sync::plan::{self, Hashes, Inputs};
use crate::file_sync::{run_log, store};
use crate::files::SourceRef;
use crate::identity::holzi_migration_source;

const PASSPHRASE: &str = "sync-rules-source-target";

fn count(db: &Database, table: &str) -> i64 {
    db.with_connection(|conn| {
        Ok(conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?)
    })
    .expect("count")
}

/// Two text columns of every row, in the order SQLite stores them.
fn pairs(db: &Database, sql: &str) -> Vec<(String, String)> {
    db.with_connection(|conn| {
        let mut statement = conn.prepare(sql)?;
        let rows =
            statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .expect("rows")
}

const PENDING_ORDER: &str =
    "SELECT rule_id || '/' || path, side FROM haex_sync_rule_pending_no_sync ORDER BY rowid";
const ITEMS_ORDER: &str = "SELECT run_id || '/' || CAST(seq AS TEXT), side \
     FROM haex_sync_run_items_no_sync ORDER BY rowid";
const STATE_ORDER: &str =
    "SELECT rule_id || '/' || path, CAST(rowid AS TEXT) FROM haex_sync_rule_files_no_sync ORDER BY rowid";

/// The rows of `fill_before_0053` plus a pending deletion and a run item on the old side `a`, and
/// a pending deletion and a run item with a side no version wrote (`x`).
fn fill_both_sides(db: &Database, device: &str) {
    fill_before_0053(db, device);
    db.with_connection(|conn| {
        conn.execute(
            "INSERT INTO haex_sync_rule_pending_no_sync (rule_id, vault_device_uuid, path, side, \
             found_run_id) VALUES ('r1', ?1, 'a.jpg', 'a', 'run1'), ('r1', ?1, 'odd.jpg', 'x', 'run1')",
            params![device],
        )?;
        conn.execute(
            "INSERT INTO haex_sync_run_items_no_sync (run_id, seq, path, action, side) \
             VALUES ('run1', 1, 'a.jpg', 'delete', 'a'), ('run1', 2, 'odd.jpg', 'delete', 'x')",
            [],
        )?;
        Ok(())
    })
    .expect("fill both sides");
}

#[tokio::test]
async fn migration_0053_renames_the_columns_and_values_and_keeps_every_row() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            PASSPHRASE,
            true,
            migration_source_before("0053_sync_rules_source_target"),
        );
        let device = device_of(&old, dir.path()).to_string();
        fill_both_sides(&old, &device);
        let counts: Vec<i64> = TABLES.iter().map(|table| count(&old, table)).collect();
        let state = pairs(&old, STATE_ORDER);
        let pending = pairs(&old, PENDING_ORDER);
        let items = pairs(&old, ITEMS_ORDER);
        drop(old);

        let db = open(dir.path(), PASSPHRASE, false, holzi_migration_source());

        let after: Vec<i64> = TABLES.iter().map(|table| count(&db, table)).collect();
        assert_eq!(after, counts, "no table lost or gained a row");
        assert_eq!(pairs(&db, STATE_ORDER), state, "state rows keep their order");

        let rule = pairs(
            &db,
            "SELECT source_location || ' ' || source_path || ' ' || target_location || ' ' || target_path, \
             direction FROM haex_sync_rules_no_sync WHERE id = 'r1'",
        );
        assert_eq!(
            rule,
            [("device /a device /b".to_owned(), "sourceToTarget".to_owned())],
            "A became the source, B the target, aToB became sourceToTarget"
        );

        let numbers = pairs(
            &db,
            "SELECT CAST(source_size AS TEXT) || ' ' || CAST(source_mtime_ms AS TEXT), \
             CAST(target_size AS TEXT) || ' ' || CAST(target_mtime_ms AS TEXT) \
             FROM haex_sync_rule_files_no_sync WHERE path = 'x.jpg'",
        );
        assert_eq!(
            numbers,
            [("1 2".to_owned(), "1 3".to_owned())],
            "the per-file numbers keep their meaning: a_* is source_*, b_* is target_*"
        );

        let expected = |before: &[(String, String)]| -> Vec<(String, String)> {
            before
                .iter()
                .map(|(key, side)| {
                    let side = match side.as_str() {
                        "a" => "source",
                        "b" => "target",
                        other => other,
                    };
                    (key.clone(), side.to_owned())
                })
                .collect()
        };
        assert_eq!(
            pairs(&db, PENDING_ORDER),
            expected(&pending),
            "pending deletions: a to source, b to target, an unknown side stays, same order"
        );
        assert_eq!(
            pairs(&db, ITEMS_ORDER),
            expected(&items),
            "run items: a to source, b to target, an unknown side stays, same order"
        );
        assert!(
            pending.iter().any(|(_, side)| side == "a")
                && pending.iter().any(|(_, side)| side == "b")
                && pending.iter().any(|(_, side)| side == "x"),
            "the fixture holds all three kinds of side"
        );
        drop(db);

        // The migration runs once: opening the migrated vault again changes nothing.
        let again = open(dir.path(), PASSPHRASE, false, holzi_migration_source());
        let again_counts: Vec<i64> = TABLES.iter().map(|table| count(&again, table)).collect();
        assert_eq!(again_counts, counts);
        assert_eq!(pairs(&again, PENDING_ORDER), expected(&pending));
        assert_eq!(pairs(&again, ITEMS_ORDER), expected(&items));
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0053_gives_a_fresh_vault_the_new_columns_only() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "sync-rules-source-target-fresh",
            true,
            holzi_migration_source(),
        );

        let rules = column_names(&db, "haex_sync_rules_no_sync");
        for column in [
            "source_location",
            "source_path",
            "target_location",
            "target_path",
        ] {
            assert!(rules.iter().any(|c| c == column), "{column} exists");
        }
        let files = column_names(&db, "haex_sync_rule_files_no_sync");
        for column in [
            "source_size",
            "source_mtime_ms",
            "target_size",
            "target_mtime_ms",
        ] {
            assert!(files.iter().any(|c| c == column), "{column} exists");
        }
        for column in rules.iter().chain(files.iter()) {
            assert!(
                !column.starts_with("place_a")
                    && !column.starts_with("place_b")
                    && !["a_size", "a_mtime_ms", "b_size", "b_mtime_ms"].contains(&column.as_str()),
                "{column} is an old name"
            );
        }
    })
    .await
    .expect("join");
}

/// A vault from before `0053` with its rows, opened by the current version.
fn migrated_vault() -> (tempfile::TempDir, Database, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = open(
        dir.path(),
        PASSPHRASE,
        true,
        migration_source_before("0053_sync_rules_source_target"),
    );
    let device = device_of(&old, dir.path()).to_string();
    fill_both_sides(&old, &device);
    drop(old);
    let db = open(dir.path(), PASSPHRASE, false, holzi_migration_source());
    (dir, db, device)
}

#[tokio::test]
async fn the_rust_readers_find_every_migrated_row() {
    tokio::task::spawn_blocking(|| {
        let (_dir, db, device) = migrated_vault();
        let rules = crate::storage::query::read(&db, |r| Ok(store::rules(r, &device)))
            .expect("read")
            .expect("rules");
        assert_eq!(rules.len(), 1, "the rule is still there");
        let place = |path: &str| SyncRulePlace {
            location: SourceRef::Device,
            path: path.to_owned(),
        };
        assert_eq!(rules[0].source, place("/a"));
        assert_eq!(rules[0].target, place("/b"));
        assert_eq!(rules[0].direction, SyncRuleDirection::SourceToTarget);

        let pending = crate::storage::query::read(&db, |r| Ok(store::pending(r, &device, "r1")))
            .expect("read")
            .expect("pending");
        let sides: Vec<(String, SyncRuleSide)> = pending
            .into_iter()
            .map(|row| (row.path, row.side))
            .collect();
        assert!(sides.contains(&("y.jpg".to_owned(), SyncRuleSide::Target)));
        assert!(sides.contains(&("a.jpg".to_owned(), SyncRuleSide::Source)));
        assert_eq!(
            sides.len(),
            2,
            "only the row with a side no version wrote is not read"
        );

        let items =
            crate::storage::query::read(&db, |r| Ok(run_log::items(r, &device, "run1", 0, 10)))
                .expect("read")
                .expect("items");
        let sides: Vec<(String, SyncRuleSide)> = items
            .into_iter()
            .map(|item| (item.path, item.side))
            .collect();
        assert_eq!(
            sides,
            [
                ("x.jpg".to_owned(), SyncRuleSide::Target),
                ("a.jpg".to_owned(), SyncRuleSide::Source)
            ],
            "the run items keep their sides and their order"
        );
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn a_rule_with_nothing_to_do_plans_no_step_after_the_migration() {
    tokio::task::spawn_blocking(|| {
        let (_dir, db, device) = migrated_vault();
        let rule = crate::storage::query::read(&db, |r| Ok(store::rules(r, &device)))
            .expect("read")
            .expect("rules")
            .remove(0);
        let state = crate::storage::query::read(&db, |r| Ok(store::state(r, &device, "r1")))
            .expect("read")
            .expect("state");
        // The files are as the state of the last success saw them: size 1 at the source from time
        // 2, size 1 at the target from time 3 (the fixture's numbers).
        let listing = |mtime_ms: i64| {
            let mut listing = Listing::default();
            listing.files.insert(
                "x.jpg".to_owned(),
                Facts {
                    size: 1,
                    mtime_ms,
                    hash: None,
                },
            );
            listing
        };
        let (source, target) = (listing(2), listing(3));
        let exclusions = Exclusions::new(None, None).expect("no exclusions");
        let hashes = Hashes::new();
        let planned = plan::build(&Inputs {
            rule: &rule,
            source: &source,
            target: &target,
            state: &state,
            exclusions: &exclusions,
            hashes: &hashes,
            conflict_name: &|path, _| format!("{path}.conflict"),
        });
        assert!(
            planned.steps.is_empty(),
            "the migrated state keeps its meaning, so nothing is copied again: {:?}",
            planned.steps
        );
    })
    .await
    .expect("join");
}
