//! Encrypted folders on this device (spec 048 US7, T073): a temporary folder of the real file system
//! is the device, seen through the local store with the id `device:/`.

use std::path::Path;

use tokio_util::sync::CancellationToken;

use super::*;
use crate::files::encrypted::device_scene::{walkdir, DeviceScene as Device};
use crate::files::encrypted::ops::create_encrypted_folder;
use crate::files::EncryptedState;
use crate::vault_gate::VaultDb;

#[cfg(unix)]
#[test]
fn the_device_root_is_slash_and_its_view_paths_are_device_paths() {
    let root = DeviceRoot::of(Path::new("/home/anna")).unwrap();
    assert_eq!(root.storage_id, "device:/");
    assert_eq!(
        root.view_of(Path::new("/home/anna/Unterlagen")),
        "/home/anna/Unterlagen"
    );
    assert_eq!(root.view_of(Path::new("/")), "/");
    assert_eq!(
        root.device_of("/home/anna/Unterlagen"),
        "/home/anna/Unterlagen"
    );
    assert_eq!(DeviceRoot::of(Path::new("relativ/pfad")), None);
}

#[cfg(windows)]
#[test]
fn a_windows_drive_is_its_own_storage() {
    let root = DeviceRoot::of(Path::new(r"c:\Users\anna")).unwrap();
    assert_eq!(root.storage_id, "device:C:");
    assert_eq!(root.view_of(Path::new(r"C:\Users\anna")), "/Users/anna");
    assert_eq!(root.device_of("/Users/anna"), r"C:\Users\anna");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_by_the_decrypted_name_leads_into_the_folder() {
    let device = Device::new();
    let raw = device.with_folder().await;
    let files = device.files();
    let inside = device.home.path().join("Unterlagen/notiz.txt");
    assert_eq!(
        route_device(&files, &device.root, &inside).await.unwrap(),
        Some(device.view("Unterlagen/notiz.txt"))
    );
    let entries = files.list(&device.view("Unterlagen")).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "notiz.txt");
    assert_eq!(
        device.root.entry(entries[0].clone()).path,
        inside.to_string_lossy()
    );
    // What exists on the disk is ordinary; a raw path through the directory is no window path.
    assert_eq!(
        route_device(&files, &device.root, device.home.path())
            .await
            .unwrap(),
        None
    );
    let missing = device.home.path().join("fehlt/datei");
    assert_eq!(
        route_device(&files, &device.root, &missing).await.unwrap(),
        None
    );
    let raw_path = device.home.path().join(&raw).join("h");
    assert_eq!(
        route_device(&files, &device.root, &raw_path)
            .await
            .unwrap_err()
            .code,
        FilesErrorCode::NotFound
    );
    // On the disk there is only the format: no name, no content in plain text.
    let mut stored = Vec::new();
    for entry in walkdir(&device.home.path().join(&raw)) {
        let bytes = std::fs::read(&entry).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("Strom"));
        assert!(!String::from_utf8_lossy(&bytes).contains("notiz"));
        stored.push(entry);
    }
    assert_eq!(stored.len(), 3, "h, one sidecar, one content: {stored:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_listing_names_the_folder_by_its_head() {
    let device = Device::new();
    device.with_folder().await;
    std::fs::write(device.home.path().join("offen.txt"), "x").unwrap();
    let named = name_encrypted(
        &device.files(),
        &device.root,
        device.home.path(),
        device.listed(),
    )
    .await
    .unwrap();
    let mut names: Vec<&str> = named.iter().map(|entry| entry.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["Unterlagen", "offen.txt"]);
    let folder = named
        .iter()
        .find(|entry| entry.name == "Unterlagen")
        .unwrap();
    assert_eq!(
        folder.path,
        device.home.path().join("Unterlagen").to_string_lossy()
    );
    let info = folder.encrypted.unwrap();
    assert_eq!(info.state, EncryptedState::Readable);
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_new_is_encrypted_inside_an_encrypted_folder() {
    let device = Device::new();
    device.with_folder().await;
    let error = create_encrypted_folder(&device.files(), &device.view("Unterlagen"), "Innen")
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::InEncrypted);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_inside_removes_the_objects_for_good() {
    let device = Device::new();
    let raw = device.with_folder().await;
    let files = device.files();
    let cancel = CancellationToken::new();
    files
        .delete(&device.view("Unterlagen/notiz.txt"), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert_eq!(
        walkdir(&device.home.path().join(&raw)).len(),
        1,
        "only the head stays"
    );
    files
        .delete(&device.view("Unterlagen"), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert!(
        !device.home.path().join(&raw).exists(),
        "no empty directory stays"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cache_forgets_a_folder_whose_directory_is_gone() {
    let device = Device::new();
    let raw = device.with_folder().await;
    let count = |db: &VaultDb| {
        db.read_blocking(|q| {
            use crate::storage::query::Query;
            q.query_map(
                "SELECT (SELECT COUNT(*) FROM encrypted_folder_heads_no_sync) \
                 + (SELECT COUNT(*) FROM encrypted_folder_entries_no_sync)",
                &[],
                |r| r.get::<_, i64>(0),
            )
        })
        .unwrap()[0]
    };
    let files = device.files();
    name_encrypted(&files, &device.root, device.home.path(), device.listed())
        .await
        .unwrap();
    files.list(&device.view("Unterlagen")).await.unwrap();
    assert!(count(&device.db) > 0);
    // Another program removes the directory.
    std::fs::remove_dir_all(device.home.path().join(&raw)).unwrap();
    let named = name_encrypted(&files, &device.root, device.home.path(), device.listed())
        .await
        .unwrap();
    assert!(named.is_empty());
    assert_eq!(count(&device.db), 0);
}
