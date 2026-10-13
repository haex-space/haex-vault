//! The local store against temporary folders (spec 048 US7, research R15, FR-047): keys stay under
//! the root, listings read as on WebDAV, a write never leaves half a file under its name, and no
//! error names a path.

use std::path::Path;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::time::Instant;

use super::{access_of, LocalStore};
use crate::remote_storage::{Access, RemoteStore, StorageError};

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

fn access(root: &Path) -> Access {
    access_of(root)
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn keys_map_under_the_root() {
    let root = tempfile::tempdir().unwrap();
    let store = LocalStore::new();
    let access = access(root.path());
    store
        .put(&access, "a/b/c.txt", b"hallo".to_vec(), soon())
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(root.path().join("a/b/c.txt")).unwrap(),
        b"hallo"
    );
    assert_eq!(
        store.get(&access, "a/b/c.txt", 100, soon()).await.unwrap(),
        b"hallo"
    );
    store
        .put(&access, "leer/", Vec::new(), soon())
        .await
        .unwrap();
    assert!(root.path().join("leer").is_dir());
}

#[tokio::test]
async fn keys_that_leave_the_root_are_refused() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("root");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(base.path().join("geheim.txt"), b"x").unwrap();
    let store = LocalStore::new();
    let access = access(&root);
    for key in [
        "../geheim.txt",
        "a/../../geheim.txt",
        "/etc/passwd",
        "a//b",
        "./a",
        "a\0b",
    ] {
        assert_eq!(
            store.get(&access, key, 100, soon()).await,
            Err(StorageError::AccessDenied),
            "{key:?}"
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(base.path(), root.join("raus")).unwrap();
        assert_eq!(
            store.get(&access, "raus/geheim.txt", 100, soon()).await,
            Err(StorageError::AccessDenied)
        );
        assert_eq!(
            store
                .put(&access, "raus/neu.txt", b"x".to_vec(), soon())
                .await,
            Err(StorageError::AccessDenied)
        );
        assert!(!base.path().join("neu.txt").exists());
    }
}

#[tokio::test]
async fn list_goes_deep_and_list_dir_one_level() {
    let root = tempfile::tempdir().unwrap();
    let store = LocalStore::new();
    let access = access(root.path());
    for key in ["f/h", "f/m/aa", "f/c/bb", "g.txt"] {
        store
            .put(&access, key, b"12".to_vec(), soon())
            .await
            .unwrap();
    }
    std::fs::write(root.path().join("f/.holzi-part-x"), b"halb").unwrap();
    let keys: Vec<String> = store
        .list(&access, "f/", 100, soon())
        .await
        .unwrap()
        .into_iter()
        .map(|object| object.key)
        .collect();
    // The folder itself shows as its marker, as on S3, WebDAV and SFTP.
    assert_eq!(keys, ["f/", "f/c/", "f/c/bb", "f/h", "f/m/", "f/m/aa"]);
    let level = store.list_dir(&access, "f/", 100, soon()).await.unwrap();
    assert_eq!(level.prefixes, ["f/c/", "f/m/"]);
    let objects: Vec<&str> = level.objects.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(objects, ["f/h"]);
    assert_eq!(level.objects[0].size, 2);
    assert!(crate::files::storage_time::parse_iso_ms(&level.objects[0].last_modified).is_some());
    assert_eq!(
        store.list(&access, "f/", 3, soon()).await,
        Err(StorageError::TooLarge)
    );
    assert_eq!(
        store.list_dir(&access, "fehlt/", 100, soon()).await,
        Err(StorageError::NotFound)
    );
}

#[tokio::test]
async fn head_gives_size_date_and_an_etag_of_the_content() {
    let root = tempfile::tempdir().unwrap();
    let store = LocalStore::new();
    let access = access(root.path());
    store
        .put(&access, "h", b"eins".to_vec(), soon())
        .await
        .unwrap();
    let first = store.head(&access, "h", soon()).await.unwrap();
    assert_eq!(first.size, 4);
    let date = first.last_modified.clone().unwrap();
    assert!(
        crate::files::storage_time::parse_http_date_ms(&date).is_some(),
        "{date}"
    );
    assert_eq!(store.head(&access, "h", soon()).await.unwrap(), first);
    store
        .put(&access, "h", b"zwei".to_vec(), soon())
        .await
        .unwrap();
    let second = store.head(&access, "h", soon()).await.unwrap();
    assert_ne!(second.etag, first.etag);
    let listed = store.list(&access, "h", 10, soon()).await.unwrap();
    assert_eq!(Some(listed[0].etag.clone()), second.etag);
    assert_eq!(
        store.head(&access, "fehlt", soon()).await,
        Err(StorageError::NotFound)
    );
}

