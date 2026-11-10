//! Steps without copies against places in memory: a file only at the target is noted once, "keep"
//! marks the state, a waiting deletion is stored, a failing step leaves the rest going, and a
//! cancelled run stops before the next step.

// These tests insert devices directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;

use super::*;
use crate::file_sync::model::{
    SyncRuleDeleteMode, SyncRuleDirection, SyncRuleDraft, SyncRulePlace,
};
use crate::file_sync::test_support::MemPlace;
use crate::files::SourceRef;
use crate::remote_storage::test_support::vault;

const DEVICE: &str = "11111111-1111-4111-8111-111111111111";

fn setup() -> (tempfile::TempDir, VaultDb, SyncRule) {
    let (dir, db) = vault();
    let place = |path: &str| SyncRulePlace {
        location: SourceRef::Device,
        path: path.to_owned(),
    };
    let draft = SyncRuleDraft {
        name: "T".to_owned(),
        source: place("/a"),
        target: place("/b"),
        source_kind: None,
        direction: SyncRuleDirection::SourceToTarget,
        delete_mode: SyncRuleDeleteMode::Keep,
        interval_secs: 900,
        enabled: true,
        allow_decrypted: false,
    };
    db.write_blocking(move |tx| {
        tx.execute(
            "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) \
             VALUES ('inst', ?1, NULL, 0)",
            params![DEVICE],
        )?;
        store::insert_rule(tx, DEVICE, "r1", &draft, "2026-10-27T00:00:00Z")?;
        Ok(())
    })
    .unwrap();
    let rule = db
        .read_blocking(|r| Ok(store::rule(r, DEVICE, "r1")))
        .unwrap()
        .unwrap()
        .unwrap();
    (dir, db, rule)
}

fn seen_row(path: &str) -> StateRow {
    StateRow {
        path: path.to_owned(),
        source: Some(Seen {
            size: 1,
            mtime_ms: 1,
        }),
        target: Some(Seen {
            size: 1,
            mtime_ms: 2,
        }),
        kept: false,
        hash: None,
    }
}

async fn exec(
    db: &VaultDb,
    rule: &SyncRule,
    source: &MemPlace,
    target: &MemPlace,
    state: &[StateRow],
    steps: &[Step],
    cancel: &CancellationToken,
) -> Executed {
    let (source_listing, target_listing) = (
        source.list_tree(cancel).await.unwrap(),
        target.list_tree(cancel).await.unwrap(),
    );
    execute(
        &Target {
            db,
            device: DEVICE,
            rule,
            run_id: "run1",
            source,
            target,
            source_listing: &source_listing,
            target_listing: &target_listing,
            state,
            cancel,
            progress: &Reporter::silent(),
        },
        steps,
    )
    .await
}

fn state(db: &VaultDb) -> Vec<StateRow> {
    db.read_blocking(|r| Ok(store::state(r, DEVICE, "r1")))
        .unwrap()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn report_keep_pending_and_forget_write_the_state() {
    let (_dir, db, rule) = setup();
    let (source, target) = (MemPlace::new(), MemPlace::new());
    target.put("mine.txt", b"m", 4);
    let rows = vec![seen_row("kept.txt"), seen_row("gone.txt")];
    let done = exec(
        &db,
        &rule,
        &source,
        &target,
        &rows,
        &[
            Step::Report {
                path: "mine.txt".to_owned(),
                what: Deviation::Added,
            },
            Step::Keep {
                path: "kept.txt".to_owned(),
            },
            Step::Pending {
                side: SyncRuleSide::Target,
                path: "ask.txt".to_owned(),
            },
            Step::Forget {
                path: "gone.txt".to_owned(),
            },
        ],
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(done.counts.deviations, 1);
    assert_eq!(done.counts.pending, 1);
    let rows = state(&db);
    // The state before the run was never written; only what the steps wrote is there.
    assert_eq!(
        rows.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(),
        ["kept.txt", "mine.txt"]
    );
    assert!(rows[0].kept);
    assert_eq!(rows[1].source, None);
    assert_eq!(rows[1].target.unwrap().mtime_ms, 4);
    let pending = db
        .read_blocking(|r| Ok(store::pending(r, DEVICE, "r1")))
        .unwrap()
        .unwrap();
    assert_eq!(pending[0].path, "ask.txt");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_step_is_noted_and_the_rest_runs() {
    let (_dir, db, rule) = setup();
    let (source, target) = (MemPlace::new(), MemPlace::new());
    target.put("x.txt", b"x", 1);
    target.put("y.txt", b"y", 1);
    target.fail("x.txt");
    let done = exec(
        &db,
        &rule,
        &source,
        &target,
        &[],
        &[
            Step::Delete {
                side: SyncRuleSide::Target,
                path: "x.txt".to_owned(),
            },
            Step::Delete {
                side: SyncRuleSide::Target,
                path: "y.txt".to_owned(),
            },
        ],
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(done.counts.errors, 1);
    assert_eq!(done.failures[0].0, "x.txt");
    assert_eq!(done.counts.deleted, 1);
    assert_eq!(target.paths(), [".holzi-trash/run1/y.txt", "x.txt"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conflict_copy_is_logged_when_a_later_copy_fails() {
    let (_dir, db, rule) = setup();
    let (source, target) = (MemPlace::new(), MemPlace::new());
    source.put("x.txt", b"new", 2);
    target.put("x.txt", b"old", 1);
    let aside = "x (Konflikt holzi 2026-10-27).txt";
    target.fail(aside);

    let done = exec(
        &db,
        &rule,
        &source,
        &target,
        &[],
        &[Step::Conflict {
            path: "x.txt".to_owned(),
            keep: SyncRuleSide::Source,
            aside: aside.to_owned(),
        }],
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(done.conflict_copies, [aside]);
    assert_eq!(done.items.len(), 2);
    assert_eq!(done.items[0].action, SyncRuleRunAction::ConflictCopy);
    assert_eq!(done.items[0].detail.as_deref(), Some(aside));
    assert_eq!(done.items[1].action, SyncRuleRunAction::Error);
}

#[tokio::test(flavor = "multi_thread")]
async fn merging_exclusions_records_copies_before_the_transfer_fails() {
    let (_dir, db, rule) = setup();
    let (source, target) = (MemPlace::new(), MemPlace::new());
    source.put(".haex-ignore", b"*.tmp\n", 1);
    target.put(".haex-ignore", b"*.bak\n", 1);
    let (aside_source, aside_target) = (".haex-ignore.a", ".haex-ignore.b");

    let done = exec(
        &db,
        &rule,
        &source,
        &target,
        &[],
        &[Step::MergeIgnore {
            aside_source: aside_source.to_owned(),
            aside_target: aside_target.to_owned(),
        }],
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(done.counts.errors, 1);
    assert_eq!(done.conflict_copies, [aside_source, aside_target]);
    assert_eq!(done.items.len(), 3);
    assert_eq!(done.items[0].action, SyncRuleRunAction::ConflictCopy);
    assert_eq!(done.items[1].action, SyncRuleRunAction::ConflictCopy);
    assert_eq!(done.items[2].action, SyncRuleRunAction::Error);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_run_stops_before_the_next_step() {
    let (_dir, db, rule) = setup();
    let (source, target) = (MemPlace::new(), MemPlace::new());
    target.put("x.txt", b"x", 1);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let done = exec(
        &db,
        &rule,
        &source,
        &target,
        &[],
        &[Step::Delete {
            side: SyncRuleSide::Target,
            path: "x.txt".to_owned(),
        }],
        &cancel,
    )
    .await;
    assert!(done.cancelled);
    assert_eq!(target.calls("trash"), 0);
}
