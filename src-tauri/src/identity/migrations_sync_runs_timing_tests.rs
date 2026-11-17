//! Migration tests for `0064_sync_runs_timing`: a vault from before it keeps its logged runs, which show
//! zero times and no route, and the migration runs once.

// These tests read `sqlite_master` and write rows directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use super::migrations_sync_rules_tests::fill;
use super::migrations_tests::{column_names, device_of, migration_source_before, open};
use crate::file_sync::run_log::{self, SyncRuleRun};
use crate::identity::holzi_migration_source;

const PASSPHRASE: &str = "sync-runs-timing";
const MIGRATION: &str = "0064_sync_runs_timing";
const COLUMNS: [&str; 6] = ["scan_ms", "hash_ms", "copy_ms", "bytes", "route", "rtt_ms"];

fn runs(db: &haex_crdt::Database, device: &str) -> Vec<SyncRuleRun> {
    crate::storage::query::read(db, |r| Ok(run_log::runs(r, device, "r1")))
        .expect("read")
        .expect("runs")
}

#[tokio::test]
async fn migration_0064_keeps_the_logged_runs_and_shows_them_without_times() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            PASSPHRASE,
            true,
            migration_source_before(MIGRATION),
        );
        let device = device_of(&old, dir.path()).to_string();
        fill(&old, &device);
        let columns = column_names(&old, "haex_sync_runs_no_sync");
        assert!(
            COLUMNS
                .iter()
                .all(|c| !columns.iter().any(|name| name == c)),
            "the old schema has none of the columns"
        );
        drop(old);

        let db = open(dir.path(), PASSPHRASE, false, holzi_migration_source());
        let columns = column_names(&db, "haex_sync_runs_no_sync");
        for column in COLUMNS {
            assert!(columns.iter().any(|name| name == column), "{column} exists");
        }
        let runs_after = runs(&db, &device);
        assert_eq!(runs_after.len(), 1, "the run is still read");
        let run = &runs_after[0];
        assert_eq!(
            (run.scan_ms, run.hash_ms, run.copy_ms, run.bytes),
            (0, 0, 0, 0)
        );
        assert_eq!((run.route, run.rtt_ms), (None, None));
        drop(db);

        // The migration runs once: opening the migrated vault again changes nothing.
        let again = open(dir.path(), PASSPHRASE, false, holzi_migration_source());
        assert_eq!(runs(&again, &device), runs_after);
    })
    .await
    .expect("join");
}
