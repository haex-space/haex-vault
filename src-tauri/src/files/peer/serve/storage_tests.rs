//! A contact opens a share of a storage (spec 056 T081, US2 scenario 7): the serving device opens
//! the storage and its encrypted folders, the contact sees plain names and plain text, writes into
//! both at level read and write, and never gets past the share's root.

use std::sync::Arc;

use super::StorageOpener;
use crate::files::encrypted::test_support::{filled, tree, Scene, BUCKET, STORAGE};
use crate::files::peer::contact_tests::{contact_of, files_of, links_of};
use crate::files::peer::contacts;
use crate::files::peer::test_support::{store_card, Device};
use crate::files::share::{FileShareItemKind, FileShareLevel};
use crate::files::storage_source::StorageFiles;
use crate::files::{FilesErrorCode, SourceRef};
use crate::sync::test_support::Member;

/// The storage of `scene` as the serving device opens it.
fn opener(scene: &Arc<Scene>) -> StorageOpener {
    let scene = Arc::clone(scene);
    Arc::new(move |id: String| {
        let files = scene.files();
        Box::pin(async move {
            assert_eq!(id, STORAGE);
            Ok(files)
        })
    })
}

/// Anna shares `path` of her storage with Ben at `level`; Ben's view of the share.
async fn shared(
    scene: &Arc<Scene>,
    path: &str,
    kind: FileShareItemKind,
    level: FileShareLevel,
) -> (Device, StorageFiles) {
    let anna = Device::serving(Member::genesis(), Some(opener(scene))).await;
    let ben = Device::of(Member::genesis()).await;
    store_card(&anna, &ben).await;
    store_card(&ben, &anna).await;
    let source = SourceRef::Storage {
        storage_id: STORAGE.to_owned(),
    };
    let share = anna.share_of(source, path, kind, "Geteilt");
    anna.grant(&share, ben.member.vault, level);
    let links = links_of(&ben, &anna).await;
    let contact = contact_of(&anna, &ben).await;
    let found = contacts::query(&ben.db, &links, contact)
        .await
        .expect("query");
    let storage_id = found.shares[0].storage_id.clone();
    let (files, _) = files_of(&ben, links, &storage_id).await;
    (anna, files)
}

async fn text(files: &StorageFiles, path: &str) -> String {
    let entry = files.stat(path).await.expect("stat");
    let bytes = files
        .read_start(path, entry.size.unwrap_or(0), 1 << 20)
        .await
        .expect("read");
    String::from_utf8(bytes).expect("text")
}

