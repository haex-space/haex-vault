//! Tests for the profile in the encrypted folder (spec 065, T021) against real temporary vaults and
//! a device folder.

use std::path::Path;
use std::sync::Arc;

use super::profile_store::{dir, ProfileStore};
use crate::files::encrypted::test_support::give_key;
use crate::remote_storage::test_support::vault;
use crate::vault_gate::VaultDb;

/// A cookie value no encrypted byte would contain by chance.
const MARKER: &[u8] = b"session=holzi-agent-browser-marker-7f3a";

fn store(dir: &Path, db: &VaultDb) -> ProfileStore {
    let db = db.clone();
    ProfileStore::new(dir, Arc::new(move || Ok(db.clone()))).expect("store")
}

/// Every byte of every file below `dir`.
fn all_bytes(dir: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else {
                bytes.extend(std::fs::read(entry.path()).unwrap());
            }
        }
    }
    bytes
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_profile_round_trips_through_the_encrypted_folder() {
    let (_vault, db) = vault();
    give_key(&db, 1);
    let data = tempfile::tempdir().unwrap();
    let dir = dir(data.path(), "anna");
    let first = store(&dir, &db);
    assert_eq!(
        first.load().await.unwrap(),
        None,
        "nothing before the first end"
    );

    first.save(MARKER).await.unwrap();
    assert_eq!(first.load().await.unwrap().as_deref(), Some(MARKER));
    assert!(
        !all_bytes(&data.path().join("agent-browser"))
            .windows(MARKER.len())
            .any(|window| window == MARKER),
        "no plaintext of the profile on the disk"
    );

    // A new store (the next start) reads the same.
    let again = store(&dir, &db);
    assert_eq!(again.load().await.unwrap().as_deref(), Some(MARKER));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_end_replaces_the_profile() {
    let (_vault, db) = vault();
    give_key(&db, 1);
    let data = tempfile::tempdir().unwrap();
    let profile = store(&dir(data.path(), "anna"), &db);
    profile.save(b"first run").await.unwrap();
    profile.save(b"second run").await.unwrap();
    assert_eq!(
        profile.load().await.unwrap().as_deref(),
        Some(&b"second run"[..])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vault_without_the_key_cannot_read_the_profile() {
    let (_vault, db) = vault();
    give_key(&db, 1);
    let data = tempfile::tempdir().unwrap();
    let dir = dir(data.path(), "anna");
    store(&dir, &db).save(MARKER).await.unwrap();

    let (_other, other_db) = vault();
    give_key(&other_db, 1);
    let read = store(&dir, &other_db).load().await;
    assert!(
        !matches!(read, Ok(Some(_))),
        "another vault never reads the profile: {read:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_instance_has_its_own_profile() {
    let (_vault, db) = vault();
    give_key(&db, 1);
    let data = tempfile::tempdir().unwrap();
    store(&dir(data.path(), "anna"), &db)
        .save(MARKER)
        .await
        .unwrap();
    assert_eq!(
        store(&dir(data.path(), "ben"), &db).load().await.unwrap(),
        None
    );
}
