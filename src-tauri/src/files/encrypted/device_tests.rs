//! Encrypted folders on this device (spec 048 US7, T073): a temporary folder of the real file system
//! is the device, seen through the local store with the id `device:/`.

use std::path::Path;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::*;
use crate::files::encrypted::ops::create_encrypted_folder;
use crate::files::encrypted::test_support::{give_key, transfer};
use crate::files::encrypted::{EncryptedFolders, Scope};
use crate::files::local::ops;
use crate::files::transfer::local::Outcome;
use crate::files::transfer::remote::Side;
use crate::files::transfer::TransferOp;
use crate::files::EncryptedState;
use crate::remote_storage::local::LocalStore;
use crate::remote_storage::test_support::vault;
use crate::vault_gate::VaultDb;

struct Device {
    _vault: tempfile::TempDir,
    db: VaultDb,
    folders: Arc<EncryptedFolders>,
    home: tempfile::TempDir,
    root: DeviceRoot,
}

impl Device {
    fn new() -> Self {
        let (dir, db) = vault();
        give_key(&db, 1);
        let home = tempfile::tempdir().unwrap();
        let root = DeviceRoot::of(home.path()).unwrap();
        Self {
            _vault: dir,
            db,
            folders: Arc::default(),
            home,
            root,
        }
    }

    /// The device as the window's storage view.
    fn files(&self) -> StorageFiles {
        let db = self.db.clone();
        self.root.files(
            Arc::new(LocalStore::new()),
            Scope {
                storage_id: self.root.storage_id.clone(),
                folders: Arc::clone(&self.folders),
                vault: Arc::new(move || Ok(db.clone())),
                background: Arc::new(|_| {}),
            },
        )
    }

    fn view(&self, rel: &str) -> String {
        self.root.view_of(&self.home.path().join(rel))
    }

    /// "Unterlagen" in the home folder with `notiz.txt` copied in.
    async fn with_folder(&self) -> String {
        create_encrypted_folder(&self.files(), &self.view(""), "Unterlagen")
            .await
            .unwrap();
        let source = tempfile::tempdir().unwrap();
        let file = source.path().join("notiz.txt");
        std::fs::write(&file, "Strom").unwrap();
        let outcome = transfer(
            TransferOp::Copy,
            Side::Device,
            &[file.to_string_lossy().into_owned()],
            Side::Storage(Arc::new(self.files())),
            &self.view("Unterlagen"),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(outcome, Outcome::Done);
        raw_dir(self.home.path())
    }

    fn listed(&self) -> Vec<Entry> {
        ops::list(
            self.home.path(),
            &crate::files::local::places::OwnPlaces::default(),
        )
        .unwrap()
    }
}

/// The name of the one encrypted folder's directory in `dir`.
fn raw_dir(dir: &Path) -> String {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .find(|name| is_folder_prefix(name))
        .expect("an encrypted folder")
}

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

fn walkdir(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(walkdir(&path));
        } else {
            files.push(path);
        }
    }
    files
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
    assert!(!device.home.path().join(&raw).join("h").exists());
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
