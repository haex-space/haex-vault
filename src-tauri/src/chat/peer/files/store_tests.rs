//! Chat files in the encrypted folder (spec 072 T062) against a real temporary vault: a file comes
//! back as stored, nothing of it lies on the disk in plaintext, and a missing one is `None`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::{dir, ChatFileStore};
use crate::files::encrypted::test_support::give_key;
use crate::remote_storage::test_support::vault;
use crate::vault_gate::VaultDb;

const MARKER: &[u8] = b"chat-file-marker-5d1c-never-in-plaintext";

fn store(dir: &Path, db: &VaultDb) -> ChatFileStore {
    let db = db.clone();
    ChatFileStore::new(dir, Arc::new(move || Ok(db.clone()))).expect("store")
}

fn all_bytes(dir: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).expect("dir").flatten() {
            if entry.file_type().expect("type").is_dir() {
                pending.push(entry.path());
            } else {
                bytes.extend(std::fs::read(entry.path()).expect("file"));
            }
        }
    }
    bytes
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_round_trips_encrypted_and_a_missing_one_is_none() {
    let (_vault, db) = vault();
    give_key(&db, 1);
    let data = tempfile::tempdir().expect("tempdir");
    let files = store(&dir(data.path(), "anna"), &db);
    assert_eq!(files.get("a1").await.expect("get"), None);

    files.put("a1", MARKER).await.expect("put");
    files.put("a2", b"second").await.expect("put");
    assert_eq!(files.get("a1").await.expect("get").as_deref(), Some(MARKER));
    assert_eq!(
        files.get("a2").await.expect("get").as_deref(),
        Some(&b"second"[..])
    );
    assert_eq!(files.get("a3").await.expect("get"), None);
    let folder = dir(data.path(), "anna");
    assert!(
        !all_bytes(&folder).is_empty(),
        "the files lie in the vault folder"
    );
    assert!(
        !all_bytes(&folder)
            .windows(MARKER.len())
            .any(|window| window == MARKER),
        "no plaintext on the disk"
    );
    let again = store(&dir(data.path(), "anna"), &db);
    assert_eq!(again.get("a1").await.expect("get").as_deref(), Some(MARKER));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_writers_of_one_attachment_take_turns_and_other_ones_do_not_wait() {
    let (_vault, db) = vault();
    give_key(&db, 1);
    let data = tempfile::tempdir().expect("tempdir");
    let files = store(&dir(data.path(), "anna"), &db);

    let first = files.writer("same").await.expect("first writer");
    let second = tokio::time::timeout(Duration::from_millis(300), files.writer("same")).await;
    assert!(second.is_err(), "the second writer waits for the first");
    let other = tokio::time::timeout(Duration::from_secs(10), files.writer("other"))
        .await
        .expect("another attachment does not wait")
        .expect("writer");
    other.abort().await;

    first.abort().await;
    let second = tokio::time::timeout(Duration::from_secs(10), files.writer("same"))
        .await
        .expect("the lock is free again")
        .expect("second writer");
    second.abort().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_removed_file_is_gone_and_removing_a_missing_one_is_fine() {
    let (_vault, db) = vault();
    give_key(&db, 1);
    let data = tempfile::tempdir().expect("tempdir");
    let files = store(&dir(data.path(), "anna"), &db);
    files.remove("never-there").await.expect("no folder yet");

    files.put("a1", b"first").await.expect("put");
    files.put("a2", b"second").await.expect("put");
    files.remove("a1").await.expect("remove");
    files.remove("a1").await.expect("removing twice");
    files.remove("a3").await.expect("a file that never was");
    assert_eq!(files.get("a1").await.expect("get"), None);
    assert_eq!(
        files.get("a2").await.expect("get").as_deref(),
        Some(&b"second"[..]),
        "another file stays"
    );
}
