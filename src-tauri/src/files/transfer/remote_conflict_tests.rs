//! A target that changed while a file went up (spec 056 US3, FR-036, research R7): the transfer asks
//! as for a taken name; "Ersetzen" sends again, "Beide behalten" sends under a free name,
//! "Überspringen" leaves it, and a second conflict ends the transfer.

use std::path::PathBuf;
use std::sync::Arc;

use super::{job, run_job};
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::local::Outcome;
use crate::files::transfer::remote::{Side, PART};
use crate::files::transfer::{ConflictChoice, TransferOp};
use crate::files::FilesErrorCode;
use crate::remote_storage::test_support::{access, FakeStore, Op};
use crate::remote_storage::StorageError;

const BUCKET: &str = "b";

/// A file `name` with `content` in a new folder, and the fake target.
fn file(name: &str, content: &[u8]) -> (tempfile::TempDir, String, Arc<FakeStore>) {
    let dir = tempfile::tempdir().unwrap();
    let path: PathBuf = std::fs::canonicalize(dir.path()).unwrap().join(name);
    std::fs::write(&path, content).unwrap();
    (
        dir,
        path.to_string_lossy().into_owned(),
        Arc::new(FakeStore::new()),
    )
}

fn up(fake: &Arc<FakeStore>, source: &str) -> super::RemoteJob {
    let to = Side::Storage(Arc::new(StorageFiles::new(fake.clone(), access(BUCKET))));
    job(TransferOp::Copy, Side::Device, &[source], to, "/")
}

#[tokio::test(flavor = "multi_thread")]
async fn replace_sends_the_file_again() {
    let (_dir, source, fake) = file("a.txt", b"neu");
    fake.fail_once(Op::Put, StorageError::Conflict);
    let (outcome, asked) = run_job(
        up(&fake, &source),
        vec![(ConflictChoice::Replace, false)],
        None,
    )
    .await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(asked, ["a.txt"], "asked once, after the conflict");
    assert_eq!(fake.object(BUCKET, "a.txt").as_deref(), Some(&b"neu"[..]));
}

#[tokio::test(flavor = "multi_thread")]
async fn keep_both_sends_a_large_file_under_a_free_name() {
    let content: Vec<u8> = (0..PART + 10).map(|i| (i % 251) as u8).collect();
    let (_dir, source, fake) = file("gross.bin", &content);
    fake.fail_once(Op::FinishUpload, StorageError::Conflict);
    let (outcome, asked) = run_job(
        up(&fake, &source),
        vec![(ConflictChoice::KeepBoth, false)],
        None,
    )
    .await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(asked, ["gross.bin"]);
    assert_eq!(fake.keys(BUCKET), ["gross (2).bin"]);
    assert_eq!(fake.object(BUCKET, "gross (2).bin"), Some(content));
    assert!(
        fake.open_uploads().is_empty(),
        "the refused upload was dropped"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn skip_leaves_the_target_as_the_other_side_wrote_it() {
    let (_dir, source, fake) = file("a.txt", b"neu");
    fake.fail_once(Op::Put, StorageError::Conflict);
    let (outcome, _) = run_job(
        up(&fake, &source),
        vec![(ConflictChoice::Skip, false)],
        None,
    )
    .await;
    assert_eq!(outcome, Outcome::Done);
    assert!(fake.keys(BUCKET).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_conflict_ends_the_transfer() {
    let (_dir, source, fake) = file("a.txt", b"neu");
    fake.fail(Op::Put, StorageError::Conflict);
    let (outcome, asked) = run_job(
        up(&fake, &source),
        vec![(ConflictChoice::Replace, true)],
        None,
    )
    .await;
    let Outcome::Failed(error) = outcome else {
        panic!("expected a failure, got {outcome:?}");
    };
    assert_eq!(error.code, FilesErrorCode::Conflict);
    assert_eq!(asked, ["a.txt"], "asked once, not in a loop");
}
