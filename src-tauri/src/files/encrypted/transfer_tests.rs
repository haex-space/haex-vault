use std::path::{Path, PathBuf};

use super::*;
use crate::files::encrypted::ops::create_encrypted_folder;
use crate::files::encrypted::route::{open, route, Route};
use crate::files::encrypted::test_support::{transfer, Scene, BUCKET};
use crate::files::transfer::local::Outcome;
use crate::files::transfer::remote::{Side, PART};
use crate::files::transfer::{ConflictChoice, TransferOp};
use crate::remote_storage::test_support::Op;

fn device() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    (dir, real)
}

fn side(files: StorageFiles) -> Side {
    Side::Storage(Arc::new(files))
}

fn under(scene: &Scene, part: &str) -> Vec<String> {
    scene
        .keys()
        .into_iter()
        .filter(|key| key.contains(part))
        .collect()
}

async fn upload(scene: &Scene, path: &Path, target: &str) -> Outcome {
    transfer(
        TransferOp::Copy,
        Side::Device,
        &[path.to_string_lossy().into_owned()],
        side(scene.files()),
        target,
        None,
        None,
    )
    .await
    .unwrap()
}

async fn download(scene: &Scene, path: &str, to: &Path) -> Outcome {
    transfer(
        TransferOp::Copy,
        side(scene.fresh_files()),
        &[path.to_owned()],
        Side::Device,
        &to.to_string_lossy(),
        None,
        None,
    )
    .await
    .unwrap()
}

async fn with_folder() -> Scene {
    let scene = Scene::new();
    create_encrypted_folder(&scene.files(), "/", "Unterlagen")
        .await
        .unwrap();
    scene
}

