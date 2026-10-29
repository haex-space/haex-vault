//! A storage folder as a place: the tree from its listings, a missing folder unreachable, another
//! own device that is off or refuses unreachable too, the rule trash, hashes read in parts and text
//! written through the transfers.

use std::sync::Arc;

use super::*;
use crate::remote_storage::test_support::{access, FakeStore, Op};
use crate::remote_storage::StorageError;

const BUCKET: &str = "b";

fn place(fake: &Arc<FakeStore>, root: &str) -> StoragePlace {
    StoragePlace::new(
        Arc::new(StorageFiles::new(fake.clone(), access(BUCKET))),
        root,
    )
}

fn filled() -> Arc<FakeStore> {
    let fake = Arc::new(FakeStore::new());
    fake.insert(BUCKET, "Fotos/a.jpg", b"aa");
    fake.insert(BUCKET, "Fotos/2026/b.jpg", b"bbb");
    fake.insert(BUCKET, "Musik/c.mp3", b"c");
    fake
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tree_lists_the_files_below_the_root() {
    let fake = filled();
    let listing = place(&fake, "/Fotos")
        .list_tree(&CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        listing.files.keys().collect::<Vec<_>>(),
        ["2026/b.jpg", "a.jpg"]
    );
    assert_eq!(listing.files["2026/b.jpg"].size, 3);
    assert_eq!(listing.folders.iter().collect::<Vec<_>>(), ["2026"]);
    let all = place(&fake, "/")
        .list_tree(&CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(all.files.len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_root_is_unreachable() {
    let fake = filled();
    let error = place(&fake, "/Gone")
        .list_tree(&CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Unreachable);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_that_is_off_or_refuses_is_unreachable() {
    // An own device that is off fails with `Network`, one whose file access is off with
    // `MissingRight`; both make the rule wait (FR-024), never a run against an empty listing.
    for error in [StorageError::Network, StorageError::MissingRight] {
        for root in ["/", "/Fotos"] {
            let fake = filled();
            for op in [Op::List, Op::ListDir, Op::Head] {
                fake.fail(op, error);
            }
            let failed = place(&fake, root)
                .list_tree(&CancellationToken::new())
                .await
                .unwrap_err();
            assert_eq!(
                failed.code,
                FilesErrorCode::Unreachable,
                "{error:?} at {root}"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn hash_reads_the_object() {
    let fake = Arc::new(FakeStore::new());
    fake.insert(BUCKET, "x/abc.txt", b"abc");
    assert_eq!(
        place(&fake, "/x")
            .hash("abc.txt", &CancellationToken::new())
            .await
            .unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn trash_moves_into_the_rule_trash() {
    let fake = filled();
    let p = place(&fake, "/Fotos");
    let trashed = p.trash("2026/b.jpg", "run1").await.unwrap();
    assert_eq!(
        trashed,
        Trashed::Rule(".holzi-trash/run1/2026/b.jpg".to_owned())
    );
    assert_eq!(
        fake.object(BUCKET, "Fotos/.holzi-trash/run1/2026/b.jpg")
            .as_deref(),
        Some(&b"bbb"[..])
    );
    assert_eq!(fake.object(BUCKET, "Fotos/2026/b.jpg"), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn rename_within_a_folder_and_across_folders() {
    let fake = filled();
    let p = place(&fake, "/Fotos");
    p.rename("a.jpg", "a2.jpg").await.unwrap();
    assert!(fake.object(BUCKET, "Fotos/a2.jpg").is_some());
    p.rename("a2.jpg", "neu/a3.jpg").await.unwrap();
    assert_eq!(
        fake.object(BUCKET, "Fotos/neu/a3.jpg").as_deref(),
        Some(&b"aa"[..])
    );
    assert_eq!(fake.object(BUCKET, "Fotos/a2.jpg"), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn text_round_trips_and_a_long_file_is_too_large() {
    let fake = filled();
    let p = place(&fake, "/Fotos");
    assert_eq!(p.read_text(".haex-ignore", 100).await.unwrap(), None);
    p.write_text(".haex-ignore", "*.tmp\n").await.unwrap();
    assert_eq!(
        p.read_text(".haex-ignore", 100).await.unwrap().as_deref(),
        Some("*.tmp\n")
    );
    p.write_text(".haex-ignore", "*.tmp\n*.bak\n")
        .await
        .unwrap();
    assert_eq!(
        p.read_text(".haex-ignore", 100).await.unwrap().as_deref(),
        Some("*.tmp\n*.bak\n"),
        "written over"
    );
    assert_eq!(
        p.read_text(".haex-ignore", 3).await.unwrap_err().code,
        FilesErrorCode::TooLarge
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn stat_and_delete() {
    let fake = filled();
    let p = place(&fake, "/Fotos");
    assert_eq!(p.stat("a.jpg").await.unwrap().unwrap().size, 2);
    p.delete("a.jpg").await.unwrap();
    assert_eq!(p.stat("a.jpg").await.unwrap(), None);
    assert_eq!(p.full_path("2026/b.jpg"), "/Fotos/2026/b.jpg");
}
