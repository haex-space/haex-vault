//! Spec 048 on every protocol (spec 050 US7, T090, T091): the same acceptance run against the S3
//! fake, the WebDAV and SFTP servers of the test process and a folder of this device. The
//! encrypted layer knows no protocol (048 FR-041); what differs is the store behind the seam.

use std::any::Any;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use super::ops::{create_encrypted_folder, file_at, refresh};
use super::route::{load_view, open, route, Route};
use super::source::EncryptedFile;
use super::test_support::{give_key, transfer};
use super::{EncryptedFolders, Scope};
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::local::Outcome;
use crate::files::transfer::remote::Side;
use crate::files::transfer::{ConflictChoice, TransferOp};
use crate::files::{Entry, FilesErrorCode};
use crate::remote_storage::local::{access_of, LocalStore};
use crate::remote_storage::sftp::{test_server as sftp_server, SftpStore};
use crate::remote_storage::test_support::{access, resolver, vault, FakeStore};
use crate::remote_storage::webdav::{test_server as webdav_server, WebDavStore};
use crate::remote_storage::{
    Access, Addressing, Credentials, EndpointScope, Location, PasswordCredentials, Protocol,
    ProviderKind, RemoteStore,
};
use crate::vault_gate::VaultDb;

const NOTE: &str = "Steuerbescheid 2025";

/// A store behind the seam, its access, and what has to live as long as it.
struct Backend {
    store: Arc<dyn RemoteStore>,
    access: Access,
    _keep: Box<dyn Any + Send>,
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    S3,
    WebDav,
    Sftp,
    Device,
}

async fn backend(kind: Kind) -> Backend {
    match kind {
        Kind::S3 => Backend {
            store: Arc::new(FakeStore::new()),
            access: access("b"),
            _keep: Box::new(()),
        },
        Kind::WebDav => Backend {
            store: Arc::new(WebDavStore::new(resolver(&[]))),
            access: webdav_server::start("Ablage").await,
            _keep: Box::new(()),
        },
        Kind::Sftp => {
            let server = sftp_server::start(sftp_server::Options::new(9)).await;
            std::fs::create_dir(server.path("Ablage")).unwrap();
            let access = Access {
                location: Location {
                    protocol: Protocol::Sftp,
                    provider_kind: ProviderKind::Other,
                    endpoint: server.endpoint(),
                    endpoint_scope: EndpointScope::Local,
                    region: String::new(),
                    addressing: Addressing::Path,
                    bucket: "Ablage".to_owned(),
                    host_key: Some(server.host_key.clone()),
                },
                credentials: Credentials::Password(PasswordCredentials {
                    username: "anna".to_owned(),
                    password: Zeroizing::new("password".to_owned()),
                }),
            };
            Backend {
                store: Arc::new(SftpStore::new(resolver(&[]))),
                access,
                _keep: Box::new(server),
            }
        }
        Kind::Device => {
            let dir = tempfile::tempdir().unwrap();
            let root = std::fs::canonicalize(dir.path()).unwrap();
            Backend {
                store: Arc::new(LocalStore::new()),
                access: access_of(&root),
                _keep: Box::new(dir),
            }
        }
    }
}

/// One device of the vault: its own memory of opened folders, the vault's cache.
fn device(backend: &Backend, db: &VaultDb) -> StorageFiles {
    let db = db.clone();
    StorageFiles::new(Arc::clone(&backend.store), backend.access.clone()).with_scope(Scope {
        storage_id: "s".to_owned(),
        folders: Arc::new(EncryptedFolders::default()),
        vault: Arc::new(move || Ok(db.clone())),
        background: Arc::new(|_| {}),
    })
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(20)
}

async fn keys(backend: &Backend) -> Vec<String> {
    backend
        .store
        .list(&backend.access, "", 10_000, soon())
        .await
        .unwrap()
        .into_iter()
        .map(|object| object.key)
        .collect()
}

fn names(entries: &[Entry]) -> Vec<String> {
    let mut names: Vec<String> = entries.iter().map(|entry| entry.name.clone()).collect();
    names.sort();
    names
}

/// The same device's view again (a transfer takes its own).
fn same(files: &StorageFiles) -> StorageFiles {
    StorageFiles::new(Arc::clone(files.store()), files.access().clone())
        .with_scope(files.scope().unwrap().clone())
}

/// Copies the device file `name` with `content` into `/Unterlagen` of `files`, replacing.
async fn put(files: &StorageFiles, dir: &Path, name: &str, content: &[u8]) {
    let file = dir.join(name);
    std::fs::write(&file, content).unwrap();
    let outcome = transfer(
        TransferOp::Copy,
        Side::Device,
        &[file.to_string_lossy().into_owned()],
        Side::Storage(Arc::new(same(files))),
        "/Unterlagen",
        Some(ConflictChoice::Replace),
        None,
    )
    .await
    .unwrap();
    assert_eq!(outcome, Outcome::Done);
}

/// The plaintext `[start, end)` of the file at `path`.
async fn range(files: &StorageFiles, path: &str, start: u64, end: u64) -> Vec<u8> {
    let Route::Encrypted(at) = route(files, path).await.unwrap() else {
        panic!("{path} is encrypted");
    };
    let (folder, version) = file_at(files, &at).await.unwrap();
    EncryptedFile::open(files, folder, &version)
        .await
        .unwrap()
        .read(start, end)
        .await
        .unwrap()
}

