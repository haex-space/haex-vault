//! Shared setup for the tests of encrypted folders: a vault that holds content keys, a fake storage
//! and the window's view of it, and a transfer run as the manager runs it.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::manage::shown_version;
use super::ops::create_encrypted_folder;
use super::route::{open, route, view, Route};
use super::tree::Version;
use super::{EncryptedFolders, Scope};
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::local::{Hooks, Outcome, Removal};
use crate::files::transfer::remote::{prepare_remote, run_remote, RemoteJob, Side};
use crate::files::transfer::{ConflictChoice, TransferOp};
use crate::remote_storage::test_support::{access, vault, FakeStore, Op};
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
        self.files_in(BUCKET, STORAGE)
    }

    /// Another storage `storage_id` of the window on `bucket` of the same provider.
    pub fn files_in(&self, bucket: &str, storage_id: &str) -> StorageFiles {
        let db = self.db.clone();
        StorageFiles::new(self.fake.clone(), access(bucket)).with_scope(Scope {
            storage_id: storage_id.to_owned(),
            folders: Arc::clone(&self.folders),
            vault: Arc::new(move || Ok(db.clone())),
            background: Arc::new(|_| {}),
        })
    }

    /// The same storage as another device of the vault sees it: nothing in memory yet.
    pub fn fresh_files(&self) -> StorageFiles {
        let db = self.db.clone();
        StorageFiles::new(self.fake.clone(), access(BUCKET)).with_scope(Scope {
            storage_id: STORAGE.to_owned(),
            folders: Arc::default(),
            vault: Arc::new(move || Ok(db.clone())),
            background: Arc::new(|_| {}),
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

/// The encrypted folder "Unterlagen" with the device files `tree` (path → content) copied in.
pub async fn filled(tree: &[(String, String)]) -> Scene {
    let scene = Scene::new();
    create_encrypted_folder(&scene.files(), "/", "Unterlagen")
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut tops = BTreeSet::new();
    for (path, content) in tree {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, content).unwrap();
        tops.insert(path.split('/').next().unwrap().to_owned());
    }
    let sources: Vec<String> = tops
        .iter()
        .map(|top| dir.path().join(top).to_string_lossy().into_owned())
        .collect();
    let outcome = transfer(
        TransferOp::Copy,
        Side::Device,
        &sources,
        Side::Storage(Arc::new(scene.files())),
        "/Unterlagen",
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(outcome, Outcome::Done);
    scene
}

pub fn tree(files: &[(&str, &str)]) -> Vec<(String, String)> {
    files
        .iter()
        .map(|(path, content)| ((*path).to_owned(), (*content).to_owned()))
        .collect()
}

/// The calls of `ops` since call number `mark`, with their keys.
pub fn since(scene: &Scene, mark: usize, ops: &[Op]) -> Vec<(Op, String)> {
    scene.fake.calls()[mark..]
        .iter()
        .filter(|(op, _)| ops.contains(op))
        .cloned()
        .collect()
}

pub const WRITES: [Op; 5] = [
    Op::Put,
    Op::Copy,
    Op::CreateMultipart,
    Op::UploadPart,
    Op::CompleteMultipart,
];

/// The version the entry at `path` shows.
pub async fn version_of(files: &StorageFiles, path: &str) -> Version {
    let Route::Encrypted(at) = route(files, path).await.unwrap() else {
        panic!("{path} is in an encrypted folder");
    };
    let folder = open(files, &at).await.unwrap();
    let view = view(files, &folder).await.unwrap();
    let index = view.tree.find(&at.rest).unwrap();
    shown_version(&view.tree.node(index).item).unwrap()
}
