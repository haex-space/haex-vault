//! Tests for the profile in the encrypted folder (spec 065, T021) against real temporary vaults and
//! a device folder.

use std::path::Path;
use std::sync::Arc;

use super::profile_store::{clear, dir, ProfileStore};
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
        !all_bytes(&dir).is_empty(),
        "the profile lies in the vault folder"
    );
    assert!(
        !all_bytes(&dir)
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

/// Spec 084 (T033): clearing the profile leaves the database and the other subfolders of the vault
/// folder alone.
#[test]
fn clearing_the_profile_leaves_the_rest_of_the_vault_folder() {
    let data = tempfile::tempdir().unwrap();
    let profile = dir(data.path(), "anna");
    let vault = profile.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(profile.join("Profil")).unwrap();
    std::fs::write(profile.join("Profil/profile.zip"), b"x").unwrap();
    std::fs::write(vault.join("vault.db"), b"database").unwrap();
    std::fs::create_dir_all(vault.join("chat-files")).unwrap();
    std::fs::write(vault.join("chat-files/f"), b"chat").unwrap();

    clear(&profile).unwrap();
    clear(&profile).unwrap();

    assert!(!profile.exists());
    assert_eq!(std::fs::read(vault.join("vault.db")).unwrap(), b"database");
    assert_eq!(std::fs::read(vault.join("chat-files/f")).unwrap(), b"chat");
}

/// Spec 084 (T042): a vault named like the old list folder no longer shares it with the tracker
/// lists, so clearing its profile leaves them alone.
#[test]
fn clearing_the_profile_of_a_vault_named_filters_keeps_the_tracker_lists() {
    let data = tempfile::tempdir().unwrap();
    let paths = crate::instances::vault_paths::VaultPaths::new(data.path());
    let profile = dir(data.path(), "filters");
    std::fs::create_dir_all(profile.join("Profil")).unwrap();
    std::fs::write(profile.join("Profil/profile.zip"), b"x").unwrap();
    std::fs::create_dir_all(paths.filter_lists()).unwrap();
    std::fs::write(paths.filter_lists().join("easylist.txt"), b"list").unwrap();
    std::fs::write(paths.filter_lists().join("settings.json"), b"settings").unwrap();

    clear(&profile).unwrap();

    assert!(!profile.exists());
    assert_eq!(
        std::fs::read(paths.filter_lists().join("easylist.txt")).unwrap(),
        b"list"
    );
    assert_eq!(
        std::fs::read(paths.filter_lists().join("settings.json")).unwrap(),
        b"settings"
    );
}
