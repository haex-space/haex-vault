use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::*;
use crate::files::{EntryKind, FilesErrorCode};
use crate::remote_storage::test_support::{access, FakeStore, Op};
use crate::remote_storage::StorageError;

const BUCKET: &str = "b";

fn scene(objects: &[&str]) -> (Arc<FakeStore>, StorageFiles) {
    let fake = Arc::new(FakeStore::new());
    for key in objects {
        fake.insert(BUCKET, key, key.as_bytes());
    }
    let files = StorageFiles::new(fake.clone(), access(BUCKET));
    (fake, files)
}

fn names(entries: &[Entry]) -> Vec<(String, EntryKind)> {
    let mut out: Vec<_> = entries
        .iter()
        .map(|entry| (entry.name.clone(), entry.kind))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn paths_map_to_keys_and_back() {
    assert_eq!(folder_prefix("/").unwrap(), "");
    assert_eq!(folder_prefix("/Fotos/2026").unwrap(), "Fotos/2026/");
    assert_eq!(object_key("/Fotos/a.jpg").unwrap(), "Fotos/a.jpg");
    assert_eq!(path_of("Fotos/2026/"), "/Fotos/2026");
    assert_eq!(path_of("Fotos/a.jpg"), "/Fotos/a.jpg");
    for bad in ["", "Fotos", "/a//b", "/a/../b", "/a/./b", "/a/"] {
        assert!(object_key(bad).is_err(), "{bad:?}");
    }
    assert!(folder_prefix("/a/..").is_err());
}

#[tokio::test]
async fn prefixes_appear_as_folders_and_the_marker_is_hidden() {
    let (_fake, files) = scene(&["Fotos/", "Fotos/a.jpg", "Fotos/2026/b.jpg", "notiz.txt"]);
    let root = files.list("/").await.unwrap();
    assert_eq!(
        names(&root),
        [
            ("Fotos".to_owned(), EntryKind::Dir),
            ("notiz.txt".to_owned(), EntryKind::File)
        ]
    );
    let fotos = files.list("/Fotos").await.unwrap();
    assert_eq!(
        names(&fotos),
        [
            ("2026".to_owned(), EntryKind::Dir),
            ("a.jpg".to_owned(), EntryKind::File)
        ]
    );
    let file = fotos.iter().find(|e| e.name == "a.jpg").unwrap();
    assert_eq!(file.path, "/Fotos/a.jpg");
    assert_eq!(file.size, Some("Fotos/a.jpg".len() as u64));
    assert_eq!(file.mime.as_deref(), Some("image/jpeg"));
    assert!(file.modified_ms.is_some());
    assert!(!file.holzi_owned && !file.symlink && !file.no_access);
}

#[tokio::test]
async fn stat_tells_files_from_folders() {
    let (fake, files) = scene(&["Fotos/a.jpg", "leer/"]);
    let file = files.stat("/Fotos/a.jpg").await.unwrap();
    assert_eq!(file.kind, EntryKind::File);
    assert!(file.modified_ms.is_some(), "from the HEAD's date");
    assert!(fake.calls().contains(&(Op::Head, "Fotos/a.jpg".to_owned())));
    assert_eq!(files.stat("/Fotos").await.unwrap().kind, EntryKind::Dir);
    assert_eq!(files.stat("/leer").await.unwrap().kind, EntryKind::Dir);
    assert_eq!(files.stat("/").await.unwrap().kind, EntryKind::Dir);
    let missing = files.stat("/weg").await.unwrap_err();
    assert_eq!(missing.code, FilesErrorCode::NotFound);
}

#[tokio::test]
async fn a_new_folder_is_a_marker_and_a_taken_name_is_refused() {
    let (fake, files) = scene(&["Fotos/a.jpg", "notiz.txt"]);
    let entry = files.create_folder("/Fotos", "2026").await.unwrap();
    assert_eq!(entry.path, "/Fotos/2026");
    assert_eq!(fake.object(BUCKET, "Fotos/2026/"), Some(Vec::new()));
    for taken in ["notiz.txt", "Fotos"] {
        let error = files.create_folder("/", taken).await.unwrap_err();
        assert_eq!(error.code, FilesErrorCode::Exists, "{taken}");
    }
    let error = files.create_folder("/", "a/b").await.unwrap_err();
    assert_eq!(error.code, FilesErrorCode::InvalidName);
}

#[tokio::test]
async fn renaming_a_file_copies_and_deletes() {
    let (fake, files) = scene(&["Fotos/a.jpg", "Fotos/c.jpg"]);
    let entry = files.rename("/Fotos/a.jpg", "b.jpg").await.unwrap();
    assert_eq!(entry.path, "/Fotos/b.jpg");
    assert_eq!(fake.keys(BUCKET), ["Fotos/b.jpg", "Fotos/c.jpg"]);
    assert_eq!(fake.object(BUCKET, "Fotos/b.jpg").unwrap(), b"Fotos/a.jpg");
    let error = files.rename("/Fotos/b.jpg", "c.jpg").await.unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Exists);
}

