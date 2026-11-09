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
use crate::file_sync::execute::FAILURES_IN_A_ROW;
use crate::file_sync::model::{
    SyncRuleDeleteMode, SyncRuleDirection, SyncRuleDraft, SyncRulePlace,
};
use crate::file_sync::place::{Facts, Listing, Trashed};
use crate::file_sync::place_device::DevicePlace;
use crate::file_sync::place_storage::StoragePlace;
use crate::file_sync::progress::SyncRulePhase;
use crate::file_sync::test_support::MemPlace;
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::remote::Side;
use crate::files::{FilesError, FilesErrorCode, SourceRef};
use crate::remote_storage::test_support::{access, vault, FakeStore, Op};

const DEVICE: &str = "11111111-1111-4111-8111-111111111111";

struct World {
    _dir: tempfile::TempDir,
    _vault: tempfile::TempDir,
    db: VaultDb,
    source: PathBuf,
    target: PathBuf,
    rule: SyncRule,
}

fn world(mode: SyncRuleDeleteMode) -> World {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let (source, target) = (root.join("a"), root.join("b"));
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    let (vault_dir, db) = vault();
    let draft = SyncRuleDraft {
        name: "Test".to_owned(),
        source: SyncRulePlace {
            location: SourceRef::Device,
            path: source.to_string_lossy().into_owned(),
        },
        target: SyncRulePlace {
            location: SourceRef::Device,
            path: target.to_string_lossy().into_owned(),
        },
        source_kind: None,
        direction: SyncRuleDirection::SourceToTarget,
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
        source,
        target,
        rule,
    }
}

impl World {
    async fn run_with(
        &self,
        source: &dyn Place,
        target: &dyn Place,
        cancel: &CancellationToken,
    ) -> RunReport {
        self.run_reporting(source, target, cancel, &Reporter::silent())
            .await
    }

