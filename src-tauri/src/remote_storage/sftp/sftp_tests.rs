//! The SFTP store against a server in the test process (spec 050 US4, T066): files through a
//! temporary file and a rename over an existing file (with and without `posix-rename`), reads,
//! ranges, listings that look like S3's, uploads in parts with a retried part, a cancel that
//! leaves nothing, and a missing folder of the storage that is never made.

use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::time::Instant;

use super::connect_tests::{access, password};
use super::test_server::{self, Options, TestServer};
use super::SftpStore;
use crate::remote_storage::test_support::resolver;
use crate::remote_storage::{Access, RemoteStore, StorageError};

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

async fn scene(posix_rename: bool) -> (TestServer, SftpStore, Access) {
    let mut options = Options::new(7);
    options.posix_rename = posix_rename;
    let server = test_server::start(options).await;
    std::fs::create_dir(server.path("Ablage")).expect("folder");
    let mut access = access(
        &server.endpoint(),
        Some(&server.host_key),
        password("password"),
    );
    access.location.bucket = "Ablage".to_owned();
    (server, SftpStore::new(resolver(&[])), access)
}

fn on_disk(server: &TestServer, relative: &str) -> Option<Vec<u8>> {
    std::fs::read(server.path(relative)).ok()
}

fn names_in(server: &TestServer, relative: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(server.path(relative))
        .expect("read")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn a_file_replaces_one_of_its_name_with_and_without_posix_rename() {
    for posix in [true, false] {
        let (server, store, access) = scene(posix).await;
        for body in [&b"eins"[..], b"zwei"] {
            store
                .put(&access, "a/b.txt", body.to_vec(), soon())
                .await
                .expect("put");
        }
        assert_eq!(
            on_disk(&server, "Ablage/a/b.txt"),
            Some(b"zwei".to_vec()),
            "{posix}"
        );
        assert_eq!(
            names_in(&server, "Ablage/a"),
            ["b.txt"],
            "no temporary file stays"
        );
        let read = store
            .get(&access, "a/b.txt", 100, soon())
            .await
            .expect("get");
        assert_eq!(read, b"zwei");
    }
}

#[tokio::test]
async fn heads_ranges_and_listings_read_as_on_s3() {
    let (server, store, access) = scene(true).await;
    let body: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    store
        .put(&access, "Film/f.mp4", body.clone(), soon())
        .await
        .expect("put");
    store
        .put(&access, "Leer/", Vec::new(), soon())
        .await
        .expect("folder");
    let head = store
        .head(&access, "Film/f.mp4", soon())
        .await
        .expect("head");
    assert_eq!(head.size, 5000);
    assert!(head.last_modified.expect("time").ends_with(" GMT"));
    assert_eq!(
        head.etag.as_deref().map(|e| e.starts_with("5000-")),
        Some(true)
    );
    assert_eq!(
        store.head(&access, "Film", soon()).await.err(),
        Some(StorageError::NotFound),
        "a folder is no object"
    );
    let mut reader = store
        .get_range(&access, "Film/f.mp4", 4000, 50, soon())
        .await
        .expect("range");
    let mut part = Vec::new();
    reader.read_to_end(&mut part).await.expect("read");
    assert_eq!(part, body[4000..4050]);
    let found: Vec<(String, u64)> = store
        .list(&access, "", 100, soon())
        .await
        .expect("list")
        .into_iter()
        .map(|o| (o.key, o.size))
        .collect();
    assert_eq!(
        found,
        [
            ("Film/".to_owned(), 0),
            ("Film/f.mp4".to_owned(), 5000),
            ("Leer/".to_owned(), 0)
        ]
    );
    let level = store
        .list_dir(&access, "", 100, soon())
        .await
        .expect("level");
    assert_eq!(level.prefixes, ["Film/", "Leer/"]);
    assert!(level.objects.is_empty());
    let _ = server;
}

#[tokio::test]
async fn an_upload_in_parts_lands_whole_and_a_retried_part_goes_to_its_place() {
    let (server, store, access) = scene(true).await;
    let id = store
        .begin_upload(&access, "gross.bin", soon())
        .await
        .expect("begin");
    let mut marks = Vec::new();
    for (number, part) in [(1u16, &b"aaaa"[..]), (2, b"bbbb"), (2, b"BBBB"), (3, b"cc")] {
        marks.push(
            store
                .upload_part(&access, "gross.bin", &id, number, part.to_vec(), soon())
                .await
                .expect("part"),
        );
    }
    store
        .finish_upload(&access, "gross.bin", &id, &marks, soon())
        .await
        .expect("finish");
    assert_eq!(
        on_disk(&server, "Ablage/gross.bin"),
        Some(b"aaaaBBBBcc".to_vec())
    );
    assert_eq!(names_in(&server, "Ablage"), ["gross.bin"]);
}

#[tokio::test]
async fn a_cancelled_upload_leaves_the_old_file_and_no_temporary_file() {
    let (server, store, access) = scene(false).await;
    store
        .put(&access, "f.bin", b"alt".to_vec(), soon())
        .await
        .expect("put");
    let id = store
        .begin_upload(&access, "f.bin", soon())
        .await
        .expect("begin");
    store
        .upload_part(&access, "f.bin", &id, 1, b"neu".to_vec(), soon())
        .await
        .expect("part");
    store
        .abort_upload(&access, "f.bin", &id, soon())
        .await
        .expect("abort");
    assert_eq!(on_disk(&server, "Ablage/f.bin"), Some(b"alt".to_vec()));
    assert_eq!(names_in(&server, "Ablage"), ["f.bin"]);
}

#[tokio::test]
async fn folders_move_and_go_with_one_call_and_a_full_one_is_no_marker() {
    let (server, store, access) = scene(true).await;
    for key in ["Fotos/a.jpg", "Fotos/2026/b.jpg"] {
        store
            .put(&access, key, b"x".to_vec(), soon())
            .await
            .expect("put");
    }
    store
        .rename_tree(&access, "Fotos/", "Archiv/Bilder/", soon())
        .await
        .expect("move");
    assert_eq!(
        on_disk(&server, "Ablage/Archiv/Bilder/2026/b.jpg"),
        Some(b"x".to_vec())
    );
    store
        .delete(&access, "Archiv/Bilder/", soon())
        .await
        .expect("delete");
    assert!(
        server.path("Ablage/Archiv/Bilder").exists(),
        "a full folder stays"
    );
    store
        .remove_tree(&access, "Archiv/", soon())
        .await
        .expect("remove");
    assert!(names_in(&server, "Ablage").is_empty());
    store
        .delete(&access, "fehlt.txt", soon())
        .await
        .expect("gone is fine");
}

#[tokio::test]
async fn a_missing_folder_of_the_storage_is_not_made() {
    let (server, store, mut access) = scene(true).await;
    access.location.bucket = "Fehlt".to_owned();
    assert_eq!(
        store
            .put(&access, "a/b.txt", b"x".to_vec(), soon())
            .await
            .err(),
        Some(StorageError::NotFound)
    );
    assert_eq!(
        store.head(&access, "", soon()).await.err(),
        Some(StorageError::NotFound)
    );
    assert!(!server.path("Fehlt").exists(), "FR-006");
    assert_eq!(
        store.get(&access, "../geheim", 10, soon()).await.err(),
        Some(StorageError::NotFound),
        "no way out of the folder"
    );
}

#[tokio::test]
async fn an_unknown_host_key_is_remembered_for_the_settings() {
    let (server, store, mut access) = scene(true).await;
    access.location.host_key = None;
    assert_eq!(
        store.head(&access, "", soon()).await.err(),
        Some(StorageError::HostKeyUnknown)
    );
    assert_eq!(
        store.shown_host_key(&access.location),
        Some(server.host_key.clone())
    );
    assert!(!store.copies_on_provider(&access));
}
