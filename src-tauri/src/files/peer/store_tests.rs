//! Another own device through [`PeerStore`] (T027): the calls the file browser, its viewer and
//! transfers make read the serving device's files; writing waits for its delivery.

use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::time::{Duration, Instant};

use super::{peer_access, PeerStore};
use crate::files::peer::test_support::{own_pair, Fixed};
use crate::files::peer::wire::Caller;
use crate::files::storage_source::StorageFiles;
use crate::files::EntryKind;
use crate::remote_storage::{ObjectStore, StorageError};

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(20)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_own_device_reads_another_one_through_the_storage_seam() {
    let (main, linked) = own_pair().await;
    main.write("Dokumente/brief.txt", b"Hallo Welt");
    main.write("Dokumente/Archiv/alt.txt", b"alt");
    let link = Arc::new(linked.link_to(&main).await.expect("link"));
    let store = PeerStore::new(Arc::new(Fixed(link)), Caller::User);
    let access = peer_access(&format!("peer:{}", main.uuid), None);
    let base = main
        .root()
        .to_string_lossy()
        .trim_start_matches('/')
        .to_owned();
    let folder = format!("{base}/Dokumente/");

    let listing = store
        .list_dir(&access, &folder, 100, soon())
        .await
        .expect("list");
    assert_eq!(listing.prefixes, [format!("{folder}Archiv/")]);
    assert_eq!(listing.objects.len(), 1);
    assert_eq!(listing.objects[0].key, format!("{folder}brief.txt"));
    assert_eq!(listing.objects[0].size, 10);

    let key = format!("{folder}brief.txt");
    let head = store.head(&access, &key, soon()).await.expect("head");
    assert_eq!(head.size, 10);
    assert!(head.etag.is_some(), "the version of FR-036");

    let mut range = String::new();
    store
        .get_range(&access, &key, 6, 4, soon())
        .await
        .expect("range")
        .read_to_string(&mut range)
        .await
        .expect("read");
    assert_eq!(range, "Welt", "only the asked range (FR-012)");
    assert_eq!(
        store.get(&access, &key, 100, soon()).await.expect("get"),
        b"Hallo Welt"
    );
    assert_eq!(
        store.get(&access, &key, 4, soon()).await,
        Err(StorageError::TooLarge)
    );
    let all = store
        .list(&access, &folder, 100, soon())
        .await
        .expect("all");
    assert_eq!(all.len(), 2, "both files, the one in the subfolder too");
    assert_eq!(
        store.delete(&access, &key, soon()).await,
        Err(StorageError::Unsupported),
        "writing comes with the delivery for own devices"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_file_browser_lists_another_device_like_a_storage() {
    let (main, linked) = own_pair().await;
    main.write("Musik/lied.mp3", b"ID3");
    let link = Arc::new(linked.link_to(&main).await.expect("link"));
    let files = StorageFiles::new(
        Arc::new(PeerStore::new(Arc::new(Fixed(link)), Caller::User)),
        peer_access(&format!("peer:{}", main.uuid), None),
    );
    let folder = format!("{}/Musik", main.root().to_string_lossy());
    let entries = files.list(&folder).await.expect("list");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "lied.mp3");
    assert_eq!(entries[0].kind, EntryKind::File);
    assert_eq!(entries[0].size, Some(3));
}
