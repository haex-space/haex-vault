//! Waiting deletions against real folders (spec 061 Story 3, FR-021): "delete" moves the file into
//! the trash, "keep" leaves it and marks the state, "restore" copies it back; a file that changed
//! since the run found it is left to the next run, and a failing step keeps its row.

// These tests insert devices directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::path::PathBuf;

use haex_crdt::rusqlite::params;

use super::*;
use crate::file_sync::model::{
    SyncRuleDeleteMode, SyncRuleDirection, SyncRuleDraft, SyncRulePlace,
};
use crate::file_sync::place_device::DevicePlace;
use crate::file_sync::store::{PendingRow, Seen};
use crate::files::SourceRef;
use crate::remote_storage::test_support::vault;

const DEVICE: &str = "11111111-1111-4111-8111-111111111111";

struct World {
    _dir: tempfile::TempDir,
    _vault: tempfile::TempDir,
    db: VaultDb,
    source: PathBuf,
    target: PathBuf,
    rule: SyncRule,
}

/// A one-way rule "ask" whose source lost `x.txt`; the target still has it, and the deletion there
/// waits for the user.
async fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let (source, target) = (root.join("a"), root.join("b"));
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(target.join("sub")).unwrap();
    std::fs::write(target.join("sub/x.txt"), "inhalt").unwrap();
    let (vault_dir, db) = vault();
    let place = |path: &PathBuf| SyncRulePlace {
        location: SourceRef::Device,
        path: path.to_string_lossy().into_owned(),
    };
    let draft = SyncRuleDraft {
        name: "Test".to_owned(),
        source: place(&source),
        target: place(&target),
        source_kind: None,
        direction: SyncRuleDirection::SourceToTarget,
        delete_mode: SyncRuleDeleteMode::Ask,
        interval_secs: 900,
        enabled: true,
        allow_decrypted: false,
    };
    let facts = DevicePlace::with_rule_trash(&target)
        .stat("sub/x.txt")
        .await
        .unwrap()
        .unwrap();
    let row = StateRow {
        path: "sub/x.txt".to_owned(),
        source: Some(Seen {
            size: facts.size,
            mtime_ms: 1,
        }),
        target: Some(Seen {
            size: facts.size,
            mtime_ms: facts.mtime_ms,
        }),
        kept: false,
        hash: None,
    };
    db.write_blocking(move |tx| {
        tx.execute(
            "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) \
             VALUES ('inst', ?1, 'Laptop', 0)",
            params![DEVICE],
        )?;
        store::insert_rule(tx, DEVICE, "r1", &draft, "2026-10-27T00:00:00Z")?;
        store::put_state(tx, DEVICE, "r1", &row)?;
        store::put_pending(
            tx,
            DEVICE,
            "r1",
            &PendingRow {
                path: "sub/x.txt".to_owned(),
                side: SyncRuleSide::Target,
                found_run_id: "run0".to_owned(),
            },
        )?;
        Ok(())
    })
    .unwrap();
    let rule = db
        .read_blocking(|r| Ok(store::rule(r, DEVICE, "r1")))
        .unwrap()
        .unwrap()
        .unwrap();
    World {
        _dir: dir,
        _vault: vault_dir,
        db,
        source,
        target,
        rule,
    }
}

impl World {
    async fn resolve(&self, choice: SyncRulePendingChoice) -> Executed {
        resolve(
            &self.db,
            DEVICE,
            &self.rule,
            "run1",
            &DevicePlace::with_rule_trash(&self.source),
            &DevicePlace::with_rule_trash(&self.target),
            &["sub/x.txt".to_owned()],
            choice,
        )
        .await
        .unwrap()
    }

    async fn pending(&self) -> Vec<SyncRulePending> {
        list(&self.db, DEVICE, "r1").await.unwrap()
    }

    fn state(&self) -> Vec<StateRow> {
        self.db
            .read_blocking(|r| Ok(store::state(r, DEVICE, "r1")))
            .unwrap()
            .unwrap()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_list_names_path_and_side() {
    let w = world().await;
    assert_eq!(
        w.pending().await,
        [SyncRulePending {
            path: "sub/x.txt".to_owned(),
            side: SyncRuleSide::Target,
        }]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_moves_the_file_into_the_trash() {
    let w = world().await;
    let done = w.resolve(SyncRulePendingChoice::Delete).await;
    assert_eq!(done.counts.deleted, 1, "{done:?}");
    assert!(!w.target.join("sub/x.txt").exists());
    assert!(w.target.join(".holzi-trash/run1/sub/x.txt").is_file());
    assert!(w.pending().await.is_empty());
    assert!(w.state().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn keep_leaves_the_file_and_marks_it_kept() {
    let w = world().await;
    let done = w.resolve(SyncRulePendingChoice::Keep).await;
    assert_eq!(done.counts.kept, 1, "{done:?}");
    assert!(w.target.join("sub/x.txt").is_file());
    assert!(w.pending().await.is_empty());
    assert!(
        w.state()[0].kept,
        "not copied back, not asked again (FR-020)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn restore_copies_the_file_back() {
    let w = world().await;
    let done = w.resolve(SyncRulePendingChoice::Restore).await;
    assert_eq!(done.counts.copied, 1, "{done:?}");
    assert_eq!(
        std::fs::read_to_string(w.source.join("sub/x.txt")).unwrap(),
        "inhalt"
    );
    assert!(w.target.join("sub/x.txt").is_file());
    assert!(w.pending().await.is_empty());
    let state = w.state();
    assert!(state[0].source.is_some() && state[0].target.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_changed_since_the_run_found_it_is_not_deleted() {
    let w = world().await;
    std::fs::write(w.target.join("sub/x.txt"), "inzwischen geändert").unwrap();
    let done = w.resolve(SyncRulePendingChoice::Delete).await;
    assert_eq!(done.counts.deleted, 0);
    assert!(w.target.join("sub/x.txt").is_file());
    assert!(
        w.pending().await.is_empty(),
        "the next run looks at the file again"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_back_on_the_other_side_is_not_deleted() {
    let w = world().await;
    std::fs::create_dir_all(w.source.join("sub")).unwrap();
    std::fs::write(w.source.join("sub/x.txt"), "zurück").unwrap();
    let done = w.resolve(SyncRulePendingChoice::Delete).await;
    assert_eq!(done.counts.deleted, 0);
    assert!(w.target.join("sub/x.txt").is_file());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_delete_keeps_the_file_and_its_row() {
    let w = world().await;
    // A file where the rule trash would be: the move into it fails.
    std::fs::write(w.target.join(".holzi-trash"), "im Weg").unwrap();
    let done = w.resolve(SyncRulePendingChoice::Delete).await;
    assert_eq!(done.failures.len(), 1, "{done:?}");
    assert!(w.target.join("sub/x.txt").is_file());
    assert_eq!(w.pending().await.len(), 1);
}
