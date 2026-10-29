//! Runs of a one-way rule between two real folders (spec 061 Story 1, SC-002, SC-004, SC-005):
//! first and second run, the target's measured time in the state, an abort mid-run, a failing
//! file, links, the delete modes, changes at the target, a known place that lists empty, a missing
//! or unreadable root, first meetings and the rule trash; and runs of a two-way rule (Story 2): both directions, conflict
//! copies on both sides, deleted against changed, moves, merged exclusion files, and a waiting
//! deletion that goes when its file comes back.

// These tests insert devices directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use haex_crdt::rusqlite::params;

use super::*;
use crate::file_sync::model::{
    SyncRuleDeleteMode, SyncRuleDirection, SyncRuleDraft, SyncRulePlace,
};
use crate::file_sync::place::{Facts, Listing, Trashed};
use crate::file_sync::place_device::DevicePlace;
use crate::file_sync::progress::SyncRulePhase;
use crate::file_sync::test_support::MemPlace;
use crate::files::transfer::remote::Side;
use crate::files::{FilesError, SourceRef};
use crate::remote_storage::test_support::vault;

const DEVICE: &str = "11111111-1111-4111-8111-111111111111";

struct World {
    _dir: tempfile::TempDir,
    _vault: tempfile::TempDir,
    db: VaultDb,
    a: PathBuf,
    b: PathBuf,
    rule: SyncRule,
}

