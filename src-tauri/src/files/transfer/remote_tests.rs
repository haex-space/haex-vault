use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::*;
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::local::{Outcome, Removal};
use crate::files::FilesErrorCode;
use crate::remote_storage::test_support::{access, FakeStore, Op};
use crate::remote_storage::StorageError;

const BUCKET: &str = "b";

fn folder() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    (dir, real)
}

fn storage(fake: &Arc<FakeStore>, bucket: &str) -> Side {
    Side::Storage(Arc::new(StorageFiles::new(fake.clone(), access(bucket))))
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Runs `job` on a blocking thread as the manager does, answering conflicts from `answers`.
pub(super) async fn run_job(
    job: RemoteJob,
    answers: Vec<(ConflictChoice, bool)>,
    cancel_after_progress: Option<usize>,
) -> (Outcome, Vec<String>) {
    tokio::task::spawn_blocking(move || {
        let handle = tokio::runtime::Handle::current();
        let cancel = CancellationToken::new();
        let plan = prepare_remote(&job, &handle, |_| None).expect("prepare");
        let asked = RefCell::new(Vec::new());
        let mut answers = answers.into_iter();
        let mut seen = 0;
        let outcome = run_remote(
            &job,
            &plan,
            &handle,
            &[Duration::ZERO; 3],
            Hooks {
                cancel: &cancel,
                ask: &mut |name| {
                    asked.borrow_mut().push(name.to_owned());
                    answers.next()
                },
                progress: &mut |_| {
                    seen += 1;
                    if cancel_after_progress == Some(seen) {
                        cancel.cancel();
                    }
                },
                removal: Removal::Permanent,
                rename: &|from, to| std::fs::rename(from, to),
            },
        );
        (outcome, asked.into_inner())
    })
    .await
    .unwrap()
}

pub(super) fn job(
    op: TransferOp,
    from: Side,
    sources: &[&str],
    to: Side,
    target: &str,
) -> RemoteJob {
    RemoteJob {
        op,
        from,
        sources: sources.iter().map(|s| (*s).to_owned()).collect(),
        to,
        target: target.to_owned(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_small_file_goes_up_in_one_put_and_a_folder_keeps_its_tree() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("Fotos/2026")).unwrap();
    std::fs::write(real.join("Fotos/a.jpg"), "a").unwrap();
    std::fs::write(real.join("Fotos/2026/b.jpg"), "b").unwrap();
    std::fs::create_dir(real.join("Fotos/leer")).unwrap();
    let fake = Arc::new(FakeStore::new());
    let source = real.join("Fotos").to_string_lossy().into_owned();
    let (outcome, _) = run_job(
        job(
            TransferOp::Copy,
            Side::Device,
            &[&source],
            storage(&fake, BUCKET),
            "/Ziel",
        ),
        vec![],
        None,
    )
    .await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(
        fake.keys(BUCKET),
        [
            "Ziel/Fotos/2026/b.jpg",
            "Ziel/Fotos/a.jpg",
            "Ziel/Fotos/leer/"
        ]
    );
    assert!(!fake.calls().iter().any(|(op, _)| *op == Op::BeginUpload));
    assert!(real.join("Fotos/a.jpg").exists(), "a copy keeps the source");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_large_file_goes_up_in_parts_of_8_mib() {
    let (_dir, real) = folder();
    let content: Vec<u8> = (0..PART + 1000).map(|i| (i % 251) as u8).collect();
    std::fs::write(real.join("gross.bin"), &content).unwrap();
    let fake = Arc::new(FakeStore::new());
    let source = real.join("gross.bin").to_string_lossy().into_owned();
    let (outcome, _) = run_job(
        job(
            TransferOp::Copy,
            Side::Device,
            &[&source],
            storage(&fake, BUCKET),
            "/",
        ),
        vec![],
        None,
    )
    .await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(fake.object(BUCKET, "gross.bin").unwrap(), content);
    let parts = fake
        .calls()
        .iter()
        .filter(|(op, _)| *op == Op::UploadPart)
        .count();
    assert_eq!(parts, 2);
    assert!(fake.open_uploads().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_upload_in_parts_is_aborted() {
    let (_dir, real) = folder();
    std::fs::write(real.join("gross.bin"), vec![1u8; 3 * PART]).unwrap();
    let fake = Arc::new(FakeStore::new());
    let source = real.join("gross.bin").to_string_lossy().into_owned();
    let (outcome, _) = run_job(
        job(
            TransferOp::Copy,
            Side::Device,
            &[&source],
            storage(&fake, BUCKET),
            "/",
        ),
        vec![],
        Some(1),
    )
    .await;
    assert_eq!(outcome, Outcome::Cancelled);
    assert!(fake.keys(BUCKET).is_empty());
    assert!(fake.open_uploads().is_empty(), "the upload was aborted");
    assert!(fake.calls().iter().any(|(op, _)| *op == Op::AbortUpload));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_download_goes_through_a_part_file() {
    let (_dir, real) = folder();
    let fake = Arc::new(FakeStore::new());
    fake.insert(BUCKET, "Fotos/a.jpg", b"aaaa");
    fake.insert(BUCKET, "Fotos/tief/b.jpg", b"bb");
    let (outcome, _) = run_job(
        job(
            TransferOp::Copy,
            storage(&fake, BUCKET),
            &["/Fotos"],
            Side::Device,
            &real.to_string_lossy(),
        ),
        vec![],
        None,
    )
    .await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(std::fs::read(real.join("Fotos/a.jpg")).unwrap(), b"aaaa");
    assert_eq!(std::fs::read(real.join("Fotos/tief/b.jpg")).unwrap(), b"bb");
    assert_eq!(names(&real.join("Fotos")), ["a.jpg", "tief"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_download_leaves_no_part_file() {
    let (_dir, real) = folder();
    let fake = Arc::new(FakeStore::new());
    fake.insert(
        BUCKET,
        "gross.bin",
        &vec![7u8; 4 * crate::files::transfer::local::CHUNK],
    );
    let (outcome, _) = run_job(
        job(
            TransferOp::Copy,
            storage(&fake, BUCKET),
            &["/gross.bin"],
            Side::Device,
            &real.to_string_lossy(),
        ),
        vec![],
        Some(1),
    )
    .await;
    assert_eq!(outcome, Outcome::Cancelled);
    assert!(names(&real).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn within_one_bucket_the_provider_copies_and_a_move_deletes() {
    let fake = Arc::new(FakeStore::new());
    fake.insert(BUCKET, "Alt/a.txt", b"a");
    fake.insert(BUCKET, "Alt/tief/b.txt", b"b");
    let (outcome, _) = run_job(
        job(
            TransferOp::Move,
            storage(&fake, BUCKET),
            &["/Alt"],
            storage(&fake, BUCKET),
            "/Neu",
        ),
        vec![],
        None,
    )
    .await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(fake.keys(BUCKET), ["Neu/Alt/a.txt", "Neu/Alt/tief/b.txt"]);
    let copies = fake
        .calls()
        .iter()
        .filter(|(op, _)| *op == Op::Copy)
        .count();
    assert_eq!(copies, 2);
    assert!(!fake.calls().iter().any(|(op, _)| *op == Op::GetRange));
}

#[tokio::test(flavor = "multi_thread")]
async fn within_one_bucket_without_copies_on_the_provider_the_bytes_stream_through() {
    // An SFTP server cannot copy (spec 050 research R8).
    let fake = Arc::new(FakeStore::new());
    fake.without_copy();
    fake.insert(BUCKET, "Alt/a.txt", b"a");
    let (outcome, _) = run_job(
        job(
            TransferOp::Copy,
            storage(&fake, BUCKET),
            &["/Alt/a.txt"],
            storage(&fake, BUCKET),
            "/Neu",
        ),
        vec![],
        None,
    )
    .await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(fake.object(BUCKET, "Neu/a.txt").as_deref(), Some(&b"a"[..]));
    assert!(!fake.calls().iter().any(|(op, _)| *op == Op::Copy));
    assert!(fake.calls().iter().any(|(op, _)| *op == Op::GetRange));
}

#[tokio::test(flavor = "multi_thread")]
async fn between_two_storages_the_bytes_stream_through() {
    let fake = Arc::new(FakeStore::new());
    fake.insert("eins", "a.txt", b"inhalt");
    let (outcome, _) = run_job(
        job(
            TransferOp::Copy,
            storage(&fake, "eins"),
            &["/a.txt"],
            storage(&fake, "zwei"),
            "/",
        ),
        vec![],
        None,
    )
    .await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(fake.object("zwei", "a.txt").unwrap(), b"inhalt");
    assert_eq!(fake.object("eins", "a.txt").unwrap(), b"inhalt");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_network_error_is_tried_again_and_then_fails() {
    let (_dir, real) = folder();
    std::fs::write(real.join("a.txt"), "a").unwrap();
    let source = real.join("a.txt").to_string_lossy().into_owned();
    let fake = Arc::new(FakeStore::new());
    fake.fail_once(Op::Put, StorageError::Network);
    let (outcome, _) = run_job(
        job(
            TransferOp::Copy,
            Side::Device,
            &[&source],
            storage(&fake, BUCKET),
            "/",
        ),
        vec![],
        None,
    )
    .await;
    assert_eq!(outcome, Outcome::Done, "once is tried again");
    let puts = fake.calls().iter().filter(|(op, _)| *op == Op::Put).count();
    assert_eq!(puts, 2);

    let broken = Arc::new(FakeStore::new());
    broken.fail(Op::Put, StorageError::Network);
    let (outcome, _) = run_job(
        job(
            TransferOp::Copy,
            Side::Device,
            &[&source],
            storage(&broken, BUCKET),
            "/",
        ),
        vec![],
        None,
    )
    .await;
    let Outcome::Failed(error) = outcome else {
        panic!("{outcome:?}")
    };
    assert_eq!(error.code, FilesErrorCode::Unreachable);
    let puts = broken
        .calls()
        .iter()
        .filter(|(op, _)| *op == Op::Put)
        .count();
    assert_eq!(puts, 4, "the first try and three more");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_taken_key_asks_and_keep_both_counts_up() {
    let (_dir, real) = folder();
    std::fs::write(real.join("a.txt"), "neu").unwrap();
    let source = real.join("a.txt").to_string_lossy().into_owned();
    let fake = Arc::new(FakeStore::new());
    fake.insert(BUCKET, "a.txt", b"alt");
    fake.insert(BUCKET, "a (2).txt", b"alt");
    let (outcome, asked) = run_job(
        job(
            TransferOp::Copy,
            Side::Device,
            &[&source],
            storage(&fake, BUCKET),
            "/",
        ),
        vec![(ConflictChoice::KeepBoth, false)],
        None,
    )
    .await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(asked, ["a.txt"]);
    assert_eq!(fake.object(BUCKET, "a (3).txt").unwrap(), b"neu");
    assert_eq!(fake.object(BUCKET, "a.txt").unwrap(), b"alt");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_move_up_removes_the_local_source() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("Ordner/tief")).unwrap();
    std::fs::write(real.join("Ordner/tief/a.txt"), "a").unwrap();
    let source = real.join("Ordner").to_string_lossy().into_owned();
    let fake = Arc::new(FakeStore::new());
    let (outcome, _) = run_job(
        job(
            TransferOp::Move,
            Side::Device,
            &[&source],
            storage(&fake, BUCKET),
            "/",
        ),
        vec![],
        None,
    )
    .await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(fake.keys(BUCKET), ["Ordner/tief/a.txt"]);
    assert!(!real.join("Ordner").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_into_itself_in_one_storage_is_refused() {
    let fake = Arc::new(FakeStore::new());
    fake.insert(BUCKET, "Alt/a.txt", b"a");
    let job = job(
        TransferOp::Copy,
        storage(&fake, BUCKET),
        &["/Alt"],
        storage(&fake, BUCKET),
        "/Alt/tief",
    );
    let result = tokio::task::spawn_blocking(move || {
        prepare_remote(&job, &tokio::runtime::Handle::current(), |_| None).map(|_| ())
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().code, FilesErrorCode::IntoItself);
}

#[tokio::test(flavor = "multi_thread")]
async fn too_little_space_for_a_download_is_refused() {
    let (_dir, real) = folder();
    let fake = Arc::new(FakeStore::new());
    fake.insert(BUCKET, "gross.bin", &[0u8; 100]);
    let job = job(
        TransferOp::Copy,
        storage(&fake, BUCKET),
        &["/gross.bin"],
        Side::Device,
        &real.to_string_lossy(),
    );
    let result = tokio::task::spawn_blocking(move || {
        prepare_remote(&job, &tokio::runtime::Handle::current(), |_| Some(99)).map(|_| ())
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().code, FilesErrorCode::NoSpace);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upload_of_a_name_kept_for_encrypted_folders_is_refused() {
    let (_dir, real) = folder();
    let reserved = real.join("aaaaaaaaaaaaaaaaaaaaaaaaaa.hxef");
    std::fs::create_dir_all(&reserved).unwrap();
    std::fs::write(reserved.join("h"), "x").unwrap();
    let fake = Arc::new(FakeStore::new());
    let job = job(
        TransferOp::Copy,
        Side::Device,
        &[&reserved.to_string_lossy()],
        storage(&fake, BUCKET),
        "/",
    );
    let result = tokio::task::spawn_blocking(move || {
        prepare_remote(&job, &tokio::runtime::Handle::current(), |_| None).map(|_| ())
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().code, FilesErrorCode::InvalidName);
    assert!(fake.keys(BUCKET).is_empty());
}
