//! The entry for sync rules (spec 061 research R2): a planned replace writes over the target, a
//! target that changed after the plan is left alone and reported as a conflict.

use std::path::PathBuf;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::*;
use crate::files::storage_source::StorageFiles;
use crate::remote_storage::test_support::{access, FakeStore, Op};
use crate::remote_storage::StorageError;

const BUCKET: &str = "b";

fn folder() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    (dir, real)
}

async fn run(one: OneTransferOwned) -> Outcome {
    tokio::task::spawn_blocking(move || {
        let handle = Handle::current();
        let cancel = CancellationToken::new();
        transfer_one(
            &OneTransfer {
                op: one.op,
                from: &one.from,
                path: &one.path,
                to: &one.to,
                folder: &one.folder,
                replaces: one.replaces,
            },
            &handle,
            &cancel,
            &mut |_| {},
        )
    })
    .await
    .unwrap()
}

struct OneTransferOwned {
    op: TransferOp,
    from: Side,
    path: String,
    to: Side,
    folder: String,
    replaces: bool,
}

fn device_copy(from: &std::path::Path, to: &std::path::Path, replaces: bool) -> OneTransferOwned {
    OneTransferOwned {
        op: TransferOp::Copy,
        from: Side::Device,
        path: from.to_string_lossy().into_owned(),
        to: Side::Device,
        folder: to.to_string_lossy().into_owned(),
        replaces,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_planned_replace_writes_over_the_target_and_keeps_the_time() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("a")).unwrap();
    std::fs::create_dir_all(real.join("b")).unwrap();
    std::fs::write(real.join("a/x.txt"), "neu").unwrap();
    std::fs::write(real.join("b/x.txt"), "alt").unwrap();
    let outcome = run(device_copy(&real.join("a/x.txt"), &real.join("b"), true)).await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(
        std::fs::read_to_string(real.join("b/x.txt")).unwrap(),
        "neu"
    );
    let names: Vec<_> = std::fs::read_dir(real.join("b"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["x.txt"], "no part or old file left");
    let time = |p: &str| std::fs::metadata(real.join(p)).unwrap().modified().unwrap();
    assert_eq!(time("a/x.txt"), time("b/x.txt"), "the copy keeps the time");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_that_appeared_after_the_plan_is_left_and_reported() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("a")).unwrap();
    std::fs::create_dir_all(real.join("b")).unwrap();
    std::fs::write(real.join("a/x.txt"), "neu").unwrap();
    std::fs::write(real.join("b/x.txt"), "fremd").unwrap();
    let outcome = run(device_copy(&real.join("a/x.txt"), &real.join("b"), false)).await;
    let Outcome::Failed(error) = outcome else {
        panic!("expected a conflict, got {outcome:?}");
    };
    assert_eq!(error.code, FilesErrorCode::Conflict);
    assert_eq!(
        std::fs::read_to_string(real.join("b/x.txt")).unwrap(),
        "fremd"
    );
}

fn upload(source: &std::path::Path, fake: &Arc<FakeStore>, replaces: bool) -> OneTransferOwned {
    OneTransferOwned {
        op: TransferOp::Copy,
        from: Side::Device,
        path: source.to_string_lossy().into_owned(),
        to: Side::Storage(Arc::new(StorageFiles::new(fake.clone(), access(BUCKET)))),
        folder: "/".to_owned(),
        replaces,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_peer_version_conflict_on_a_new_file_does_not_write_over_it() {
    let (_dir, real) = folder();
    std::fs::write(real.join("a.txt"), "neu").unwrap();
    let fake = Arc::new(FakeStore::new());
    fake.fail_once(Op::Put, StorageError::Conflict);
    let outcome = run(upload(&real.join("a.txt"), &fake, false)).await;
    let Outcome::Failed(error) = outcome else {
        panic!("expected a conflict, got {outcome:?}");
    };
    assert_eq!(error.code, FilesErrorCode::Conflict);
    assert!(fake.keys(BUCKET).is_empty(), "nothing was written over");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_peer_version_conflict_after_a_planned_replace_is_reported() {
    let (_dir, real) = folder();
    std::fs::write(real.join("a.txt"), "neu").unwrap();
    let fake = Arc::new(FakeStore::new());
    fake.insert(BUCKET, "a.txt", b"alt");
    fake.fail_once(Op::Put, StorageError::Conflict);
    let outcome = run(upload(&real.join("a.txt"), &fake, true)).await;
    let Outcome::Failed(error) = outcome else {
        panic!("expected a conflict, got {outcome:?}");
    };
    assert_eq!(error.code, FilesErrorCode::Conflict);
    assert_eq!(fake.object(BUCKET, "a.txt").as_deref(), Some(&b"alt"[..]));
}