#[tokio::test(flavor = "multi_thread")]
async fn up_writes_content_then_sidecar_and_down_gives_the_same_bytes() {
    let scene = with_folder().await;
    let (_dir, real) = device();
    std::fs::create_dir_all(real.join("Fotos/2026")).unwrap();
    std::fs::write(real.join("Fotos/2026/notiz.txt"), "Strom 2026").unwrap();
    assert_eq!(
        upload(&scene, &real.join("Fotos"), "/Unterlagen").await,
        Outcome::Done
    );

    let calls = scene.fake.calls();
    let content = calls
        .iter()
        .position(|(op, key)| *op == Op::Put && key.contains(".hxef/c/"))
        .unwrap();
    let sidecar = calls
        .iter()
        .rposition(|(op, key)| *op == Op::Put && key.contains(".hxef/m/"))
        .unwrap();
    assert!(
        content < sidecar,
        "the content is complete before its sidecar"
    );
    assert!(scene.keys().iter().all(|key| !key.contains("notiz")));

    let listed = scene
        .fresh_files()
        .list("/Unterlagen/Fotos/2026")
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "notiz.txt");
    assert_eq!(listed[0].size, Some(10));

    let (_out, back) = device();
    let outcome = download(&scene, "/Unterlagen/Fotos", &back).await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(
        std::fs::read_to_string(back.join("Fotos/2026/notiz.txt")).unwrap(),
        "Strom 2026"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_sidecar_or_a_cancel_leaves_nothing_visible() {
    let scene = with_folder().await;
    let (_dir, real) = device();
    let big = real.join("gross.bin");
    std::fs::write(&big, vec![7u8; PART + 1000]).unwrap();
    scene
        .fake
        .fail(Op::Put, crate::remote_storage::StorageError::AccessDenied);
    let outcome = upload(&scene, &big, "/Unterlagen").await;
    assert!(matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
    assert!(under(&scene, ".hxef/m/").is_empty());
    assert!(
        under(&scene, ".hxef/c/").is_empty(),
        "the content goes again"
    );
    scene.fake.heal(Op::Put);

    std::fs::write(&big, vec![7u8; 3 * PART]).unwrap();
    let outcome = transfer(
        TransferOp::Copy,
        Side::Device,
        &[big.to_string_lossy().into_owned()],
        side(scene.files()),
        "/Unterlagen",
        None,
        Some(1),
    )
    .await
    .unwrap();
    assert_eq!(outcome, Outcome::Cancelled);
    assert!(under(&scene, ".hxef/m/").is_empty());
    assert!(
        scene.fake.open_uploads().is_empty(),
        "the upload was aborted"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_part_is_sent_again_with_the_same_bytes() {
    let scene = with_folder().await;
    let (_dir, real) = device();
    let content: Vec<u8> = (0..PART + 1000).map(|i| (i % 251) as u8).collect();
    std::fs::write(real.join("gross.bin"), &content).unwrap();
    scene
        .fake
        .fail_once(Op::UploadPart, crate::remote_storage::StorageError::Network);
    let outcome = upload(&scene, &real.join("gross.bin"), "/Unterlagen").await;
    assert_eq!(outcome, Outcome::Done);
    let parts = scene
        .fake
        .calls()
        .iter()
        .filter(|(op, _)| *op == Op::UploadPart)
        .count();
    assert_eq!(parts, 3, "two parts and one retry");
    // Re-sealed bytes would not open: the round trip proves the same ciphertext went again.
    let (_out, back) = device();
    let outcome = download(&scene, "/Unterlagen/gross.bin", &back).await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(std::fs::read(back.join("gross.bin")).unwrap(), content);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_part_that_keeps_failing_aborts_and_the_next_try_is_a_new_object() {
    let scene = with_folder().await;
    let (_dir, real) = device();
    std::fs::write(real.join("gross.bin"), vec![1u8; PART + 10]).unwrap();
    scene
        .fake
        .fail(Op::UploadPart, crate::remote_storage::StorageError::Network);
    let outcome = upload(&scene, &real.join("gross.bin"), "/Unterlagen").await;
    assert!(matches!(outcome, Outcome::Failed(_)));
    assert!(scene.fake.open_uploads().is_empty());
    assert!(under(&scene, ".hxef/m/").is_empty());
    let first: Vec<String> = scene
        .fake
        .calls()
        .iter()
        .filter(|(op, _)| *op == Op::BeginUpload)
        .map(|(_, key)| key.clone())
        .collect();
    scene.fake.heal(Op::UploadPart);
    assert_eq!(
        upload(&scene, &real.join("gross.bin"), "/Unterlagen").await,
        Outcome::Done
    );
    let content = under(&scene, ".hxef/c/");
    assert_eq!(content.len(), 1);
    assert!(!first.contains(&content[0]), "a new content object");
    let (a, b) = (SealedParts::fresh(PART), SealedParts::fresh(PART));
    assert_ne!(a.finish().1.cid, b.finish().1.cid);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_download_checks_the_sha256_of_the_sidecar() {
    let scene = with_folder().await;
    let (_dir, real) = device();
    std::fs::write(real.join("notiz.txt"), "echt").unwrap();
    upload(&scene, &real.join("notiz.txt"), "/Unterlagen").await;
    // A sidecar with another SHA-256 (sealed with the folder key, as only a writer can).
    let files = scene.files();
    let Route::Encrypted(at) = route(&files, "/Unterlagen/notiz.txt").await.unwrap() else {
        panic!("encrypted");
    };
    let folder = open(&files, &at).await.unwrap();
    let mut version = crate::files::encrypted::ops::file_at(&files, &at)
        .await
        .unwrap()
        .1;
    if let SidecarKind::File(facts) = &mut version.sidecar.kind {
        facts.sha256 = [0; 32];
    }
    let body = folder.seal(&version.sid, &version.sidecar).unwrap();
    scene
        .fake
        .insert(BUCKET, &folder.sidecar_key(&version.sid), &body);

    let (_out, back) = device();
    let outcome = download(&scene, "/Unterlagen/notiz.txt", &back).await;
    let Outcome::Failed(error) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(error.code, FilesErrorCode::Damaged);
    assert!(
        std::fs::read_dir(&back).unwrap().next().is_none(),
        "no part file"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn copies_from_ordinary_folders_are_sealed_on_the_way() {
    let scene = with_folder().await;
    scene.fake.insert(BUCKET, "offen.txt", b"offen");
    let copy = |from: &str, to: &str| {
        let (from, to) = (from.to_owned(), to.to_owned());
        let files = (scene.files(), scene.files());
        async move {
            transfer(
                TransferOp::Copy,
                side(files.0),
                &[from],
                side(files.1),
                &to,
                None,
                None,
            )
            .await
            .unwrap()
        }
    };
    assert_eq!(copy("/offen.txt", "/Unterlagen").await, Outcome::Done);
    assert!(!scene.fake.calls().iter().any(|(op, _)| *op == Op::Copy));
    assert!(!scene
        .keys()
        .iter()
        .any(|key| key.contains(".hxef/") && key.contains("offen")));
    let (_out, back) = device();
    download(&scene, "/Unterlagen/offen.txt", &back).await;
    assert_eq!(std::fs::read(back.join("offen.txt")).unwrap(), b"offen");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_taken_name_is_replaced_after_the_new_version_or_kept_beside() {
    let scene = with_folder().await;
    let (_dir, real) = device();
    let file = real.join("notiz.txt");
    std::fs::write(&file, "eins").unwrap();
    upload(&scene, &file, "/Unterlagen").await;
    std::fs::write(&file, "zwei!").unwrap();
    let send = |answer| {
        let (files, source) = (scene.files(), file.to_string_lossy().into_owned());
        async move {
            transfer(
                TransferOp::Copy,
                Side::Device,
                &[source],
                side(files),
                "/Unterlagen",
                Some(answer),
                None,
            )
            .await
            .unwrap()
        }
    };
    assert_eq!(send(ConflictChoice::Replace).await, Outcome::Done);
    assert_eq!(
        under(&scene, ".hxef/c/").len(),
        1,
        "the old content is gone"
    );
    assert_eq!(
        under(&scene, ".hxef/m/").len(),
        1,
        "the old sidecar is gone"
    );
    let listed = scene.fresh_files().list("/Unterlagen").await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].size, Some(5));

    assert_eq!(send(ConflictChoice::KeepBoth).await, Outcome::Done);
    let mut names: Vec<String> = scene
        .fresh_files()
        .list("/Unterlagen")
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    names.sort();
    assert_eq!(names, ["notiz (2).txt", "notiz.txt"]);
}

/// The file `name` below the folder `Unterlagen`, as a stream put sees it.
async fn put_at(scene: &Scene, name: &str) -> Located {
    let Route::Encrypted(at) = route(&scene.files(), &format!("/Unterlagen/{name}"))
        .await
        .unwrap()
    else {
        panic!("the folder is encrypted");
    };
    at
}

fn bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 % 251) as u8).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stream_put_sends_parts_before_the_commit_and_shows_the_same_bytes() {
    let scene = with_folder().await;
    let data = bytes(2 * PART + 12_345);
    let files = scene.files();
    let at = put_at(&scene, "gross.bin").await;

    let mut put = PutStream::begin(&files, &at).await.unwrap();
    for piece in data.chunks(1024 * 1024) {
        put.push(piece).await.unwrap();
    }
    // Two full parts are at the storage already: the file was never held whole.
    let calls = scene.fake.calls();
    let parts = calls.iter().filter(|(op, _)| *op == Op::UploadPart).count();
    assert!(parts >= 2, "{calls:?}");
    assert!(
        under(&scene, ".hxef/m/").is_empty(),
        "nothing shows before the commit"
    );
    let listed = scene.fresh_files().list("/Unterlagen").await.unwrap();
    assert!(listed.is_empty());

    put.commit().await.unwrap();
    assert!(!under(&scene, ".hxef/m/").is_empty());

    let (_dir, back) = device();
    assert_eq!(
        download(&scene, "/Unterlagen/gross.bin", &back).await,
        Outcome::Done
    );
    assert_eq!(std::fs::read(back.join("gross.bin")).unwrap(), data);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stream_put_that_fits_one_part_is_one_request_like_put_file() {
    let scene = with_folder().await;
    let files = scene.files();
    let at = put_at(&scene, "klein.txt").await;
    let mut put = PutStream::begin(&files, &at).await.unwrap();
    put.push(b"Strom ").await.unwrap();
    put.push(b"2026").await.unwrap();
    put.commit().await.unwrap();
    let calls = scene.fake.calls();
    assert!(!calls.iter().any(|(op, _)| *op == Op::BeginUpload));

    let (_dir, back) = device();
    download(&scene, "/Unterlagen/klein.txt", &back).await;
    assert_eq!(
        std::fs::read_to_string(back.join("klein.txt")).unwrap(),
        "Strom 2026"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_aborted_stream_put_leaves_no_part_and_no_file() {
    let scene = with_folder().await;
    let files = scene.files();
    let at = put_at(&scene, "weg.bin").await;
    let mut put = PutStream::begin(&files, &at).await.unwrap();
    put.push(&bytes(PART + 10)).await.unwrap();
    assert!(
        scene
            .fake
            .calls()
            .iter()
            .any(|(op, _)| *op == Op::UploadPart),
        "a part is at the storage"
    );
    put.abort().await;
    assert!(scene
        .fake
        .calls()
        .iter()
        .any(|(op, _)| *op == Op::AbortUpload));
    assert!(under(&scene, ".hxef/c/").is_empty());
    assert!(under(&scene, ".hxef/m/").is_empty());
    assert!(scene
        .fresh_files()
        .list("/Unterlagen")
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_commit_takes_the_parts_down_and_shows_nothing() {
    let scene = with_folder().await;
    let files = scene.files();
    let at = put_at(&scene, "kaputt.bin").await;
    let mut put = PutStream::begin(&files, &at).await.unwrap();
    put.push(&bytes(PART + 10)).await.unwrap();
    scene.fake.fail(
        Op::FinishUpload,
        crate::remote_storage::StorageError::AccessDenied,
    );
    assert!(put.commit().await.is_err());
    assert!(under(&scene, ".hxef/m/").is_empty());
    assert!(scene
        .fresh_files()
        .list("/Unterlagen")
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stream_put_that_is_dropped_aborts_its_upload() {
    let scene = with_folder().await;
    let files = scene.files();
    let at = put_at(&scene, "verloren.bin").await;
    let mut put = PutStream::begin(&files, &at).await.unwrap();
    put.push(&bytes(PART + 10)).await.unwrap();
    assert!(scene
        .fake
        .calls()
        .iter()
        .any(|(op, _)| *op == Op::UploadPart));
    // A cancelled caller drops the writer without `abort`.
    drop(put);
    for _ in 0..50 {
        if scene
            .fake
            .calls()
            .iter()
            .any(|(op, _)| *op == Op::AbortUpload)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        scene
            .fake
            .calls()
            .iter()
            .any(|(op, _)| *op == Op::AbortUpload),
        "the upload is aborted by the drop"
    );
    assert!(under(&scene, ".hxef/m/").is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_committed_stream_put_does_not_abort_when_it_is_dropped_later() {
    let scene = with_folder().await;
    let files = scene.files();
    let at = put_at(&scene, "fertig.bin").await;
    let mut put = PutStream::begin(&files, &at).await.unwrap();
    put.push(&bytes(PART + 10)).await.unwrap();
    put.commit().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!scene
        .fake
        .calls()
        .iter()
        .any(|(op, _)| *op == Op::AbortUpload));
    assert!(!under(&scene, ".hxef/m/").is_empty());
}
