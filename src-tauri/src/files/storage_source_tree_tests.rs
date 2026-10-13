//! A storage with real folders in the file browser (spec 050 US2, T039): an entry is found in the
//! listing of its folder, a folder is created, renamed and deleted with one call each, a name that
//! differs only in case counts as taken, and S3 keeps its ways object by object.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::*;
use crate::files::{EntryKind, FilesErrorCode};
use crate::remote_storage::test_support::{access, FakeStore, Op};
use crate::remote_storage::Protocol;

const FOLDER: &str = "Ablage";

fn scene(objects: &[&str]) -> (Arc<FakeStore>, StorageFiles) {
    let fake = Arc::new(FakeStore::new());
    for key in objects {
        fake.insert(FOLDER, key, key.as_bytes());
    }
    let mut access = access(FOLDER);
    access.location.protocol = Protocol::WebDav;
    let files = StorageFiles::new(fake.clone(), access);
    (fake, files)
}

fn ops(fake: &FakeStore) -> Vec<Op> {
    fake.calls().into_iter().map(|(op, _)| op).collect()
}

#[tokio::test]
async fn an_entry_is_found_in_the_listing_of_its_folder() {
    let (fake, files) = scene(&["Fotos/", "Fotos/a.jpg", "notiz.txt"]);
    let file = files.stat("/Fotos/a.jpg").await.unwrap();
    assert_eq!((file.kind, file.size), (EntryKind::File, Some(11)));
    assert_eq!(files.stat("/Fotos").await.unwrap().kind, EntryKind::Dir);
    assert_eq!(
        files.stat("/fehlt").await.unwrap_err().code,
        FilesErrorCode::NotFound
    );
    assert!(
        !ops(&fake).contains(&Op::Head),
        "a folder asked for without its / could be redirected"
    );
}

#[tokio::test]
async fn a_folder_is_created_with_one_put() {
    let (fake, files) = scene(&[]);
    files.create_folder("/", "Fotos").await.unwrap();
    assert_eq!(ops(&fake), [Op::ListDir, Op::Put]);
    assert!(fake.has_folder("Ablage/Fotos"));
}

#[tokio::test]
async fn a_folder_is_renamed_and_deleted_with_one_call_each() {
    let (fake, files) = scene(&["Fotos/", "Fotos/a.jpg", "Fotos/2026/b.jpg"]);
    let renamed = files.rename("/Fotos", "Bilder").await.unwrap();
    assert_eq!(renamed.path, "/Bilder");
    assert_eq!(
        fake.keys(FOLDER),
        ["Bilder/", "Bilder/2026/b.jpg", "Bilder/a.jpg"]
    );
    let calls = ops(&fake);
    assert_eq!(calls.iter().filter(|op| **op == Op::RenameTree).count(), 1);
    assert!(!calls.contains(&Op::Copy) && !calls.contains(&Op::Delete));

    let mut heard = Vec::new();
    files
        .delete("/Bilder", &CancellationToken::new(), &mut |n| heard.push(n))
        .await
        .unwrap();
    assert!(fake.keys(FOLDER).is_empty());
    assert_eq!(heard, [1], "one step");
    assert_eq!(
        ops(&fake)
            .iter()
            .filter(|op| **op == Op::RemoveTree)
            .count(),
        1
    );
}

#[tokio::test]
async fn a_file_is_renamed_with_one_move() {
    let (fake, files) = scene(&["notiz.txt"]);
    let renamed = files.rename("/notiz.txt", "brief.txt").await.unwrap();
    assert_eq!(
        (renamed.path.as_str(), renamed.kind),
        ("/brief.txt", EntryKind::File)
    );
    assert_eq!(fake.keys(FOLDER), ["brief.txt"]);
    assert!(!ops(&fake).contains(&Op::Copy));
}

