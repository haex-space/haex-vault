//! The run log (FR-043 to FR-046): depth 10 by default keeps the latest ten runs, depth 0 writes
//! nothing, a smaller depth trims every rule at once, every file action is kept without a cap, the
//! trash of a deletion is stored, the log goes with its rule, and runs of another device are not
//! listed.

// These tests insert devices directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;

use super::*;
use crate::file_sync::execute::Counts;
use crate::file_sync::model::{
    SyncRuleDeleteMode, SyncRuleDirection, SyncRuleDraft, SyncRulePlace,
};
use crate::file_sync::store;
use crate::files::SourceRef;
use crate::remote_storage::test_support::vault;
use crate::vault_gate::VaultDb;

const DEVICE: &str = "11111111-1111-4111-8111-111111111111";
const OTHER: &str = "22222222-2222-4222-8222-222222222222";

fn setup() -> (tempfile::TempDir, VaultDb) {
    let (dir, db) = vault();
    db.write_blocking(|tx| {
        for (device, installation) in [(DEVICE, "inst"), (OTHER, "other")] {
            tx.execute(
                "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, \
                 first_seen) VALUES (?1, ?2, NULL, 0)",
                params![installation, device],
            )?;
        }
        for (device, id, path) in [
            (DEVICE, "r1", "/a"),
            (DEVICE, "r2", "/c"),
            (OTHER, "r3", "/e"),
        ] {
            let place = |path: String| SyncRulePlace {
                source: SourceRef::Device,
                path,
            };
            let draft = SyncRuleDraft {
                name: id.to_owned(),
                place_a: place(path.to_owned()),
                place_b: place(format!("{path}-b")),
                direction: SyncRuleDirection::AToB,
                delete_mode: SyncRuleDeleteMode::Keep,
                interval_secs: 900,
                enabled: true,
                allow_decrypted: false,
            };
            store::insert_rule(tx, device, id, &draft, "2026-10-29T00:00:00Z")?;
        }
        Ok(())
    })
    .unwrap();
    (dir, db)
}

fn finished(started_ms: i64, items: Vec<RunItem>) -> Finished {
    let report = RunReport {
        outcome: SyncRuleOutcome::Done,
        error: None,
        counts: Counts {
            copied: items.len() as u64,
            ..Counts::default()
        },
        failures: Vec::new(),
        skipped: Vec::new(),
        ignore_errors: Vec::new(),
        ignored: 0,
        trashed: Vec::new(),
        conflict_copies: Vec::new(),
        items,
    };
    Finished::of(SyncRuleRunCause::Manual, started_ms, 5, &report)
}

fn record_runs(db: &VaultDb, device: &'static str, rule: &'static str, n: i64) {
    db.write_blocking(move |tx| {
        for started in 0..n {
            let item = RunItem::new(SyncRuleRunAction::Copy, SyncRuleSide::B, "x.txt");
            record(tx, device, rule, &finished(started, vec![item]))?;
        }
        Ok(())
    })
    .unwrap();
}

fn runs_of(db: &VaultDb, device: &'static str, rule: &'static str) -> Vec<SyncRuleRun> {
    db.read_blocking(move |r| Ok(runs(r, device, rule)))
        .unwrap()
        .unwrap()
}

fn item_rows(db: &VaultDb) -> i64 {
    db.read_blocking(|r| {
        r.query_row(
            "SELECT COUNT(*) FROM haex_sync_run_items_no_sync",
            &[],
            |row| row.get::<_, i64>(0),
        )
    })
    .unwrap()
    .unwrap_or(0)
}

