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
        source: SourceRef::Device,
        path: path.to_owned(),
    };
    let draft = SyncRuleDraft {
        name: "T".to_owned(),
        place_a: place("/a"),
        place_b: place("/b"),
        direction: SyncRuleDirection::AToB,
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
        a: Some(Seen {
            size: 1,
            mtime_ms: 1,
        }),
        b: Some(Seen {
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
    a: &MemPlace,
    b: &MemPlace,
    state: &[StateRow],
    steps: &[Step],
    cancel: &CancellationToken,
) -> Executed {
    let (la, lb) = (
        a.list_tree(cancel).await.unwrap(),
        b.list_tree(cancel).await.unwrap(),
    );
    execute(
        &Target {
            db,
            device: DEVICE,
            rule,
            run_id: "run1",
            a,
            b,
            listing_a: &la,
            listing_b: &lb,
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
    let (a, b) = (MemPlace::new(), MemPlace::new());
    b.put("mine.txt", b"m", 4);
    let rows = vec![seen_row("kept.txt"), seen_row("gone.txt")];
    let done = exec(
        &db,
        &rule,
        &a,
        &b,
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
                side: SyncRuleSide::B,
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
    assert_eq!(rows[1].a, None);
    assert_eq!(rows[1].b.unwrap().mtime_ms, 4);
    let pending = db
        .read_blocking(|r| Ok(store::pending(r, DEVICE, "r1")))
        .unwrap()
        .unwrap();
    assert_eq!(pending[0].path, "ask.txt");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_step_is_noted_and_the_rest_runs() {
    let (_dir, db, rule) = setup();
    let (a, b) = (MemPlace::new(), MemPlace::new());
    b.put("x.txt", b"x", 1);
    b.put("y.txt", b"y", 1);
    b.fail("x.txt");
    let done = exec(
        &db,
        &rule,
        &a,
        &b,
        &[],
        &[
            Step::Delete {
                side: SyncRuleSide::B,
                path: "x.txt".to_owned(),
            },
            Step::Delete {
                side: SyncRuleSide::B,
                path: "y.txt".to_owned(),
            },
        ],
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(done.counts.errors, 1);
    assert_eq!(done.failures[0].0, "x.txt");
    assert_eq!(done.counts.deleted, 1);
    assert_eq!(b.paths(), [".holzi-trash/run1/y.txt", "x.txt"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conflict_copy_is_reported_when_a_later_copy_fails() {
    let (_dir, db, rule) = setup();
    let (a, b) = (MemPlace::new(), MemPlace::new());
    a.put("x.txt", b"new", 2);
    b.put("x.txt", b"old", 1);
    a.fail("x.txt");
    let aside = "x (Konflikt Laptop 2026-10-29).txt";

    let done = exec(
        &db,
        &rule,
        &a,
        &b,
        &[],
        &[Step::Conflict {
            path: "x.txt".to_owned(),
            keep: SyncRuleSide::A,
            aside: aside.to_owned(),
        }],
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(done.counts.errors, 1);
    assert_eq!(done.conflict_copies, [aside]);
    assert_eq!(b.paths(), [aside]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_run_stops_before_the_next_step() {
    let (_dir, db, rule) = setup();
    let (a, b) = (MemPlace::new(), MemPlace::new());
    b.put("x.txt", b"x", 1);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let done = exec(
        &db,
        &rule,
        &a,
        &b,
        &[],
        &[Step::Delete {
            side: SyncRuleSide::B,
            path: "x.txt".to_owned(),
        }],
        &cancel,
    )
    .await;
    assert!(done.cancelled);
    assert_eq!(b.calls("trash"), 0);
}
