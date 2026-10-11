//! Shared setup for the tests of encrypted folders: a vault that holds content keys, a fake storage
//! and the window's view of it, and a transfer run as the manager runs it.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::{EncryptedFolders, Scope};
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::local::{Hooks, Outcome, Removal};
use crate::files::transfer::remote::{prepare_remote, run_remote, RemoteJob, Side};
use crate::files::transfer::{ConflictChoice, TransferOp};
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

/// Runs a transfer on a blocking thread as the manager does; conflicts are answered `answer` (none
/// cancels), and `cancel_after` progress reports cancel it.
pub async fn transfer(
    op: TransferOp,
    from: Side,
    sources: &[String],
    to: Side,
    target: &str,
    answer: Option<ConflictChoice>,
    cancel_after: Option<usize>,
) -> Result<Outcome, crate::files::FilesError> {
    let job = RemoteJob {
        op,
        from,
        sources: sources.to_vec(),
        to,
        target: target.to_owned(),
    };
    tokio::task::spawn_blocking(move || {
        let handle = tokio::runtime::Handle::current();
        let plan = prepare_remote(&job, &handle, |_| None)?;
        let cancel = CancellationToken::new();
        let mut seen = 0;
        Ok(run_remote(
            &job,
            &plan,
            &handle,
            &[Duration::ZERO; 3],
            Hooks {
                cancel: &cancel,
                ask: &mut |_| answer.map(|choice| (choice, false)),
                progress: &mut |_| {
                    seen += 1;
                    if cancel_after == Some(seen) {
                        cancel.cancel();
                    }
                },
                removal: Removal::Permanent,
                rename: &|from, to| std::fs::rename(from, to),
            },
        ))
    })
    .await
    .expect("join")
}