fn set(db: &VaultDb, depth: u32) {
    db.write_blocking(move |tx| {
        set_depth(tx, DEVICE, depth)?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn the_depth_is_a_whole_number_up_to_a_hundred() {
    for valid in ["0", "10", "100"] {
        assert!(is_valid_depth(valid), "{valid}");
    }
    for invalid in ["", "-1", "101", "1.5", "07", " 5", "zehn"] {
        assert!(!is_valid_depth(invalid), "{invalid}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_default_depth_keeps_the_latest_ten_runs() {
    let (_dir, db) = setup();
    assert_eq!(
        db.read_blocking(|r| Ok(depth(r, DEVICE))).unwrap().unwrap(),
        DEFAULT_LOG_DEPTH
    );
    record_runs(&db, DEVICE, "r1", 12);
    let runs = runs_of(&db, DEVICE, "r1");
    assert_eq!(runs.len(), 10);
    assert_eq!(runs[0].started_ms, 11, "the latest first");
    assert_eq!(runs[9].started_ms, 2);
    assert_eq!(item_rows(&db), 10, "the files of dropped runs go with them");
}

#[tokio::test(flavor = "multi_thread")]
async fn depth_zero_writes_nothing() {
    let (_dir, db) = setup();
    set(&db, 0);
    record_runs(&db, DEVICE, "r1", 3);
    assert!(runs_of(&db, DEVICE, "r1").is_empty());
    assert_eq!(item_rows(&db), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_smaller_depth_trims_every_rule_of_the_device_at_once() {
    let (_dir, db) = setup();
    record_runs(&db, DEVICE, "r1", 5);
    record_runs(&db, DEVICE, "r2", 4);
    record_runs(&db, OTHER, "r3", 4);
    set(&db, 2);
    assert_eq!(runs_of(&db, DEVICE, "r1").len(), 2);
    assert_eq!(runs_of(&db, DEVICE, "r2").len(), 2);
    assert_eq!(runs_of(&db, OTHER, "r3").len(), 4, "only this device");
    assert_eq!(
        db.read_blocking(|r| Ok(depth(r, DEVICE))).unwrap().unwrap(),
        2
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn every_file_action_is_kept_with_its_trash() {
    let (_dir, db) = setup();
    let mut items: Vec<RunItem> = (0..500)
        .map(|n| {
            RunItem::new(
                SyncRuleRunAction::Copy,
                SyncRuleSide::B,
                format!("f{n}.txt"),
            )
        })
        .collect();
    let mut deleted = RunItem::new(SyncRuleRunAction::Delete, SyncRuleSide::B, "weg.txt");
    deleted.trash = Some(Trashed::Rule(".holzi-trash/run/weg.txt".to_owned()));
    items.push(deleted);
    let mut system = RunItem::new(SyncRuleRunAction::Delete, SyncRuleSide::A, "fort.txt");
    system.trash = Some(Trashed::System);
    items.push(system);
    let run = finished(1, items);
    let id = run.id.clone();
    db.write_blocking(move |tx| {
        record(tx, DEVICE, "r1", &run)?;
        Ok(())
    })
    .unwrap();
    let all = {
        let id = id.clone();
        db.read_blocking(move |r| Ok(super::items(r, DEVICE, &id, 0, 1_000)))
            .unwrap()
            .unwrap()
    };
    assert_eq!(all.len(), 502, "no cap per run");
    assert_eq!(all[500].trash_kind, Some(SyncRuleTrashKind::Rule));
    assert_eq!(
        all[500].trash_path.as_deref(),
        Some(".holzi-trash/run/weg.txt")
    );
    assert_eq!(all[501].trash_kind, Some(SyncRuleTrashKind::System));
    assert_eq!(all[501].trash_path, None);
    let page = {
        let id = id.clone();
        db.read_blocking(move |r| Ok(super::items(r, DEVICE, &id, 498, 3)))
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        page.iter().map(|item| item.seq).collect::<Vec<_>>(),
        [498, 499, 500]
    );
    let foreign = db
        .read_blocking(move |r| Ok(super::items(r, OTHER, &id, 0, 10)))
        .unwrap()
        .unwrap();
    assert!(foreign.is_empty(), "the run of another device");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_log_goes_with_its_rule() {
    let (_dir, db) = setup();
    record_runs(&db, DEVICE, "r1", 3);
    db.write_blocking(|tx| {
        store::delete_rule(tx, DEVICE, "r1")?;
        Ok(())
    })
    .unwrap();
    assert!(runs_of(&db, DEVICE, "r1").is_empty());
    assert_eq!(item_rows(&db), 0);
}
