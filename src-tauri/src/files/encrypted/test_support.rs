//! Shared setup for the tests of encrypted folders: a vault that holds content keys, a fake storage
//! and the window's view of it.

use std::sync::Arc;

use super::{EncryptedFolders, Scope};
use crate::files::storage_source::StorageFiles;
use crate::remote_storage::test_support::{access, vault, FakeStore};
use crate::sync::content_keys::ContentKey;
use crate::vault_gate::VaultDb;

pub const BUCKET: &str = "b";
pub const STORAGE: &str = "s1";

/// Stores a fresh content key of `generation` in `db`, as unwrapping its envelope would.
pub fn give_key(db: &VaultDb, generation: u64) -> ContentKey {
    let key = ContentKey::generate(generation);
    let (key_id, bytes) = (key.key_id, *key.key);
    db.write_blocking(move |tx| {
        tx.execute(
            "INSERT INTO vault_content_keys_no_sync (key_id, generation, key) VALUES (?1, ?2, ?3)",
            haex_crdt::rusqlite::params![
                key_id.as_slice(),
                i64::try_from(generation).expect("small generation"),
                bytes.as_slice()
            ],
        )?;
        Ok(())
    })
    .expect("store a content key");
    key
}

/// A vault with one content key, a fake storage, and the session's encrypted folders.
pub struct Scene {
    pub _dir: tempfile::TempDir,
    pub db: VaultDb,
    pub fake: Arc<FakeStore>,
    pub folders: Arc<EncryptedFolders>,
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

impl Scene {
    pub fn new() -> Self {
        let scene = Self::without_key();
        give_key(&scene.db, 1);
        scene
    }

    pub fn without_key() -> Self {
        let (dir, db) = vault();
        Self {
            _dir: dir,
            db,
            fake: Arc::new(FakeStore::new()),
            folders: Arc::default(),
        }
    }

    /// The storage as the window sees it.
    pub fn files(&self) -> StorageFiles {
        let db = self.db.clone();
        StorageFiles::new(self.fake.clone(), access(BUCKET)).with_scope(Scope {
            storage_id: STORAGE.to_owned(),
            folders: Arc::clone(&self.folders),
            vault: Arc::new(move || Ok(db.clone())),
        })
    }

    /// The same storage as another device of the vault sees it: nothing in memory yet.
    pub fn fresh_files(&self) -> StorageFiles {
        let db = self.db.clone();
        StorageFiles::new(self.fake.clone(), access(BUCKET)).with_scope(Scope {
            storage_id: STORAGE.to_owned(),
            folders: Arc::default(),
            vault: Arc::new(move || Ok(db.clone())),
        })
    }

    /// Every object key of the bucket.
    pub fn keys(&self) -> Vec<String> {
        self.fake.keys(BUCKET)
    }
}