#[tokio::test]
async fn get_range_reads_exactly_the_range() {
    let root = tempfile::tempdir().unwrap();
    let store = LocalStore::new();
    let access = access(root.path());
    store
        .put(&access, "v", b"0123456789".to_vec(), soon())
        .await
        .unwrap();
    let mut read = Vec::new();
    store
        .get_range(&access, "v", 3, 4, soon())
        .await
        .unwrap()
        .read_to_end(&mut read)
        .await
        .unwrap();
    assert_eq!(read, b"3456");
    assert_eq!(
        store.get(&access, "v", 5, soon()).await,
        Err(StorageError::TooLarge)
    );
}

#[tokio::test]
async fn uploads_never_leave_half_a_file_under_the_name() {
    let root = tempfile::tempdir().unwrap();
    let store = LocalStore::new();
    let access = access(root.path());
    let id = store
        .begin_upload(&access, "o/datei", soon())
        .await
        .unwrap();
    store
        .upload_part(&access, "o/datei", &id, 1, b"ab".to_vec(), soon())
        .await
        .unwrap();
    assert!(!root.path().join("o/datei").exists());
    store
        .upload_part(&access, "o/datei", &id, 2, b"cd".to_vec(), soon())
        .await
        .unwrap();
    store
        .finish_upload(&access, "o/datei", &id, &[], soon())
        .await
        .unwrap();
    assert_eq!(std::fs::read(root.path().join("o/datei")).unwrap(), b"abcd");
    assert_eq!(names(&root.path().join("o")), ["datei"]);

    let id = store.begin_upload(&access, "o/weg", soon()).await.unwrap();
    store
        .upload_part(&access, "o/weg", &id, 1, b"ab".to_vec(), soon())
        .await
        .unwrap();
    store
        .abort_upload(&access, "o/weg", &id, soon())
        .await
        .unwrap();
    assert_eq!(names(&root.path().join("o")), ["datei"]);
    assert_eq!(
        store
            .finish_upload(&access, "o/weg", &id, &[], soon())
            .await,
        Err(StorageError::NotFound)
    );
}

#[tokio::test]
async fn copy_rename_and_remove_trees() {
    let root = tempfile::tempdir().unwrap();
    let store = LocalStore::new();
    let access = access(root.path());
    assert!(store.copies_on_provider(&access));
    store
        .put(&access, "a/x", b"1".to_vec(), soon())
        .await
        .unwrap();
    store.copy(&access, "a/x", "b/y", soon()).await.unwrap();
    assert_eq!(std::fs::read(root.path().join("b/y")).unwrap(), b"1");
    store
        .rename_tree(&access, "a/", "c/d/", soon())
        .await
        .unwrap();
    assert!(!root.path().join("a").exists());
    assert_eq!(std::fs::read(root.path().join("c/d/x")).unwrap(), b"1");
    store.remove_tree(&access, "c/", soon()).await.unwrap();
    assert!(!root.path().join("c").exists());
    store.remove_tree(&access, "c/", soon()).await.unwrap();
    store.delete(&access, "b/y", soon()).await.unwrap();
    store.delete(&access, "b/y", soon()).await.unwrap();
    store
        .put(&access, "b/z", b"1".to_vec(), soon())
        .await
        .unwrap();
    store.delete(&access, "b/", soon()).await.unwrap();
    assert!(
        root.path().join("b/z").exists(),
        "a folder goes only when empty"
    );
}

#[tokio::test]
async fn errors_carry_no_path() {
    let root = tempfile::tempdir().unwrap();
    let store = LocalStore::new();
    let access = access(root.path());
    let error = store
        .get(&access, "fehlt/datei", 100, soon())
        .await
        .unwrap_err();
    assert_eq!(error, StorageError::NotFound);
    let text = error.to_string();
    assert!(!text.contains("fehlt"), "{text}");
    assert!(!text.contains(&*root.path().to_string_lossy()), "{text}");
}
