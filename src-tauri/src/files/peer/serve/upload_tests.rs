//! Uploads from another own device (FR-035, FR-036, FR-044, SC-008): a stale version is a
//! conflict that leaves the file as it is, and an aborted or orphaned upload leaves nothing behind.

use std::path::Path;
use std::sync::Arc;

use tokio::time::{Duration, Instant};

use crate::files::peer::client::PeerLink;
use crate::files::peer::store::{peer_access, PeerStore};
use crate::files::peer::test_support::{own_pair, Fixed};
use crate::files::peer::wire::Caller;
use crate::remote_storage::{Access, ObjectStore, StorageError};

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(20)
}

fn key(path: &Path) -> String {
    path.to_string_lossy().trim_start_matches('/').to_owned()
}

/// Every name in `dir`, temporary files included.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("folder")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

async fn store_on(link: PeerLink, target: &str) -> (Arc<PeerLink>, PeerStore, Access) {
    let link = Arc::new(link);
    let store = PeerStore::new(Arc::new(Fixed(Arc::clone(&link))), Caller::User);
    (link, store, peer_access(target, None))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_file_changed_meanwhile_is_a_conflict_and_stays() {
    let (main, linked) = own_pair().await;
    let file = main.write("Text/notiz.txt", b"erste");
    let link = linked.link_to(&main).await.expect("link");
    let (_link, store, access) = store_on(link, &format!("peer:{}", main.uuid)).await;
    let key = key(&file);

    let id = store
        .begin_upload(&access, &key, soon())
        .await
        .expect("begin");
    std::fs::write(&file, b"zweite, laenger").expect("changed on the device");
    let mark = store
        .upload_part(&access, &key, &id, 1, b"meine".to_vec(), soon())
        .await
        .expect("part");
    assert_eq!(
        store
            .finish_upload(&access, &key, &id, &[mark], soon())
            .await,
        Err(StorageError::Conflict)
    );
    assert_eq!(std::fs::read(&file).expect("file"), b"zweite, laenger");
    assert_eq!(
        names(file.parent().unwrap()),
        ["notiz.txt"],
        "no temporary file"
    );

    store
        .put(&access, &key, b"dritte".to_vec(), soon())
        .await
        .expect("a fresh upload over the version seen now");
    assert_eq!(std::fs::read(&file).expect("file"), b"dritte");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_file_that_appeared_meanwhile_is_a_conflict() {
    let (main, linked) = own_pair().await;
    std::fs::create_dir(main.root().join("Neu")).expect("folder");
    let link = linked.link_to(&main).await.expect("link");
    let (_link, store, access) = store_on(link, &format!("peer:{}", main.uuid)).await;
    let key = key(&main.root().join("Neu/a.txt"));

    let id = store
        .begin_upload(&access, &key, soon())
        .await
        .expect("begin");
    main.write("Neu/a.txt", b"von dort");
    assert_eq!(
        store.finish_upload(&access, &key, &id, &[], soon()).await,
        Err(StorageError::Conflict)
    );
    assert_eq!(
        std::fs::read(main.root().join("Neu/a.txt")).expect("file"),
        b"von dort"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_aborted_upload_leaves_nothing() {
    let (main, linked) = own_pair().await;
    std::fs::create_dir(main.root().join("Ziel")).expect("folder");
    let link = linked.link_to(&main).await.expect("link");
    let (_link, store, access) = store_on(link, &format!("peer:{}", main.uuid)).await;
    let key = key(&main.root().join("Ziel/gross.bin"));

    let id = store
        .begin_upload(&access, &key, soon())
        .await
        .expect("begin");
    store
        .upload_part(&access, &key, &id, 1, vec![7; 1024], soon())
        .await
        .expect("part");
    store
        .abort_upload(&access, &key, &id, soon())
        .await
        .expect("abort");
    assert!(names(&main.root().join("Ziel")).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_upload_ends_with_its_connection() {
    let (main, linked) = own_pair().await;
    std::fs::create_dir(main.root().join("Ziel")).expect("folder");
    let link = linked.link_to(&main).await.expect("link");
    let (link, store, access) = store_on(link, &format!("peer:{}", main.uuid)).await;
    let key = key(&main.root().join("Ziel/halb.bin"));

    let id = store
        .begin_upload(&access, &key, soon())
        .await
        .expect("begin");
    store
        .upload_part(&access, &key, &id, 1, vec![7; 1024], soon())
        .await
        .expect("part");
    assert_eq!(
        names(&main.root().join("Ziel")).len(),
        1,
        "the temporary file"
    );
    link.close();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !names(&main.root().join("Ziel")).is_empty() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the serving device cleans up (FR-044)");
}