    async fn run_reporting(
        &self,
        source: &dyn Place,
        target: &dyn Place,
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
                cause: run_log::SyncRuleRunCause::Manual,
                cancel,
                progress,
            },
            &self.rule,
            source,
            target,
            Started::now(),
        )
        .await
    }

    async fn run(&self) -> RunReport {
        self.run_with(
            &DevicePlace::with_rule_trash(&self.source),
            &DevicePlace::with_rule_trash(&self.target),
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
    write(&w.source.join("x.txt"), "x");
    write(&w.source.join("Fotos/2026/y.jpg"), "yy");
    std::fs::create_dir_all(w.source.join("leer")).unwrap();
    let first = w.run().await;
    assert_eq!(first.outcome, SyncRuleOutcome::Done, "{first:?}");
    assert_eq!(first.counts.copied, 2);
    assert_eq!(names(&w.target), ["Fotos/2026/y.jpg", "x.txt"]);
    assert!(w.target.join("leer").is_dir(), "empty folders come too");
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
    write(&w.source.join("x.txt"), "abc");
    write(&w.source.join("Fotos/y.jpg"), "de");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let reporter = Reporter::new("r1", move |progress| {
        sink.lock().unwrap().push(progress.clone());
    });
    let report = w
        .run_reporting(
            &DevicePlace::with_rule_trash(&w.source),
            &DevicePlace::with_rule_trash(&w.target),
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
    write(&w.source.join("x.txt"), "abc");
    w.run().await;
    let row = w.state().pop().unwrap();
    let target = DevicePlace::with_rule_trash(&w.target)
        .stat("x.txt")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.target.unwrap().mtime_ms, target.mtime_ms);
    assert_eq!(
        row.hash.as_deref(),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_file_arrives_and_a_deleted_one_stays_with_keep() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.source.join("x.txt"), "1");
    write(&w.source.join("y.txt"), "1");
    w.run().await;
    write(&w.source.join("x.txt"), "zwei");
    std::fs::remove_file(w.source.join("y.txt")).unwrap();
    let report = w.run().await;
    assert_eq!(report.counts.copied, 1);
    assert_eq!(report.counts.kept, 1);
    assert_eq!(
        std::fs::read_to_string(w.target.join("x.txt")).unwrap(),
        "zwei"
    );
    assert!(w.target.join("y.txt").exists(), "keep leaves it (FR-020)");
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
    write(&w.source.join("x.txt"), "1");
    write(&w.source.join("keep.txt"), "1");
    w.run().await;
    std::fs::remove_file(w.source.join("x.txt")).unwrap();
    let report = w.run().await;
    assert_eq!(report.counts.deleted, 1);
    assert_eq!(
        report.trashed[0].to,
        Trashed::Rule(".holzi-trash/run1/x.txt".to_owned())
    );
    assert!(w.target.join(".holzi-trash/run1/x.txt").is_file());
    assert!(!w.target.join("x.txt").exists());
    assert_eq!(w.run().await.counts.copied, 0, "the trash is not synced");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_at_the_target_is_reported_and_kept() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.source.join("x.txt"), "1");
    w.run().await;
    write(&w.target.join("x.txt"), "von Hand");
    let report = w.run().await;
    assert_eq!(report.counts.deviations, 1);
    assert_eq!(
        std::fs::read_to_string(w.target.join("x.txt")).unwrap(),
        "von Hand"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_first_meeting_adopts_equal_files_and_keeps_both_of_different_ones() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.source.join("same.txt"), "gleich");
    write(&w.target.join("same.txt"), "gleich");
    write(&w.source.join("other.txt"), "AAAA");
    write(&w.target.join("other.txt"), "BBBB");
    let report = w.run().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    assert_eq!(report.counts.adopted, 1);
    assert_eq!(report.counts.copied, 1);
    assert_eq!(report.counts.conflicts, 1);
    assert_eq!(
        std::fs::read_to_string(w.target.join("other.txt")).unwrap(),
        "AAAA"
    );
    assert_eq!(
        std::fs::read_to_string(w.target.join("other (Konflikt Laptop 2026-10-27).txt")).unwrap(),
        "BBBB",
        "nothing is lost (FR-012a)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_moved_file_is_moved_at_the_target_without_copying() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.source.join("alt/x.txt"), "inhalt");
    w.run().await;
    std::fs::create_dir_all(w.source.join("neu")).unwrap();
    std::fs::rename(w.source.join("alt/x.txt"), w.source.join("neu/x.txt")).unwrap();
    let report = w.run().await;
    assert_eq!(report.counts.moved, 1, "{report:?}");
    assert_eq!(report.counts.copied, 0);
    assert!(w.target.join("neu/x.txt").is_file());
    assert!(!w.target.join("alt/x.txt").exists());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn links_are_skipped_and_named() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.source.join("x.txt"), "x");
    std::os::unix::fs::symlink(w.source.join("x.txt"), w.source.join("link")).unwrap();
    let report = w.run().await;
    assert_eq!(report.skipped, [("link".to_owned(), SkipReason::Link)]);
    assert_eq!(names(&w.target), ["x.txt"]);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_failing_file_does_not_stop_the_run_and_comes_again() {
    use std::os::unix::fs::PermissionsExt;
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.source.join("good.txt"), "g");
    write(&w.source.join("locked.txt"), "l");
    std::fs::set_permissions(
        w.source.join("locked.txt"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    if std::fs::read(w.source.join("locked.txt")).is_ok() {
        return; // root ignores permissions
    }
    let report = w.run().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Partial);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].0, "locked.txt");
    assert!(w.target.join("good.txt").exists());
    std::fs::set_permissions(
        w.source.join("locked.txt"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let again = w.run().await;
    assert_eq!(again.outcome, SyncRuleOutcome::Done);
    assert!(w.target.join("locked.txt").exists(), "tried again (FR-023)");
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

/// A device place whose files cannot be checked after a copy: the place answers every file the same
/// way, as a storage does that limits or refuses an account.
struct Refusing {
    inner: DevicePlace,
    code: FilesErrorCode,
    asked: AtomicUsize,
}

#[async_trait]
impl Place for Refusing {
    async fn list_tree(&self, cancel: &CancellationToken) -> Result<Listing, FilesError> {
        self.inner.list_tree(cancel).await
    }
    async fn stat(&self, _path: &str) -> Result<Option<Facts>, FilesError> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Err(FilesError::new(self.code, "the place refuses"))
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
        self.inner.write_text(path, text).await
    }
    fn side(&self) -> Side {
        self.inner.side()
    }
    fn full_path(&self, path: &str) -> String {
        self.inner.full_path(path)
    }
}

async fn run_against_refusing(code: FilesErrorCode, files: usize) -> (RunReport, usize) {
    let w = world(SyncRuleDeleteMode::Keep);
    for n in 0..files {
        write(&w.source.join(format!("f{n:03}.txt")), "x");
    }
    let target = Refusing {
        inner: DevicePlace::with_rule_trash(&w.target),
        code,
        asked: AtomicUsize::new(0),
    };
    let report = w
        .run_with(
            &DevicePlace::with_rule_trash(&w.source),
            &target,
            &CancellationToken::new(),
        )
        .await;
    (report, target.asked.load(Ordering::SeqCst))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_place_that_refuses_the_credentials_stops_the_run_at_the_first_file() {
    let (report, asked) = run_against_refusing(FilesErrorCode::Credentials, 30).await;
    assert_eq!(report.outcome, SyncRuleOutcome::Error, "{report:?}");
    assert_eq!(report.counts.errors, 1);
    assert_eq!(asked, 1, "the other files are not asked");
    assert!(report.error.unwrap().contains("stopped after 1"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_place_that_does_not_answer_makes_the_rule_wait() {
    let (report, asked) = run_against_refusing(FilesErrorCode::Unreachable, 30).await;
    assert_eq!(report.outcome, SyncRuleOutcome::Waiting, "{report:?}");
    assert_eq!(asked, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn files_that_fail_ten_times_in_a_row_stop_the_run() {
    let (report, asked) = run_against_refusing(FilesErrorCode::NoAccess, 30).await;
    assert_eq!(report.outcome, SyncRuleOutcome::Error, "{report:?}");
    assert_eq!(asked, FAILURES_IN_A_ROW as usize);
    assert_eq!(report.counts.errors, u64::from(FAILURES_IN_A_ROW));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_with_a_file_going_in_needs_no_marker_in_an_object_storage() {
    let w = world(SyncRuleDeleteMode::Keep);
    write(&w.source.join("x.txt"), "x");
    write(&w.source.join("Fotos/2026/y.jpg"), "yy");
    std::fs::create_dir_all(w.source.join("leer")).unwrap();
    let fake = Arc::new(FakeStore::new());
    let storage = StoragePlace::new(Arc::new(StorageFiles::new(fake.clone(), access("b"))), "/");
    let report = w
        .run_with(
            &DevicePlace::with_rule_trash(&w.source),
            &storage,
            &CancellationToken::new(),
        )
        .await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    // Only the empty folder has a marker; the others are the prefixes of their files.
    assert_eq!(
        fake.keys("b"),
        ["Fotos/2026/y.jpg", "leer/", "x.txt"],
        "{:?}",
        fake.calls()
    );
    // A file is checked once, after it went up; the storage is not asked for its name first.
    let calls = fake.calls();
    for key in ["x.txt", "Fotos/2026/y.jpg"] {
        let at = |op: Op| calls.iter().position(|call| *call == (op, key.to_owned()));
        let heads = calls
            .iter()
            .filter(|call| **call == (Op::Head, key.to_owned()))
            .count();
        assert_eq!(heads, 1, "{key}: {calls:?}");
        assert!(at(Op::Put) < at(Op::Head), "{key}: {calls:?}");
    }
    let again = w
        .run_with(
            &DevicePlace::with_rule_trash(&w.source),
            &storage,
            &CancellationToken::new(),
        )
        .await;
    assert_eq!(again.counts.copied, 0, "{again:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_abort_keeps_the_state_of_finished_files_and_leaves_no_half_file() {
    let w = world(SyncRuleDeleteMode::Keep);
    for n in 1..=5 {
        write(&w.source.join(format!("f{n}.txt")), &"x".repeat(n));
    }
    let cancel = CancellationToken::new();
    let source = Tripwire {
        inner: DevicePlace::with_rule_trash(&w.source),
        cancel: cancel.clone(),
        stats: AtomicUsize::new(0),
        after: 3,
        fail_ignore_write: false,
    };
    let report = w
        .run_with(&source, &DevicePlace::with_rule_trash(&w.target), &cancel)
        .await;
    assert_eq!(report.outcome, SyncRuleOutcome::Cancelled);
    assert_eq!(
        w.state().len(),
        3,
        "only finished files are in the state (FR-015)"
    );
    assert_eq!(names(&w.target).len(), 3, "no half file (FR-014)");
    let again = w.run().await;
    assert_eq!(again.counts.copied, 2, "the rest, nothing twice (SC-005)");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_known_place_that_lists_empty_waits_and_changes_nothing() {
    let w = world(SyncRuleDeleteMode::Mirror);
    let (source, target) = (MemPlace::new(), MemPlace::new());
    source.put("x.txt", b"x", 1);
    let row = StateRow {
        path: "x.txt".to_owned(),
        source: Some(store::Seen {
            size: 1,
            mtime_ms: 1,
        }),
        target: Some(store::Seen {
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
    let report = w
        .run_with(&source, &target, &CancellationToken::new())
        .await;
    assert_eq!(report.outcome, SyncRuleOutcome::Waiting, "{report:?}");
    assert_eq!(
        target.calls("trash") + target.calls("delete") + target.calls("rename"),
        0
    );
    let gone_source = MemPlace::new();
    target.put("x.txt", b"x", 2);
    let report = w
        .run_with(&gone_source, &target, &CancellationToken::new())
        .await;
    assert_eq!(
        report.outcome,
        SyncRuleOutcome::Waiting,
        "an emptied source never mirrors"
    );
    assert_eq!(target.calls("trash"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_or_unreadable_root_stops_before_any_change() {
    let w = world(SyncRuleDeleteMode::Mirror);
    write(&w.source.join("x.txt"), "x");
    w.run().await;
    let moved = w.source.with_file_name("a-weg");
    std::fs::rename(&w.source, &moved).unwrap();
    let report = w.run().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Waiting, "{report:?}");
    assert_eq!(names(&w.target), ["x.txt"], "nothing deleted (SC-004)");
    std::fs::rename(&moved, &w.source).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&w.source, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root ignores permissions; nothing to check then.
        let readable = std::fs::read_dir(&w.source).is_ok();
        let report = w.run().await;
        std::fs::set_permissions(&w.source, std::fs::Permissions::from_mode(0o755)).unwrap();
        if !readable {
            assert!(
                matches!(
                    report.outcome,
                    SyncRuleOutcome::Waiting | SyncRuleOutcome::Error
                ),
                "{report:?}"
            );
            assert_eq!(names(&w.target), ["x.txt"]);
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_is_logged_with_its_files_and_a_waiting_run_with_its_reason() {
    use crate::file_sync::run_log::{SyncRuleRunAction, SyncRuleTrashKind};
    let w = world(SyncRuleDeleteMode::Mirror);
    write(&w.source.join("x.txt"), "x");
    write(&w.source.join("bleibt.txt"), "b");
    w.run().await;
    std::fs::remove_file(w.source.join("x.txt")).unwrap();
    w.run().await;
    let moved = w.source.with_file_name("a-weg");
    std::fs::rename(&w.source, &moved).unwrap();
    w.run().await;
    let runs =
        w.db.read_blocking(|r| Ok(run_log::runs(r, DEVICE, "r1")))
            .unwrap()
            .unwrap();
    assert_eq!(runs.len(), 3);
    assert_eq!(
        runs[0].outcome,
        SyncRuleOutcome::Waiting,
        "the latest first"
    );
    assert!(runs[0].error.is_some(), "with its reason (FR-043)");
    assert_eq!(runs[2].copied, 2);
    let items = |run: &str| {
        let run = run.to_owned();
        w.db.read_blocking(move |r| Ok(run_log::items(r, DEVICE, &run, 0, 100)))
            .unwrap()
            .unwrap()
    };
    let first = items(&runs[2].id);
    assert_eq!(
        first
            .iter()
            .map(|item| (item.action, item.path.as_str()))
            .collect::<Vec<_>>(),
        [
            (SyncRuleRunAction::Copy, "bleibt.txt"),
            (SyncRuleRunAction::Copy, "x.txt")
        ]
    );
    let deleted = items(&runs[1].id);
    assert_eq!(deleted[0].action, SyncRuleRunAction::Delete);
    assert_eq!(deleted[0].trash_kind, Some(SyncRuleTrashKind::Rule));
    assert_eq!(
        deleted[0].trash_path.as_deref(),
        Some(".holzi-trash/run1/x.txt")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_exclusion_added_later_leaves_the_file_alone_until_it_goes() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.source.join("x.tmp"), "eins");
    write(&w.source.join("bleibt.txt"), "b");
    w.run().await;
    write(&w.source.join(".haex-ignore"), "*.tmp\n[z-a\n");
    std::fs::remove_file(w.target.join("x.tmp")).unwrap();
    let report = w.run().await;
    assert_eq!(
        report.outcome,
        SyncRuleOutcome::Done,
        "a bad line does not stop it"
    );
    assert_eq!(report.ignore_errors.len(), 1, "and is named (FR-038)");
    assert!(w.source.join("x.tmp").is_file(), "not mirrored (SC-009)");
    assert!(!w.target.join("x.tmp").exists(), "not copied back");
    assert!(
        w.state().iter().all(|row| row.path != "x.tmp"),
        "out of the state (FR-040)"
    );
    // The emptied file reaches B first; the union holds until then.
    write(&w.source.join(".haex-ignore"), "");
    w.run().await;
    w.run().await;
    assert_eq!(
        read(&w.target.join("x.tmp")),
        "eins",
        "new again once the exclusion goes"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_place_waits() {
    let w = world(SyncRuleDeleteMode::Keep);
    let (source, target) = (MemPlace::new(), MemPlace::new());
    target.set_unreachable(true);
    let report = w
        .run_with(&source, &target, &CancellationToken::new())
        .await;
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
    write(&w.source.join("x.txt"), "x");
    write(&w.target.join("Fotos/y.txt"), "y");
    let first = w.run().await;
    assert_eq!(first.outcome, SyncRuleOutcome::Done, "{first:?}");
    assert_eq!(names(&w.source), ["Fotos/y.txt", "x.txt"]);
    assert_eq!(names(&w.target), ["Fotos/y.txt", "x.txt"]);
    write(&w.target.join("Fotos/y.txt"), "geändert");
    let second = w.run().await;
    assert_eq!(second.counts.copied, 1, "{second:?}");
    assert_eq!(read(&w.source.join("Fotos/y.txt")), "geändert");
    assert_eq!(w.run().await.counts.copied, 0, "and then nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_way_both_changed_the_newer_keeps_the_name_and_both_sides_get_both() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.source.join("x.txt"), "1");
    w.run().await;
    write_at(&w.source.join("x.txt"), "älter", 1_700_000_000);
    write_at(&w.target.join("x.txt"), "neuer!", 1_700_000_100);
    let report = w.run().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    assert_eq!(report.counts.conflicts, 1);
    let aside = "x (Konflikt Laptop 2026-10-27).txt";
    assert_eq!(report.conflict_copies, [aside]);
    for side in [&w.source, &w.target] {
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
    write(&w.source.join("x.txt"), "1");
    // A place that lists empty would make the rule wait (FR-022).
    write(&w.source.join("bleibt.txt"), "1");
    w.run().await;
    std::fs::remove_file(w.source.join("x.txt")).unwrap();
    write(&w.target.join("x.txt"), "geändert");
    let report = w.run().await;
    assert_eq!(report.counts.deleted, 0, "{report:?}");
    assert_eq!(read(&w.source.join("x.txt")), "geändert", "FR-013");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_way_a_deletion_is_mirrored_into_the_trash_of_the_other_side() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.source.join("x.txt"), "1");
    write(&w.source.join("bleibt.txt"), "1");
    w.run().await;
    std::fs::remove_file(w.target.join("x.txt")).unwrap();
    let report = w.run().await;
    assert_eq!(report.counts.deleted, 1, "{report:?}");
    assert!(w.source.join(".holzi-trash/run1/x.txt").is_file());
    assert_eq!(names(&w.target), ["bleibt.txt"]);
    let again = w.run().await;
    assert_eq!(again.counts.copied, 0, "the rule trash is never synced");
    assert_eq!(names(&w.target), ["bleibt.txt"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_waiting_deletion_goes_when_the_file_comes_back() {
    let w = both(SyncRuleDeleteMode::Ask);
    write(&w.source.join("x.txt"), "1");
    write(&w.source.join("bleibt.txt"), "1");
    w.run().await;
    std::fs::remove_file(w.source.join("x.txt")).unwrap();
    let report = w.run().await;
    assert_eq!(report.counts.pending, 1, "{report:?}");
    write_at(&w.source.join("x.txt"), "wieder da", 1_800_000_000);
    w.run().await;
    let pending =
        w.db.read_blocking(|r| Ok(store::pending(r, DEVICE, "r1")))
            .unwrap()
            .unwrap();
    assert!(pending.is_empty(), "{pending:?}");
    assert_eq!(read(&w.target.join("x.txt")), "wieder da");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_way_a_file_moved_on_the_other_side_moves_here() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.source.join("alt/x.txt"), "inhalt");
    w.run().await;
    std::fs::create_dir_all(w.target.join("neu")).unwrap();
    std::fs::rename(w.target.join("alt/x.txt"), w.target.join("neu/x.txt")).unwrap();
    let report = w.run().await;
    assert_eq!(report.counts.moved, 1, "{report:?}");
    assert_eq!(report.counts.copied, 0);
    assert_eq!(names(&w.source), ["neu/x.txt"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_way_a_first_run_adopts_equal_files_and_keeps_both_of_different_ones() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.source.join("same.txt"), "gleich");
    write(&w.target.join("same.txt"), "gleich");
    write_at(&w.source.join("other.txt"), "AAAA", 1_700_000_000);
    write_at(&w.target.join("other.txt"), "BBBB", 1_700_000_100);
    let report = w.run().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    assert_eq!(report.counts.adopted, 1);
    assert_eq!(report.counts.conflicts, 1);
    for side in [&w.source, &w.target] {
        assert_eq!(read(&side.join("other.txt")), "BBBB");
        assert_eq!(
            read(&side.join("other (Konflikt Laptop 2026-10-27).txt")),
            "AAAA"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_way_differing_exclusion_files_keep_both_and_become_their_union() {
    use crate::file_sync::run_log::SyncRuleRunAction;

    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.source.join(".haex-ignore"), "*.tmp\n");
    write(&w.target.join(".haex-ignore"), "*.log\n");
    write(&w.source.join("x.tmp"), "a");
    write(&w.target.join("y.log"), "b");
    let (aside_source, aside_target) = (
        ".haex-ignore (Konflikt Laptop 2026-10-27)",
        ".haex-ignore (Konflikt Laptop 2026-10-27 2)",
    );
    let report = w.run().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    let runs =
        w.db.read_blocking(|r| Ok(run_log::runs(r, DEVICE, "r1")))
            .unwrap()
            .unwrap();
    let items =
        w.db.read_blocking({
            let run_id = runs[0].id.clone();
            move |r| Ok(run_log::items(r, DEVICE, &run_id, 0, 100))
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|item| (item.action, item.path.as_str(), item.side))
            .collect::<Vec<_>>(),
        [
            (
                SyncRuleRunAction::ConflictCopy,
                ".haex-ignore",
                SyncRuleSide::Source
            ),
            (
                SyncRuleRunAction::ConflictCopy,
                ".haex-ignore",
                SyncRuleSide::Target
            ),
            (SyncRuleRunAction::Copy, aside_source, SyncRuleSide::Target),
            (SyncRuleRunAction::Copy, aside_target, SyncRuleSide::Source),
            (
                SyncRuleRunAction::Copy,
                ".haex-ignore",
                SyncRuleSide::Source
            ),
            (
                SyncRuleRunAction::Copy,
                ".haex-ignore",
                SyncRuleSide::Target
            ),
        ]
    );
    for side in [&w.source, &w.target] {
        assert_eq!(read(&side.join(".haex-ignore")), "*.tmp\n*.log\n");
        assert_eq!(read(&side.join(aside_source)), "*.tmp\n");
        assert_eq!(read(&side.join(aside_target)), "*.log\n");
    }
    assert!(!w.target.join("x.tmp").exists() && !w.source.join("y.log").exists());
    let again = w.run().await;
    assert_eq!(again.counts.copied + again.counts.conflicts, 0, "{again:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_exclusion_union_write_keeps_both_original_files() {
    let w = both(SyncRuleDeleteMode::Mirror);
    write(&w.source.join(".haex-ignore"), "*.tmp\n");
    write(&w.target.join(".haex-ignore"), "*.log\n");
    let source = Tripwire {
        inner: DevicePlace::with_rule_trash(&w.source),
        cancel: CancellationToken::new(),
        stats: AtomicUsize::new(0),
        after: usize::MAX,
        fail_ignore_write: true,
    };

    let report = w
        .run_with(
            &source,
            &DevicePlace::with_rule_trash(&w.target),
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
    assert_eq!(read(&w.source.join(".haex-ignore")), "*.tmp\n");
    assert_eq!(read(&w.target.join(".haex-ignore")), "*.log\n");
}

#[path = "perf_tests.rs"]
mod perf;

// ---- file rules (spec 074) ----

use crate::file_sync::model::SyncRuleSourceKind;
use crate::file_sync::place_file::OneFilePlace;

const FILE: &str = "Haushalt.ods";

/// A file rule: the source folder holds `Haushalt.ods` and another file, the target folder is empty.
fn file_world() -> World {
    let mut w = world(SyncRuleDeleteMode::Keep);
    write(&w.source.join(FILE), "eins");
    write(&w.source.join("anderes.txt"), "x");
    w.rule.source_kind = SyncRuleSourceKind::File;
    w.rule.source.path = w.source.join(FILE).to_string_lossy().into_owned();
    w
}

impl World {
    /// The two sides of a file rule as `open_rule_place` wraps them: the folder of the rule's source
    /// file and the target folder, each showing the file's name.
    fn file_places(&self) -> (OneFilePlace, OneFilePlace) {
        let path = Path::new(&self.rule.source.path);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        (
            OneFilePlace::new(
                Box::new(DevicePlace::with_rule_trash(path.parent().unwrap())),
                name.clone(),
            ),
            OneFilePlace::new(Box::new(DevicePlace::with_rule_trash(&self.target)), name),
        )
    }

    async fn run_file(&self) -> RunReport {
        let (source, target) = self.file_places();
        self.run_with(&source, &target, &CancellationToken::new())
            .await
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_rule_copies_its_file_and_nothing_else_and_then_nothing_at_all() {
    let w = file_world();
    let first = w.run_file().await;
    assert_eq!(first.outcome, SyncRuleOutcome::Done, "{first:?}");
    assert_eq!(first.counts.copied, 1);
    assert_eq!(
        names(&w.target),
        [FILE],
        "only the file, not its neighbours"
    );
    assert_eq!(
        std::fs::read_to_string(w.target.join(FILE)).unwrap(),
        "eins"
    );
    let second = w.run_file().await;
    assert_eq!(second.outcome, SyncRuleOutcome::Done);
    assert_eq!(
        second.counts.copied, 0,
        "an unchanged source copies nothing"
    );
    assert_eq!(second.items.len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_source_file_is_copied_again_and_the_log_has_the_copy() {
    let w = file_world();
    assert_eq!(w.run_file().await.counts.copied, 1);
    write(&w.source.join(FILE), "zwei und drei");
    let report = w.run_file().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    assert_eq!(report.counts.copied, 1);
    assert_eq!(
        std::fs::read_to_string(w.target.join(FILE)).unwrap(),
        "zwei und drei"
    );
    assert!(
        report
            .items
            .iter()
            .any(|item| item.path == FILE && item.action == run_log::SyncRuleRunAction::Copy),
        "{:?}",
        report.items
    );
    assert_eq!(names(&w.target), [FILE]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_different_file_at_the_target_on_the_first_run_is_kept_as_a_conflict_copy() {
    let w = file_world();
    write(&w.target.join(FILE), "fremde Version");
    let report = w.run_file().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    assert_eq!(report.conflict_copies.len(), 1, "{report:?}");
    assert_eq!(
        std::fs::read_to_string(w.target.join(FILE)).unwrap(),
        "eins"
    );
    let all = names(&w.target);
    assert_eq!(all.len(), 2, "the file and the conflict copy: {all:?}");
    let aside = all.iter().find(|name| name.as_str() != FILE).unwrap();
    assert_eq!(
        std::fs::read_to_string(w.target.join(aside)).unwrap(),
        "fremde Version",
        "nothing is lost"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_copy_of_a_one_way_file_rule_stays_and_counts_a_deviation() {
    let w = file_world();
    assert_eq!(w.run_file().await.counts.copied, 1);
    write(&w.target.join(FILE), "im Ziel geändert");
    let report = w.run_file().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    assert_eq!(report.counts.deviations, 1);
    assert_eq!(report.counts.copied, 0);
    assert_eq!(
        std::fs::read_to_string(w.target.join(FILE)).unwrap(),
        "im Ziel geändert"
    );
    // The source changes later too: the changed copy is kept as a conflict copy, the source wins.
    write(&w.source.join(FILE), "Quelle neu und länger");
    let again = w.run_file().await;
    assert_eq!(again.conflict_copies.len(), 1, "{again:?}");
    assert_eq!(
        std::fs::read_to_string(w.target.join(FILE)).unwrap(),
        "Quelle neu und länger"
    );
    assert!(names(&w.target).iter().any(|name| name != FILE
        && std::fs::read_to_string(w.target.join(name)).unwrap() == "im Ziel geändert"));
}

// ---- a source that is gone, a copy that is gone, both ways (spec 074) ----

use crate::file_sync::model::SyncRuleDirection as Direction;

/// The target folder's copy and the source folder's file are both still there.
fn both_still_there(w: &World, content: &str) {
    assert_eq!(
        std::fs::read_to_string(w.target.join(FILE)).unwrap(),
        content,
        "the copy is untouched"
    );
    assert_eq!(names(&w.target), [FILE], "nothing else appeared or went");
}

async fn invalid_run(w: &World, reason: &str) {
    let report = w.run_file().await;
    assert_eq!(report.outcome, SyncRuleOutcome::InvalidSource, "{report:?}");
    assert_eq!(report.error.as_deref(), Some(reason), "{report:?}");
    assert_eq!(report.counts.copied, 0);
    assert_eq!(report.counts.deleted, 0);
    assert!(report.items.is_empty());
    let rule =
        w.db.read_blocking(|r| Ok(store::rule(r, DEVICE, "r1")))
            .unwrap()
            .unwrap()
            .unwrap();
    assert_eq!(rule.last_outcome, Some(SyncRuleOutcome::InvalidSource));
    assert_eq!(rule.last_error.as_deref(), Some(reason));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_renamed_moved_or_deleted_source_file_leaves_the_target_untouched_and_the_source_invalid()
{
    for both in [false, true] {
        for action in ["rename", "move", "delete"] {
            let mut w = file_world();
            if both {
                w.rule.direction = Direction::Both;
            }
            assert_eq!(w.run_file().await.counts.copied, 1);
            let file = w.source.join(FILE);
            match action {
                "rename" => std::fs::rename(&file, w.source.join("Haushalt-2026.ods")).unwrap(),
                "move" => {
                    std::fs::create_dir(w.source.join("alt")).unwrap();
                    std::fs::rename(&file, w.source.join("alt").join(FILE)).unwrap();
                }
                _ => std::fs::remove_file(&file).unwrap(),
            }
            // Twice: the second run finds the same, and still changes nothing.
            invalid_run(&w, "noFile").await;
            invalid_run(&w, "noFile").await;
            both_still_there(&w, "eins");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_above_the_source_file_renamed_or_a_folder_at_its_path_is_invalid() {
    let mut w = file_world();
    let sub = w.source.join("sub");
    write(&sub.join(FILE), "eins");
    w.rule.source.path = sub.join(FILE).to_string_lossy().into_owned();
    assert_eq!(w.run_file().await.counts.copied, 1);
    std::fs::rename(&sub, w.source.join("sub2")).unwrap();
    invalid_run(&w, "folderGone").await;
    both_still_there(&w, "eins");

    // A folder at the file's path, in a folder that is there.
    let mut w = file_world();
    assert_eq!(w.run_file().await.counts.copied, 1);
    std::fs::remove_file(w.source.join(FILE)).unwrap();
    std::fs::create_dir(w.source.join(FILE)).unwrap();
    invalid_run(&w, "noFile").await;
    both_still_there(&w, "eins");
    w.rule.direction = Direction::Both;
    invalid_run(&w, "noFile").await;
    both_still_there(&w, "eins");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_at_the_path_again_ends_the_invalid_state_without_a_step_by_the_user() {
    let w = file_world();
    assert_eq!(w.run_file().await.counts.copied, 1);
    std::fs::remove_file(w.source.join(FILE)).unwrap();
    invalid_run(&w, "noFile").await;
    write(&w.source.join(FILE), "wieder da, anders");
    let report = w.run_file().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    assert_eq!(report.counts.copied, 1);
    assert_eq!(
        std::fs::read_to_string(w.target.join(FILE)).unwrap(),
        "wieder da, anders"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_replaced_by_renaming_a_new_one_over_it_stays_valid() {
    let w = file_world();
    assert_eq!(w.run_file().await.counts.copied, 1);
    // An editor that writes a new file beside the old one and renames it over the old one.
    write(&w.source.join("Haushalt.ods.tmp"), "neu geschrieben");
    std::fs::rename(w.source.join("Haushalt.ods.tmp"), w.source.join(FILE)).unwrap();
    let report = w.run_file().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    assert_eq!(
        std::fs::read_to_string(w.target.join(FILE)).unwrap(),
        "neu geschrieben"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rule_edited_to_the_renamed_file_is_valid_and_runs() {
    let mut w = file_world();
    assert_eq!(w.run_file().await.counts.copied, 1);
    std::fs::rename(w.source.join(FILE), w.source.join("Haushalt-2026.ods")).unwrap();
    invalid_run(&w, "noFile").await;
    // The edit picks the renamed file as the source.
    w.rule.source.path = w
        .source
        .join("Haushalt-2026.ods")
        .to_string_lossy()
        .into_owned();
    let report = w.run_file().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Done, "{report:?}");
    assert_eq!(report.counts.copied, 1);
    assert_eq!(
        names(&w.target),
        ["Haushalt-2026.ods", FILE],
        "the old copy stays, nothing is deleted"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_rule_with_an_invalid_source_changes_neither_file() {
    let w = file_world();
    assert_eq!(w.run_file().await.counts.copied, 1);
    std::fs::remove_file(w.source.join(FILE)).unwrap();
    invalid_run(&w, "noFile").await;
    let gone =
        w.db.write_blocking(|tx| Ok(store::delete_rule(tx, DEVICE, "r1")))
            .unwrap()
            .unwrap();
    assert!(gone);
    both_still_there(&w, "eins");
    assert!(!w.source.join(FILE).exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_target_folder_waits_and_deletes_nothing() {
    let w = file_world();
    assert_eq!(w.run_file().await.counts.copied, 1);
    std::fs::remove_dir_all(&w.target).unwrap();
    let report = w.run_file().await;
    assert_eq!(report.outcome, SyncRuleOutcome::Waiting, "{report:?}");
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|e| e.starts_with("target")),
        "{report:?}"
    );
    assert_eq!(
        std::fs::read_to_string(w.source.join(FILE)).unwrap(),
        "eins",
        "the source is untouched"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_source_place_waits_and_is_not_an_invalid_source() {
    let w = file_world();
    let memory = MemPlace::new();
    memory.set_unreachable(true);
    let source = OneFilePlace::new(Box::new(memory), FILE);
    let (_, target) = w.file_places();
    let report = w
        .run_with(&source, &target, &CancellationToken::new())
        .await;
    assert_eq!(report.outcome, SyncRuleOutcome::Waiting, "{report:?}");
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|e| e.starts_with("source")),
        "{report:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_two_way_file_rule_brings_an_edited_copy_back_and_keeps_both_versions_in_a_conflict() {
    let mut w = file_world();
    w.rule.direction = Direction::Both;
    assert_eq!(w.run_file().await.counts.copied, 1);
    // Only the copy changed: it goes back to the source.
    write(&w.target.join(FILE), "im Ziel bearbeitet");
    let back = w.run_file().await;
    assert_eq!(back.outcome, SyncRuleOutcome::Done, "{back:?}");
    assert_eq!(back.counts.copied, 1);
    assert_eq!(
        std::fs::read_to_string(w.source.join(FILE)).unwrap(),
        "im Ziel bearbeitet"
    );
    // Both changed since the last run: neither is overwritten, both versions are on both sides.
    write(&w.source.join(FILE), "links geändert!");
    write(&w.target.join(FILE), "rechts geändert");
    let conflict = w.run_file().await;
    assert_eq!(conflict.outcome, SyncRuleOutcome::Done, "{conflict:?}");
    assert_eq!(conflict.counts.conflicts, 1);
    for folder in [&w.source, &w.target] {
        let mut versions: Vec<String> = names(folder)
            .into_iter()
            .filter(|name| name.starts_with("Haushalt"))
            .map(|name| std::fs::read_to_string(folder.join(name)).unwrap())
            .collect();
        versions.sort();
        assert_eq!(
            versions,
            ["links geändert!", "rechts geändert"],
            "{folder:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deleted_copy_is_copied_again_and_nothing_is_deleted_whatever_the_mode() {
    for both in [false, true] {
        for mode in [
            SyncRuleDeleteMode::Keep,
            SyncRuleDeleteMode::Mirror,
            SyncRuleDeleteMode::Ask,
        ] {
            let mut w = file_world();
            w.rule.delete_mode = mode;
            if both {
                w.rule.direction = Direction::Both;
            }
            assert_eq!(w.run_file().await.counts.copied, 1);
            std::fs::remove_file(w.target.join(FILE)).unwrap();
            let report = w.run_file().await;
            assert_eq!(
                report.outcome,
                SyncRuleOutcome::Done,
                "both={both} {mode:?}: {report:?}"
            );
            assert_eq!(report.counts.copied, 1);
            assert_eq!(report.counts.deleted, 0);
            assert_eq!(report.counts.pending, 0);
            both_still_there(&w, "eins");
            assert_eq!(
                std::fs::read_to_string(w.source.join(FILE)).unwrap(),
                "eins",
                "the source is never deleted"
            );
        }
    }
}