#[tokio::test]
async fn renaming_a_folder_copies_every_object_under_it() {
    let (fake, files) = scene(&["Alt/", "Alt/a.txt", "Alt/tief/b.txt", "Altbau.txt"]);
    let entry = files.rename("/Alt", "Neu").await.unwrap();
    assert_eq!(entry.kind, EntryKind::Dir);
    assert_eq!(
        fake.keys(BUCKET),
        ["Altbau.txt", "Neu/", "Neu/a.txt", "Neu/tief/b.txt"]
    );
}

#[tokio::test]
async fn deleting_a_folder_deletes_every_object_and_then_the_marker() {
    let (fake, files) = scene(&["Alt/", "Alt/a.txt", "Alt/tief/b.txt", "bleibt.txt"]);
    let mut deleted = 0;
    files
        .delete("/Alt", &CancellationToken::new(), &mut |count| {
            deleted = count
        })
        .await
        .unwrap();
    assert_eq!(fake.keys(BUCKET), ["bleibt.txt"]);
    assert_eq!(deleted, 3);
    let deletes: Vec<_> = fake
        .calls()
        .into_iter()
        .filter(|(op, _)| *op == Op::Delete)
        .map(|(_, key)| key)
        .collect();
    assert_eq!(deletes.last().map(String::as_str), Some("Alt/"));
    files
        .delete("/bleibt.txt", &CancellationToken::new(), &mut |_| {})
        .await
        .unwrap();
    assert!(fake.keys(BUCKET).is_empty());
}

#[tokio::test]
async fn a_failed_delete_says_what_is_left() {
    let (fake, files) = scene(&["Alt/a.txt", "Alt/b.txt", "Alt/c.txt"]);
    fake.fail(Op::Delete, StorageError::MissingRight);
    let error = files
        .delete("/Alt", &CancellationToken::new(), &mut |_| {})
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::NoAccess);
    assert!(error.message.contains("3"), "{}", error.message);
    assert!(error.message.contains("Alt/a.txt"), "{}", error.message);
}

#[tokio::test]
async fn errors_name_the_kind_and_never_the_credentials() {
    let (fake, files) = scene(&["a.txt"]);
    for (failure, code) in [
        (StorageError::AccessDenied, FilesErrorCode::Credentials),
        (StorageError::MissingRight, FilesErrorCode::NoAccess),
        (StorageError::Network, FilesErrorCode::Unreachable),
        (StorageError::TimedOut, FilesErrorCode::Unreachable),
    ] {
        fake.fail_once(Op::ListDir, failure);
        let error = files.list("/").await.unwrap_err();
        assert_eq!(error.code, code, "{failure:?}");
        let text = format!("{error:?} {error}");
        assert!(!text.contains("placeholder-secret"), "{text}");
        assert!(!text.contains("AKIDEXAMPLE"), "{text}");
        assert!(!text.contains("127.0.0.1"), "{text}");
    }
}

#[tokio::test]
async fn the_name_form_of_encrypted_folders_is_refused_for_ordinary_entries() {
    let (fake, files) = scene(&["notiz.txt"]);
    let reserved = "aaaaaaaaaaaaaaaaaaaaaaaaaa.hxef";
    let error = files.create_folder("/", reserved).await.unwrap_err();
    assert_eq!(error.code, FilesErrorCode::InvalidName);
    let error = files.rename("/notiz.txt", reserved).await.unwrap_err();
    assert_eq!(error.code, FilesErrorCode::InvalidName);
    assert_eq!(fake.keys(BUCKET), ["notiz.txt"]);
    // A name that only looks close is an ordinary name.
    files
        .create_folder("/", "aaaaaaaaaaaaaaaaaaaaaaaaa.hxef")
        .await
        .unwrap();
}

#[tokio::test]
async fn a_flat_listing_is_one_request_and_names_the_folders_of_the_keys() {
    let (fake, files) = scene(&[
        "Fotos/",
        "Fotos/a.jpg",
        "Fotos/2026/05/b.jpg",
        "leer/",
        "n.txt",
    ]);
    let tree = files.list_flat("/").await.unwrap().unwrap();
    assert_eq!(
        tree.folders.iter().map(String::as_str).collect::<Vec<_>>(),
        ["Fotos", "Fotos/2026", "Fotos/2026/05", "leer"]
    );
    assert_eq!(
        tree.files.keys().map(String::as_str).collect::<Vec<_>>(),
        ["Fotos/2026/05/b.jpg", "Fotos/a.jpg", "n.txt"]
    );
    assert_eq!(tree.files["n.txt"].0, "n.txt".len() as u64);
    let ops: Vec<Op> = fake.calls().into_iter().map(|(op, _)| op).collect();
    assert_eq!(ops, [Op::List], "no request per folder");
    let below = files.list_flat("/Fotos").await.unwrap().unwrap();
    assert_eq!(
        below.files.keys().map(String::as_str).collect::<Vec<_>>(),
        ["2026/05/b.jpg", "a.jpg"]
    );
}

#[tokio::test]
async fn a_flat_listing_gives_way_to_the_folder_walk_for_encrypted_folders_and_odd_keys() {
    let (_fake, files) = scene(&["a/abcdefghijklmnopqrstuvwxyz.hxef/head", "n.txt"]);
    assert!(files.list_flat("/").await.unwrap().is_none());
    let (_fake, files) = scene(&["a//b.txt"]);
    assert!(files.list_flat("/").await.unwrap().is_none());
}