async fn acceptance(kind: Kind) {
    let backend = backend(kind).await;
    let (_vault, db) = vault();
    give_key(&db, 1);
    let one = device(&backend, &db);
    create_encrypted_folder(&one, "/", "Unterlagen")
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let big: Vec<u8> = (0..200_000u32).map(|n| (n % 251) as u8).collect();
    put(&one, dir.path(), "brief.txt", NOTE.as_bytes()).await;
    put(&one, dir.path(), "gross.bin", &big).await;

    // Another device lists and reads by the decrypted names, also a range across blocks.
    let two = device(&backend, &db);
    assert_eq!(
        names(&two.list("/Unterlagen").await.unwrap()),
        ["brief.txt", "gross.bin"],
        "{kind:?}"
    );
    assert_eq!(
        two.read_start("/Unterlagen/brief.txt", 64, 64)
            .await
            .unwrap(),
        NOTE.as_bytes()
    );
    assert_eq!(
        range(&two, "/Unterlagen/gross.bin", 65_000, 140_000).await,
        &big[65_000..140_000]
    );

    // At the provider: only the head, sidecars and content objects, nothing in the clear.
    let stored = keys(&backend).await;
    let object = |key: &&String| !key.ends_with('/');
    let prefix = stored
        .iter()
        .find(|key| key.ends_with(".hxef/h"))
        .map(|key| key.trim_end_matches('h').to_owned())
        .expect("a head");
    for key in stored.iter().filter(object) {
        let rest = key.strip_prefix(&prefix).expect("under the folder");
        assert!(
            rest == "h" || rest.starts_with("m/") || rest.starts_with("c/"),
            "{kind:?}: {key}"
        );
        let body = backend
            .store
            .get(&backend.access, key, 1 << 20, soon())
            .await
            .unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("Steuer"), "{key}");
    }
    assert_eq!(
        stored.iter().filter(object).count(),
        5,
        "{kind:?}: {stored:?}"
    );

    // Every sidecar and content object has the provider's time, which the cleanup needs (FR-016).
    let Route::Encrypted(at) = route(&two, "/Unterlagen").await.unwrap() else {
        panic!("encrypted");
    };
    let loaded = load_view(&two, &open(&two, &at).await.unwrap())
        .await
        .unwrap();
    assert_eq!(loaded.times.len(), 4, "{kind:?}: 2 sidecars, 2 contents");

    // A rename on one device reaches the other with a refresh.
    one.rename("/Unterlagen/brief.txt", "anschreiben.txt")
        .await
        .unwrap();
    refresh(&two, "/Unterlagen").await.unwrap();
    assert_eq!(
        names(&two.list("/Unterlagen").await.unwrap()),
        ["anschreiben.txt", "gross.bin"]
    );

    // T091: both replace the same version without seeing each other's: a conflict copy.
    put(&one, dir.path(), "anschreiben.txt", b"von eins").await;
    tokio::time::sleep(Duration::from_millis(5)).await;
    put(&two, dir.path(), "anschreiben.txt", b"von zwei, laenger").await;
    let third = device(&backend, &db);
    let listed = third.list("/Unterlagen").await.unwrap();
    let copies: Vec<&Entry> = listed.iter().filter(|entry| entry.conflict).collect();
    assert_eq!(copies.len(), 1, "{kind:?}: {:?}", names(&listed));
    assert_eq!(copies[0].size, Some(8), "the older write is the copy");
    assert!(listed
        .iter()
        .any(|entry| entry.name == "anschreiben.txt" && entry.size == Some(17)));

    // A content object cut short at the provider: the file is damaged, nothing else.
    let mut objects = Vec::new();
    let contents = keys(&backend).await.into_iter();
    for key in contents.filter(|key| key.contains("/c/") && !key.ends_with('/')) {
        let head = backend
            .store
            .head(&backend.access, &key, soon())
            .await
            .unwrap();
        objects.push((head.size, key));
    }
    objects.sort();
    let (_, largest) = objects.last().cloned().unwrap();
    let body = backend
        .store
        .get(&backend.access, &largest, 1 << 20, soon())
        .await
        .unwrap();
    backend
        .store
        .put(
            &backend.access,
            &largest,
            body[..body.len() / 2].to_vec(),
            soon(),
        )
        .await
        .unwrap();
    let fourth = device(&backend, &db);
    let Route::Encrypted(at) = route(&fourth, "/Unterlagen/gross.bin").await.unwrap() else {
        panic!("encrypted");
    };
    let damaged = match file_at(&fourth, &at).await {
        Ok((folder, version)) => EncryptedFile::open(&fourth, folder, &version)
            .await
            .err()
            .map(|error| error.code),
        Err(error) => Some(error.code),
    };
    assert_eq!(damaged, Some(FilesErrorCode::Damaged), "{kind:?}");

    // Deleting the folder leaves nothing of it.
    one.delete("/Unterlagen", &CancellationToken::new(), &mut |_| {})
        .await
        .unwrap();
    let left: Vec<String> = keys(&backend)
        .await
        .into_iter()
        .filter(|key| key.contains(".hxef"))
        .collect();
    assert!(left.is_empty(), "{kind:?}: {left:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn encrypted_folders_on_s3() {
    acceptance(Kind::S3).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn encrypted_folders_on_webdav() {
    acceptance(Kind::WebDav).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn encrypted_folders_on_sftp() {
    acceptance(Kind::Sftp).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn encrypted_folders_on_the_device() {
    acceptance(Kind::Device).await;
}
