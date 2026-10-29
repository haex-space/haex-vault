//! The profile between two runs (spec 065, research R3, data-model.md "Profil zwischen zwei
//! Läufen"): `profile.zip` in the encrypted folder `Profil` of spec 048, inside the ordinary
//! device folder `<app data>/agent-browser/<instance>/`. The agent browser brings no encryption of
//! its own; without the open vault the folder cannot be read, and the file never leaves this
//! device's data directory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::files::encrypted::device::DeviceRoot;
use crate::files::encrypted::ops::{create_encrypted_folder, file_at};
use crate::files::encrypted::route::{route, Route};
use crate::files::encrypted::source::EncryptedFile;
use crate::files::encrypted::transfer::put_file;
use crate::files::encrypted::{EncryptedFolders, Scope, VaultSource};
use crate::files::storage_source::StorageFiles;
use crate::files::{FilesError, FilesErrorCode};
use crate::remote_storage::local::LocalStore;

use super::error::{BrowserError, Result};

/// The encrypted folder that holds the profile.
pub const FOLDER: &str = "Profil";
/// The profile file in it.
pub const FILE: &str = "profile.zip";

// ponytail: the whole ZIP is held in memory once per start and end (`put_file` seals it in one
// piece). Ceiling: a profile of a few hundred MB (big IndexedDB) costs that much memory for a
// moment. Upgrade path: the part-wise sealing of transfers (`SealedParts`) straight from the file.

/// The ordinary device folder of `instance` that holds the encrypted folder.
pub fn dir(app_local_data: &Path, instance: &str) -> PathBuf {
    app_local_data.join("agent-browser").join(instance)
}

/// The profile's place: the device as a storage, with the vault's keys.
pub struct ProfileStore {
    files: StorageFiles,
    root: DeviceRoot,
    dir: PathBuf,
}

impl ProfileStore {
    /// The store in `dir` (absolute), reading keys through `vault`.
    pub fn new(dir: &Path, vault: VaultSource) -> Result<Self> {
        let root = DeviceRoot::of(dir)
            .ok_or_else(|| BrowserError::Profile("the data folder is not absolute".to_owned()))?;
        let files = root.files(
            Arc::new(LocalStore::new()),
            Scope {
                storage_id: root.storage_id.clone(),
                folders: Arc::new(EncryptedFolders::default()),
                vault,
                background: Arc::new(|_| {}),
            },
        );
        Ok(Self {
            files,
            root,
            dir: dir.to_path_buf(),
        })
    }

    fn view(&self, rel: &[&str]) -> String {
        let mut path = self.dir.clone();
        path.extend(rel);
        self.root.view_of(&path)
    }

    /// The profile of the last orderly end; `None` before the first one. A profile that cannot be
    /// read (damaged, or the vault lacks its key) is an error, so the caller starts empty and
    /// says why.
    pub async fn load(&self) -> Result<Option<Vec<u8>>> {
        if !self.dir.is_dir() {
            return Ok(None);
        }
        let Route::Encrypted(at) = route(&self.files, &self.view(&[FOLDER, FILE]))
            .await
            .map_err(store_error)?
        else {
            return Ok(None);
        };
        let (folder, version) = match file_at(&self.files, &at).await {
            Ok(found) => found,
            Err(error) if error.code == FilesErrorCode::NotFound => return Ok(None),
            Err(error) => return Err(store_error(error)),
        };
        let file = EncryptedFile::open(&self.files, folder, &version)
            .await
            .map_err(store_error)?;
        file.read(0, file.size())
            .await
            .map(Some)
            .map_err(store_error)
    }

    /// Writes `zip` as the new version of the profile, making the folders on first use.
    pub async fn save(&self, zip: &[u8]) -> Result<()> {
        std::fs::create_dir_all(&self.dir).map_err(|e| BrowserError::Profile(e.to_string()))?;
        let folder = self.view(&[FOLDER]);
        if !matches!(
            route(&self.files, &folder).await.map_err(store_error)?,
            Route::Encrypted(_)
        ) {
            create_encrypted_folder(&self.files, &self.view(&[]), FOLDER)
                .await
                .map_err(store_error)?;
        }
        let Route::Encrypted(at) = route(&self.files, &self.view(&[FOLDER, FILE]))
            .await
            .map_err(store_error)?
        else {
            return Err(BrowserError::Profile(
                "the encrypted folder is gone".to_owned(),
            ));
        };
        put_file(&self.files, &at, zip).await.map_err(store_error)
    }
}

fn store_error(error: FilesError) -> BrowserError {
    BrowserError::Profile(error.message)
}