fn world(mode: SyncRuleDeleteMode) -> World {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let (a, b) = (root.join("a"), root.join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    let (vault_dir, db) = vault();
    let draft = SyncRuleDraft {
        name: "Test".to_owned(),
        place_a: SyncRulePlace {
            source: SourceRef::Device,
            path: a.to_string_lossy().into_owned(),
        },
        place_b: SyncRulePlace {
            source: SourceRef::Device,
            path: b.to_string_lossy().into_owned(),
        },
        direction: SyncRuleDirection::AToB,
        delete_mode: mode,
        interval_secs: 900,
        enabled: true,
        allow_decrypted: false,
    };
    let stored = draft.clone();
    db.write_blocking(move |tx| {
        tx.execute(
            "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) \
             VALUES ('inst', ?1, 'Laptop', 0)",
            params![DEVICE],
        )?;
        store::insert_rule(tx, DEVICE, "r1", &stored, "2026-10-27T00:00:00Z")?;
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
        a,
        b,
        rule,
    }
}

impl World {
    async fn run_with(
        &self,
        a: &dyn Place,
        b: &dyn Place,
        cancel: &CancellationToken,
    ) -> RunReport {
        self.run_reporting(a, b, cancel, &Reporter::silent()).await
    }

    async fn run_reporting(
        &self,
        a: &dyn Place,
        b: &dyn Place,
        cancel: &CancellationToken,
        progress: &Reporter,
    ) -> RunReport {
        let now = || "2026-10-27T12:00:00Z".to_owned();
        run(
            &RunContext {
                db: &self.db,
                device: DEVICE,
                device_name: "Laptop",
                today: "2026-10-27",
                now: &now,
                run_id: "run1",
                cancel,
                progress,
            },
            &self.rule,
            a,
            b,
        )
        .await
    }

    async fn run(&self) -> RunReport {
        self.run_with(
            &DevicePlace::with_rule_trash(&self.a),
            &DevicePlace::with_rule_trash(&self.b),
            &CancellationToken::new(),
        )
        .await
    }

    fn state(&self) -> Vec<StateRow> {
        self.db
            .read_blocking(|r| Ok(store::state(r, DEVICE, "r1")))
            .unwrap()
            .unwrap()
    }
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = walkdir::WalkDir::new(dir)
        .min_depth(1)
        .into_iter()
        .map(|e| e.unwrap())
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            e.path()
                .strip_prefix(dir)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_run_copies_everything_and_the_second_nothing() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.a.join("x.txt"), "x");
    write(&w.a.join("Fotos/2026/y.jpg"), "yy");
    std::fs::create_dir_all(w.a.join("leer")).unwrap();
    let first = w.run().await;
    assert_eq!(first.outcome, SyncRuleOutcome::Done, "{first:?}");
    assert_eq!(first.counts.copied, 2);
    assert_eq!(names(&w.b), ["Fotos/2026/y.jpg", "x.txt"]);
    assert!(w.b.join("leer").is_dir(), "empty folders come too");
    let second = w.run().await;
    assert_eq!(second.outcome, SyncRuleOutcome::Done);
    assert_eq!(second.counts.copied, 0);
    let rule =
        w.db.read_blocking(|r| Ok(store::rule(r, DEVICE, "r1")))
            .unwrap()
            .unwrap()
            .unwrap();
    assert_eq!(rule.last_outcome, Some(SyncRuleOutcome::Done));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_reports_its_phases_and_the_copied_files() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.a.join("x.txt"), "abc");
    write(&w.a.join("Fotos/y.jpg"), "de");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let reporter = Reporter::new("r1", move |progress| {
        sink.lock().unwrap().push(progress.clone());
    });
    let report = w
        .run_reporting(
            &DevicePlace::with_rule_trash(&w.a),
            &DevicePlace::with_rule_trash(&w.b),
            &CancellationToken::new(),
            &reporter,
        )
        .await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].phase, SyncRulePhase::Scanning);
    let planned = seen
        .iter()
        .find(|progress| progress.phase == SyncRulePhase::Copying)
        .unwrap();
    assert_eq!((planned.files_total, planned.bytes_total), (2, 5));
    let last = seen.last().unwrap();
    assert_eq!(
        (last.files_done, last.bytes_done),
        (2, 5),
        "the last report is the finished copy"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_state_holds_the_targets_measured_time_and_the_hash() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.a.join("x.txt"), "abc");
    w.run().await;
    let row = w.state().pop().unwrap();
    let target = DevicePlace::with_rule_trash(&w.b)
        .stat("x.txt")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.b.unwrap().mtime_ms, target.mtime_ms);
    assert_eq!(
        row.hash.as_deref(),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_file_arrives_and_a_deleted_one_stays_with_keep() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.a.join("x.txt"), "1");
    write(&w.a.join("y.txt"), "1");
    w.run().await;
    write(&w.a.join("x.txt"), "zwei");
    std::fs::remove_file(w.a.join("y.txt")).unwrap();
    let report = w.run().await;
    assert_eq!(report.counts.copied, 1);
    assert_eq!(report.counts.kept, 1);
    assert_eq!(std::fs::read_to_string(w.b.join("x.txt")).unwrap(), "zwei");
    assert!(w.b.join("y.txt").exists(), "keep leaves it (FR-020)");
    let again = w.run().await;
    assert_eq!(
        again.counts.copied + again.counts.kept,
        0,
        "and never again"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn mirror_moves_a_deleted_file_into_the_rule_trash() {
    let w = world(SyncRuleDeleteMode::Mirror);
    write(&w.a.join("x.txt"), "1");
    write(&w.a.join("keep.txt"), "1");
    w.run().await;
    std::fs::remove_file(w.a.join("x.txt")).unwrap();
    let report = w.run().await;
    assert_eq!(report.counts.deleted, 1);
    assert_eq!(
        report.trashed[0].to,
        Trashed::Rule(".holzi-trash/run1/x.txt".to_owned())
    );
    assert!(w.b.join(".holzi-trash/run1/x.txt").is_file());
    assert!(!w.b.join("x.txt").exists());
    assert_eq!(w.run().await.counts.copied, 0, "the trash is not synced");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_at_the_target_is_reported_and_kept() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.a.join("x.txt"), "1");
    w.run().await;
    write(&w.b.join("x.txt"), "von Hand");
    let report = w.run().await;
    assert_eq!(report.counts.deviations, 1);
    assert_eq!(
        std::fs::read_to_string(w.b.join("x.txt")).unwrap(),
        "von Hand"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_first_meeting_adopts_equal_files_and_keeps_both_of_different_ones() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.a.join("same.txt"), "gleich");
    write(&w.b.join("same.txt"), "gleich");
    write(&w.a.join("other.txt"), "AAAA");
    write(&w.b.join("other.txt"), "BBBB");
    let report = w.run().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    assert_eq!(report.counts.adopted, 1);
    assert_eq!(report.counts.copied, 1);
    assert_eq!(report.counts.conflicts, 1);
    assert_eq!(
        std::fs::read_to_string(w.b.join("other.txt")).unwrap(),
        "AAAA"
    );
    assert_eq!(
        std::fs::read_to_string(w.b.join("other (Konflikt Laptop 2026-10-27).txt")).unwrap(),
        "BBBB",
        "nothing is lost (FR-012a)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_moved_file_is_moved_at_the_target_without_copying() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.a.join("alt/x.txt"), "inhalt");
    w.run().await;
    std::fs::create_dir_all(w.a.join("neu")).unwrap();
    std::fs::rename(w.a.join("alt/x.txt"), w.a.join("neu/x.txt")).unwrap();
    let report = w.run().await;
    assert_eq!(report.counts.moved, 1, "{report:?}");
    assert_eq!(report.counts.copied, 0);
    assert!(w.b.join("neu/x.txt").is_file());
    assert!(!w.b.join("alt/x.txt").exists());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn links_are_skipped_and_named() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.a.join("x.txt"), "x");
    std::os::unix::fs::symlink(w.a.join("x.txt"), w.a.join("link")).unwrap();
    let report = w.run().await;
    assert_eq!(report.skipped, [("link".to_owned(), SkipReason::Link)]);
    assert_eq!(names(&w.b), ["x.txt"]);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_failing_file_does_not_stop_the_run_and_comes_again() {
    use std::os::unix::fs::PermissionsExt;
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.a.join("good.txt"), "g");
    write(&w.a.join("locked.txt"), "l");
    std::fs::set_permissions(
        w.a.join("locked.txt"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    if std::fs::read(w.a.join("locked.txt")).is_ok() {
        return; // root ignores permissions
    }
    let report = w.run().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Partial);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].0, "locked.txt");
    assert!(w.b.join("good.txt").exists());
    std::fs::set_permissions(
        w.a.join("locked.txt"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let again = w.run().await;
    assert_eq!(again.outcome, SyncRuleOutcome::Done);
    assert!(w.b.join("locked.txt").exists(), "tried again (FR-023)");
}

/// A device place that cancels the run after `after` stats of copied files.
struct Tripwire {
    inner: DevicePlace,
    cancel: CancellationToken,
    stats: AtomicUsize,
    after: usize,
    fail_ignore_write: bool,
}

#[async_trait]
impl Place for Tripwire {
    async fn list_tree(&self, cancel: &CancellationToken) -> Result<Listing, FilesError> {
        self.inner.list_tree(cancel).await
    }
    async fn stat(&self, path: &str) -> Result<Option<Facts>, FilesError> {
        if self.stats.fetch_add(1, Ordering::SeqCst) + 1 == self.after {
            self.cancel.cancel();
        }
        self.inner.stat(path).await
    }
    async fn hash(&self, path: &str, cancel: &CancellationToken) -> Result<String, FilesError> {
        self.inner.hash(path, cancel).await
    }
    async fn create_folder(&self, path: &str) -> Result<(), FilesError> {
        self.inner.create_folder(path).await
    }
    async fn rename(&self, from: &str, to: &str) -> Result<(), FilesError> {
        self.inner.rename(from, to).await
    }
    async fn delete(&self, path: &str) -> Result<(), FilesError> {
        self.inner.delete(path).await
    }
    async fn trash(&self, path: &str, run: &str) -> Result<Trashed, FilesError> {
        self.inner.trash(path, run).await
    }
    async fn read_text(&self, path: &str, max: usize) -> Result<Option<String>, FilesError> {
        self.inner.read_text(path, max).await
    }
    async fn write_text(&self, path: &str, text: &str) -> Result<(), FilesError> {
        if self.fail_ignore_write && path == ".haex-ignore" {
            return Err(FilesError {
                code: crate::files::FilesErrorCode::NoAccess,
                message: "test write failure".to_owned(),
            });
        }
        self.inner.write_text(path, text).await
    }
    fn side(&self) -> Side {
        self.inner.side()
    }
    fn full_path(&self, path: &str) -> String {
        self.inner.full_path(path)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_abort_keeps_the_state_of_finished_files_and_leaves_no_half_file() {
    let w = world(SyncRuleDeleteMode::Keep);
    for n in 1..=5 {
        write(&w.a.join(format!("f{n}.txt")), &"x".repeat(n));
    }
    let cancel = CancellationToken::new();
    let source = Tripwire {
        inner: DevicePlace::with_rule_trash(&w.a),
        cancel: cancel.clone(),
        stats: AtomicUsize::new(0),
        after: 3,
        fail_ignore_write: false,
    };
    let report = w
        .run_with(&source, &DevicePlace::with_rule_trash(&w.b), &cancel)
        .await;
    assert_eq!(report.outcome, SyncRuleOutcome::Cancelled);
    assert_eq!(
        w.state().len(),
        3,
        "only finished files are in the state (FR-015)"
    );
    assert_eq!(names(&w.b).len(), 3, "no half file (FR-014)");
    let again = w.run().await;
    assert_eq!(again.counts.copied, 2, "the rest, nothing twice (SC-005)");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_known_place_that_lists_empty_waits_and_changes_nothing() {
    let w = world(SyncRuleDeleteMode::Mirror);
    let (a, b) = (MemPlace::new(), MemPlace::new());
    a.put("x.txt", b"x", 1);
    let row = StateRow {
        path: "x.txt".to_owned(),
        a: Some(store::Seen {
            size: 1,
            mtime_ms: 1,
        }),
        b: Some(store::Seen {
            size: 1,
            mtime_ms: 2,
        }),
        kept: false,
        hash: None,
    };
    w.db.write_blocking(move |tx| {
        store::put_state(tx, DEVICE, "r1", &row)?;
        Ok(())
    })
    .unwrap();
    let report = w.run_with(&a, &b, &CancellationToken::new()).await;
    assert_eq!(report.outcome, SyncRuleOutcome::Waiting, "{report:?}");
    assert_eq!(b.calls("trash") + b.calls("delete") + b.calls("rename"), 0);
    let gone_source = MemPlace::new();
    b.put("x.txt", b"x", 2);
    let report = w
        .run_with(&gone_source, &b, &CancellationToken::new())
        .await;
    assert_eq!(
        report.outcome,
        SyncRuleOutcome::Waiting,
        "an emptied source never mirrors"
    );
    assert_eq!(b.calls("trash"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_or_unreadable_root_stops_before_any_change() {
    let w = world(SyncRuleDeleteMode::Mirror);
    write(&w.a.join("x.txt"), "x");
    w.run().await;
    let moved = w.a.with_file_name("a-weg");
    std::fs::rename(&w.a, &moved).unwrap();
    let report = w.run().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Waiting, "{report:?}");
    assert_eq!(names(&w.b), ["x.txt"], "nothing deleted (SC-004)");
    std::fs::rename(&moved, &w.a).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&w.a, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root ignores permissions; nothing to check then.
        let readable = std::fs::read_dir(&w.a).is_ok();
        let report = w.run().await;
        std::fs::set_permissions(&w.a, std::fs::Permissions::from_mode(0o755)).unwrap();
        if !readable {
            assert!(
                matches!(
                    report.outcome,
                    SyncRuleOutcome::Waiting | SyncRuleOutcome::Error
                ),
                "{report:?}"
            );
            assert_eq!(names(&w.b), ["x.txt"]);
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_place_waits() {
    let w = world(SyncRuleDeleteMode::Keep);
    let (a, b) = (MemPlace::new(), MemPlace::new());
    b.set_unreachable(true);
    let report = w.run_with(&a, &b, &CancellationToken::new()).await;
    assert_eq!(report.outcome, SyncRuleOutcome::Waiting);
}

fn both(mode: SyncRuleDeleteMode) -> World {
    let mut w = world(mode);
    w.rule.direction = SyncRuleDirection::Both;
    w
}

/// Writes `content` with a modification time `secs` after 1970.
fn write_at(path: &Path, content: &str, secs: i64) {
    write(path, content);
    filetime::set_file_mtime(path, filetime::FileTime::from_unix_time(secs, 0)).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn two_way_new_and_changed_files_reach_both_sides() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.a.join("x.txt"), "x");
    write(&w.b.join("Fotos/y.txt"), "y");
    let first = w.run().await;
    assert_eq!(first.outcome, SyncRuleOutcome::Done, "{first:?}");
    assert_eq!(names(&w.a), ["Fotos/y.txt", "x.txt"]);
    assert_eq!(names(&w.b), ["Fotos/y.txt", "x.txt"]);
    write(&w.b.join("Fotos/y.txt"), "geändert");
    let second = w.run().await;
    assert_eq!(second.counts.copied, 1, "{second:?}");
    assert_eq!(read(&w.a.join("Fotos/y.txt")), "geändert");
    assert_eq!(w.run().await.counts.copied, 0, "and then nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_way_both_changed_the_newer_keeps_the_name_and_both_sides_get_both() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.a.join("x.txt"), "1");
    w.run().await;
    write_at(&w.a.join("x.txt"), "älter", 1_700_000_000);
    write_at(&w.b.join("x.txt"), "neuer!", 1_700_000_100);
    let report = w.run().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    assert_eq!(report.counts.conflicts, 1);
    let aside = "x (Konflikt Laptop 2026-10-27).txt";
    assert_eq!(report.conflict_copies, [aside]);
    for side in [&w.a, &w.b] {
        assert_eq!(names(side), [aside, "x.txt"]);
        assert_eq!(read(&side.join("x.txt")), "neuer!");
        assert_eq!(read(&side.join(aside)), "älter");
    }
    let again = w.run().await;
    assert_eq!(again.counts.copied + again.counts.conflicts, 0, "{again:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_way_a_deleted_file_changed_on_the_other_side_comes_back() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.a.join("x.txt"), "1");
    // A place that lists empty would make the rule wait (FR-022).
    write(&w.a.join("bleibt.txt"), "1");
    w.run().await;
    std::fs::remove_file(w.a.join("x.txt")).unwrap();
    write(&w.b.join("x.txt"), "geändert");
    let report = w.run().await;
    assert_eq!(report.counts.deleted, 0, "{report:?}");
    assert_eq!(read(&w.a.join("x.txt")), "geändert", "FR-013");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_way_a_deletion_is_mirrored_into_the_trash_of_the_other_side() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.a.join("x.txt"), "1");
    write(&w.a.join("bleibt.txt"), "1");
    w.run().await;
    std::fs::remove_file(w.b.join("x.txt")).unwrap();
    let report = w.run().await;
    assert_eq!(report.counts.deleted, 1, "{report:?}");
    assert!(w.a.join(".holzi-trash/run1/x.txt").is_file());
    assert_eq!(names(&w.b), ["bleibt.txt"]);
    let again = w.run().await;
    assert_eq!(again.counts.copied, 0, "the rule trash is never synced");
    assert_eq!(names(&w.b), ["bleibt.txt"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_waiting_deletion_goes_when_the_file_comes_back() {
    let w = both(SyncRuleDeleteMode::Ask);
    write(&w.a.join("x.txt"), "1");
    write(&w.a.join("bleibt.txt"), "1");
    w.run().await;
    std::fs::remove_file(w.a.join("x.txt")).unwrap();
    let report = w.run().await;
    assert_eq!(report.counts.pending, 1, "{report:?}");
    write_at(&w.a.join("x.txt"), "wieder da", 1_800_000_000);
    w.run().await;
    let pending =
        w.db.read_blocking(|r| Ok(store::pending(r, DEVICE, "r1")))
            .unwrap()
            .unwrap();
    assert!(pending.is_empty(), "{pending:?}");
    assert_eq!(read(&w.b.join("x.txt")), "wieder da");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_way_a_file_moved_on_the_other_side_moves_here() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.a.join("alt/x.txt"), "inhalt");
    w.run().await;
    std::fs::create_dir_all(w.b.join("neu")).unwrap();
    std::fs::rename(w.b.join("alt/x.txt"), w.b.join("neu/x.txt")).unwrap();
    let report = w.run().await;
    assert_eq!(report.counts.moved, 1, "{report:?}");
    assert_eq!(report.counts.copied, 0);
    assert_eq!(names(&w.a), ["neu/x.txt"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_way_a_first_run_adopts_equal_files_and_keeps_both_of_different_ones() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.a.join("same.txt"), "gleich");
    write(&w.b.join("same.txt"), "gleich");
    write_at(&w.a.join("other.txt"), "AAAA", 1_700_000_000);
    write_at(&w.b.join("other.txt"), "BBBB", 1_700_000_100);
    let report = w.run().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    assert_eq!(report.counts.adopted, 1);
    assert_eq!(report.counts.conflicts, 1);
    for side in [&w.a, &w.b] {
        assert_eq!(read(&side.join("other.txt")), "BBBB");
        assert_eq!(
            read(&side.join("other (Konflikt Laptop 2026-10-27).txt")),
            "AAAA"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_way_differing_exclusion_files_keep_both_and_become_their_union() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.a.join(".haex-ignore"), "*.tmp\n");
    write(&w.b.join(".haex-ignore"), "*.log\n");
    write(&w.a.join("x.tmp"), "a");
    write(&w.b.join("y.log"), "b");
    let report = w.run().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    let (aside_a, aside_b) = (
        ".haex-ignore (Konflikt Laptop 2026-10-27)",
        ".haex-ignore (Konflikt Laptop 2026-10-27 2)",
    );
    for side in [&w.a, &w.b] {
        assert_eq!(read(&side.join(".haex-ignore")), "*.tmp\n*.log\n");
        assert_eq!(read(&side.join(aside_a)), "*.tmp\n");
        assert_eq!(read(&side.join(aside_b)), "*.log\n");
    }
    assert!(!w.b.join("x.tmp").exists() && !w.a.join("y.log").exists());
    let again = w.run().await;
    assert_eq!(again.counts.copied + again.counts.conflicts, 0, "{again:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_exclusion_union_write_keeps_both_original_files() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.a.join(".haex-ignore"), "*.tmp\n");
    write(&w.b.join(".haex-ignore"), "*.log\n");
    let source = Tripwire {
        inner: DevicePlace::with_rule_trash(&w.a),
        cancel: CancellationToken::new(),
        stats: AtomicUsize::new(0),
        after: usize::MAX,
        fail_ignore_write: true,
    };

    let report = w
        .run_with(
            &source,
            &DevicePlace::with_rule_trash(&w.b),
            &CancellationToken::new(),
        )
        .await;

    assert_eq!(report.outcome, SyncRuleOutcome::Partial, "{report:?}");
    assert_eq!(
        report.conflict_copies,
        [
            ".haex-ignore (Konflikt Laptop 2026-10-27)",
            ".haex-ignore (Konflikt Laptop 2026-10-27 2)"
        ]
    );
    assert_eq!(read(&w.a.join(".haex-ignore")), "*.tmp\n");
    assert_eq!(read(&w.b.join(".haex-ignore")), "*.log\n");
}