fn names(entries: &[crate::files::Entry]) -> Vec<String> {
    let mut names: Vec<String> = entries.iter().map(|entry| entry.name.clone()).collect();
    names.sort();
    names
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_plain_storage_folder_is_read_and_written_below_its_root_only() {
    let scene = Arc::new(Scene::new());
    scene.fake.insert(BUCKET, "Fotos/a.txt", b"im Speicher");
    scene.fake.insert(BUCKET, "Privat/b.txt", b"nicht geteilt");
    let (_anna, files) = shared(
        &scene,
        "/Fotos",
        FileShareItemKind::Folder,
        FileShareLevel::ReadWrite,
    )
    .await;

    assert_eq!(names(&files.list("/").await.expect("root")), ["a.txt"]);
    assert_eq!(text(&files, "/a.txt").await, "im Speicher");
    assert!(files.list("/../Privat").await.is_err());

    files.create_folder("/", "Neu").await.expect("a folder");
    files
        .store()
        .put(
            files.access(),
            "Neu/c.txt",
            b"von Ben".to_vec(),
            super::soon(),
        )
        .await
        .expect("written");
    assert_eq!(
        scene.fake.object(BUCKET, "Fotos/Neu/c.txt").as_deref(),
        Some(&b"von Ben"[..])
    );
    files
        .rename("/a.txt", "umbenannt.txt")
        .await
        .expect("renamed");
    assert!(scene.fake.object(BUCKET, "Fotos/umbenannt.txt").is_some());
    files
        .delete("/umbenannt.txt", &Default::default(), &mut |_| {})
        .await
        .expect("deleted");
    assert!(scene.fake.object(BUCKET, "Fotos/umbenannt.txt").is_none());
    assert!(scene.fake.object(BUCKET, "Privat/b.txt").is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_encrypted_folder_is_served_as_plain_names_and_text() {
    let scene = Arc::new(filled(&tree(&[("Briefe/a.txt", "geheim"), ("b.txt", "zwei")])).await);
    let (_anna, files) = shared(
        &scene,
        "/Unterlagen",
        FileShareItemKind::Folder,
        FileShareLevel::ReadWrite,
    )
    .await;

    assert_eq!(
        names(&files.list("/").await.expect("root")),
        ["Briefe", "b.txt"]
    );
    assert_eq!(text(&files, "/Briefe/a.txt").await, "geheim");
    // Ben's side holds no key: what reaches it is the plain text.
    assert!(files.scope().is_none());

    files
        .store()
        .put(
            files.access(),
            "Briefe/c.txt",
            b"von Ben".to_vec(),
            super::soon(),
        )
        .await
        .expect("sealed on Anna's device");
    assert_eq!(
        text(&scene.files(), "/Unterlagen/Briefe/c.txt").await,
        "von Ben"
    );
    assert!(
        scene.keys().iter().all(|key| !key.contains("c.txt")),
        "no plain name reaches the bucket"
    );
    files
        .store()
        .put(files.access(), "b.txt", b"neu".to_vec(), super::soon())
        .await
        .expect("the next version");
    assert_eq!(text(&scene.files(), "/Unterlagen/b.txt").await, "neu");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_read_share_of_a_storage_takes_nothing() {
    let scene = Arc::new(Scene::new());
    scene.fake.insert(BUCKET, "Fotos/a.txt", b"im Speicher");
    let (_anna, files) = shared(
        &scene,
        "/Fotos/a.txt",
        FileShareItemKind::File,
        FileShareLevel::Read,
    )
    .await;
    assert_eq!(names(&files.list("/").await.expect("root")), ["a.txt"]);
    assert_eq!(text(&files, "/a.txt").await, "im Speicher");
    let refused = files
        .create_folder("/", "Neu")
        .await
        .expect_err("read only");
    assert_eq!(refused.code, FilesErrorCode::NoAccess);
    assert_eq!(scene.fake.keys(BUCKET), ["Fotos/a.txt"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_large_file_goes_into_an_encrypted_folder_in_parts_and_back_window_by_window() {
    use tokio::io::AsyncReadExt;

    let scene = Arc::new(filled(&tree(&[("b.txt", "zwei")])).await);
    let (_anna, files) = shared(
        &scene,
        "/Unterlagen",
        FileShareItemKind::Folder,
        FileShareLevel::ReadWrite,
    )
    .await;
    let body: Vec<u8> = (0..9 * 1024 * 1024 + 17).map(|i| (i % 251) as u8).collect();
    let mark = scene.fake.calls().len();
    files
        .store()
        .put(files.access(), "gross.bin", body.clone(), super::soon())
        .await
        .expect("sealed in parts");
    let parts = crate::files::encrypted::test_support::since(
        &scene,
        mark,
        &[crate::remote_storage::test_support::Op::UploadPart],
    );
    assert_eq!(parts.len(), 2, "a full part and the last one");

    let mut read = Vec::new();
    files
        .store()
        .get_range(
            files.access(),
            "gross.bin",
            0,
            body.len() as u64,
            super::soon(),
        )
        .await
        .expect("a reader")
        .read_to_end(&mut read)
        .await
        .expect("read");
    assert!(read == body, "the same bytes back");
    let mut tail = Vec::new();
    files
        .store()
        .get_range(
            files.access(),
            "gross.bin",
            8 * 1024 * 1024 - 3,
            10,
            super::soon(),
        )
        .await
        .expect("a range")
        .read_to_end(&mut tail)
        .await
        .expect("read");
    assert_eq!(tail, body[8 * 1024 * 1024 - 3..8 * 1024 * 1024 + 7]);
}