#[tokio::test]
async fn a_name_that_differs_only_in_case_is_taken() {
    let (fake, files) = scene(&["Fotos/", "notiz.txt", "brief.txt"]);
    assert_eq!(
        files.create_folder("/", "fotos").await.unwrap_err().code,
        FilesErrorCode::Exists
    );
    assert_eq!(
        files
            .rename("/brief.txt", "NOTIZ.txt")
            .await
            .unwrap_err()
            .code,
        FilesErrorCode::Exists
    );
    assert!(files.key_taken("Notiz.TXT", None).await.unwrap());
    assert!(!files.key_taken("neu.txt", None).await.unwrap());
    assert!(!files.key_taken("Unbekannt/neu.txt", None).await.unwrap());
    assert_eq!(fake.keys(FOLDER), ["Fotos/", "brief.txt", "notiz.txt"]);
}

#[tokio::test]
async fn a_rename_that_changes_only_case_goes_through_a_passing_name() {
    let (fake, files) = scene(&["notiz.txt", "Fotos/", "Fotos/a.jpg"]);
    files.rename("/notiz.txt", "Notiz.txt").await.unwrap();
    files.rename("/Fotos", "FOTOS").await.unwrap();
    assert_eq!(fake.keys(FOLDER), ["FOTOS/", "FOTOS/a.jpg", "Notiz.txt"]);
    let moves: Vec<String> = fake
        .calls()
        .into_iter()
        .filter(|(op, _)| *op == Op::RenameTree)
        .map(|(_, key)| key)
        .collect();
    assert_eq!(moves.len(), 4, "two steps each: {moves:?}");
    assert!(moves[1].starts_with(".holzi-rename-"));
}

#[tokio::test]
async fn s3_keeps_renaming_and_deleting_object_by_object() {
    let fake = Arc::new(FakeStore::new());
    for key in ["Fotos/", "Fotos/a.jpg"] {
        fake.insert("b", key, b"x");
    }
    let files = StorageFiles::new(fake.clone(), access("b"));
    files.rename("/Fotos", "Bilder").await.unwrap();
    files
        .delete("/Bilder", &CancellationToken::new(), &mut |_| {})
        .await
        .unwrap();
    let calls = ops(&fake);
    assert!(calls.contains(&Op::Copy) && calls.contains(&Op::Delete));
    assert!(!calls.contains(&Op::RenameTree) && !calls.contains(&Op::RemoveTree));
}

/// Spec 050 US5 (T082): the same ways on a real SFTP server of the test process.
#[tokio::test(flavor = "multi_thread")]
async fn an_sftp_storage_takes_the_same_ways_in_the_file_browser() {
    use crate::remote_storage::sftp::test_server::{self, Options};
    use crate::remote_storage::sftp::SftpStore;
    use crate::remote_storage::test_support::resolver;
    use crate::remote_storage::{Credentials, PasswordCredentials};

    let server = test_server::start(Options::new(33)).await;
    std::fs::create_dir_all(server.path("Ablage/Fotos/2026")).unwrap();
    std::fs::write(server.path("Ablage/Fotos/2026/a.jpg"), b"a").unwrap();
    let mut access = access(FOLDER);
    access.location.protocol = Protocol::Sftp;
    access.location.endpoint = server.endpoint();
    access.location.host_key = Some(server.host_key.clone());
    access.credentials = Credentials::Password(PasswordCredentials {
        username: "anna".to_owned(),
        password: zeroize::Zeroizing::new("password".to_owned()),
    });
    let files = StorageFiles::new(Arc::new(SftpStore::new(resolver(&[]))), access);

    assert_eq!(files.stat("/Fotos").await.unwrap().kind, EntryKind::Dir);
    files.create_folder("/", "Neu").await.unwrap();
    assert_eq!(
        files.create_folder("/", "neu").await.unwrap_err().code,
        FilesErrorCode::Exists,
        "a name that differs only in case is taken"
    );
    files.rename("/Fotos", "Bilder").await.unwrap();
    assert!(server.path("Ablage/Bilder/2026/a.jpg").exists());
    files.rename("/Neu", "NEU").await.unwrap();
    assert!(server.path("Ablage/NEU").is_dir(), "only the case changed");
    let mut heard = Vec::new();
    files
        .delete("/Bilder", &CancellationToken::new(), &mut |n| heard.push(n))
        .await
        .unwrap();
    assert!(!server.path("Ablage/Bilder").exists());
    assert_eq!(heard, [1]);
    let listed = files.list("/").await.unwrap();
    assert_eq!(
        listed.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        ["NEU"]
    );
}
