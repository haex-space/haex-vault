//! A temporary folder of the real file system as the device (spec 048 US7): a vault with a content
//! key and the device's storage view of it, for the tests of encrypted folders on the device.

use std::path::Path;
use std::sync::Arc;

use super::device::DeviceRoot;
use super::format::is_folder_prefix;
use super::ops::create_encrypted_folder;
use super::test_support::{give_key, transfer};
use super::{EncryptedFolders, Scope};
use crate::files::local::ops;
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::local::Outcome;
use crate::files::transfer::remote::Side;
use crate::files::transfer::TransferOp;
use crate::files::Entry;
use crate::remote_storage::local::LocalStore;
use crate::remote_storage::test_support::vault;
use crate::vault_gate::VaultDb;

/// The device with a vault that holds a content key.
pub struct DeviceScene {
    pub _vault: tempfile::TempDir,
    pub db: VaultDb,
    pub folders: Arc<EncryptedFolders>,
    pub home: tempfile::TempDir,
    pub root: DeviceRoot,
}

impl Default for DeviceScene {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceScene {
    pub fn new() -> Self {
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

    /// The same with the home folder as a drive of its own (`device:T:`), as on Windows: the
    /// window paths of its storage view are not its device paths.
    pub fn on_drive() -> Self {
        let mut scene = Self::new();
        scene.root = DeviceRoot {
            dir: std::fs::canonicalize(scene.home.path()).unwrap(),
            storage_id: "device:T:".to_owned(),
        };
        scene
    }

    /// The device as the window's storage view.
    pub fn files(&self) -> StorageFiles {
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

    pub fn view(&self, rel: &str) -> String {
        let home = std::fs::canonicalize(self.home.path()).unwrap();
        self.root.view_of(&home.join(rel))
    }

    /// "Unterlagen" in the home folder with `notiz.txt` copied in.
    pub async fn with_folder(&self) -> String {
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

    pub fn listed(&self) -> Vec<Entry> {
        ops::list(
            self.home.path(),
            &crate::files::local::places::OwnPlaces::default(),
        )
        .unwrap()
    }
}

/// The name of the one encrypted folder's directory in `dir`.
pub fn raw_dir(dir: &Path) -> String {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .find(|name| is_folder_prefix(name))
        .expect("an encrypted folder")
}

/// Every file below `dir`.
pub fn walkdir(dir: &Path) -> Vec<std::path::PathBuf> {
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
