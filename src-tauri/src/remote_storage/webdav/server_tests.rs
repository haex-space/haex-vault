//! The WebDAV store against a real server in the test process (spec 050 US2, T040, research R14):
//! an upload in parts lands whole or not at all and never touches the file under its name before
//! it is complete (SC-003), a folder of 100 files moves with one call, and ranges read for a seek.

use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::time::Instant;

use super::{test_server, WebDavStore};
use crate::remote_storage::test_support::resolver;
use crate::remote_storage::{Access, ObjectStore, StorageError};

fn store() -> WebDavStore {
    WebDavStore::new(resolver(&[]))
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

async fn keys(store: &WebDavStore, access: &Access, prefix: &str) -> Vec<String> {
    store
        .list(access, prefix, 1000, soon())
        .await
        .expect("list")
        .into_iter()
        .map(|object| object.key)
        .collect()
}

#[tokio::test]
async fn an_upload_in_parts_lands_whole_and_a_cancelled_one_leaves_the_old_file() {
    let access = test_server::start("Ablage").await;
    let store = store();
    store
        .put(&access, "Filme/film.bin", b"alt".to_vec(), soon())
        .await
        .expect("put");

    let cancelled = store
        .begin_upload(&access, "Filme/film.bin", soon())
        .await
        .expect("begin");
    store
        .upload_part(
            &access,
            "Filme/film.bin",
            &cancelled,
            1,
            vec![b'x'; 4096],
            soon(),
        )
        .await
        .expect("part");
    store
        .abort_upload(&access, "Filme/film.bin", &cancelled, soon())
        .await
        .expect("abort");
    let body = store
        .get(&access, "Filme/film.bin", 1 << 20, soon())
        .await
        .expect("get");
    assert_eq!(
        body, b"alt",
        "the file under its name is unchanged (SC-003)"
    );
    assert_eq!(
        keys(&store, &access, "").await,
        ["Filme/", "Filme/film.bin"]
    );

    let upload = store
        .begin_upload(&access, "Filme/film.bin", soon())
        .await
        .expect("begin");
    let mut marks = Vec::new();
    for (number, part) in [(1u16, b"eins-"), (2, b"zwei-"), (3, b"drei!")] {
        marks.push(
            store
                .upload_part(
                    &access,
                    "Filme/film.bin",
                    &upload,
                    number,
                    part.to_vec(),
                    soon(),
                )
                .await
                .expect("part"),
        );
    }
    store
        .finish_upload(&access, "Filme/film.bin", &upload, &marks, soon())
        .await
        .expect("finish");
    let body = store
        .get(&access, "Filme/film.bin", 1 << 20, soon())
        .await
        .expect("get");
    assert_eq!(body, b"eins-zwei-drei!");
    let listing = store
        .list_dir(&access, "Filme/", 100, soon())
        .await
        .expect("list_dir");
    assert_eq!(listing.objects.len(), 1, "no temporary file stays");
}

#[tokio::test]
async fn a_folder_of_100_files_moves_and_goes_with_one_call() {
    let access = test_server::start("Ablage").await;
    let store = store();
    for n in 0..100 {
        store
            .put(&access, &format!("Fotos/{n:03}.jpg"), vec![n as u8], soon())
            .await
            .expect("put");
    }
    store
        .rename_tree(&access, "Fotos/", "Bilder/", soon())
        .await
        .expect("move");
    let moved = keys(&store, &access, "").await;
    assert_eq!(moved.len(), 101, "the folder and its 100 files");
    assert_eq!(moved[0], "Bilder/");
    assert!(moved.iter().all(|key| key.starts_with("Bilder/")));
    let body = store
        .get(&access, "Bilder/042.jpg", 10, soon())
        .await
        .expect("get");
    assert_eq!(body, [42]);

    store
        .delete(&access, "Bilder/", soon())
        .await
        .expect("delete of a full folder");
    assert_eq!(
        keys(&store, &access, "").await.len(),
        101,
        "a folder marker goes only when its folder is empty, as on S3"
    );
    store
        .remove_tree(&access, "Bilder/", soon())
        .await
        .expect("remove");
    assert!(keys(&store, &access, "").await.is_empty());
}

#[tokio::test]
async fn ranges_read_for_a_seek_and_the_head_tells_the_size() {
    let access = test_server::start("").await;
    let store = store();
    let body: Vec<u8> = (0..=255u8).cycle().take(4000).collect();
    store
        .put(&access, "video.mp4", body.clone(), soon())
        .await
        .expect("put");
    let head = store
        .head(&access, "video.mp4", soon())
        .await
        .expect("head");
    assert_eq!(head.size, 4000);
    assert!(head.etag.is_some());
    let mut reader = store
        .get_range(&access, "video.mp4", 3000, 100, soon())
        .await
        .expect("range");
    let mut part = Vec::new();
    reader.read_to_end(&mut part).await.expect("read");
    assert_eq!(part, body[3000..3100]);
}

#[tokio::test]
async fn listings_show_times_like_s3_and_a_missing_file_is_not_found() {
    let access = test_server::start("Ablage").await;
    let store = store();
    store
        .put(&access, "a/b/c.txt", b"c".to_vec(), soon())
        .await
        .expect("put");
    let found = store.list(&access, "a/b/", 10, soon()).await.expect("list");
    let file = found.iter().find(|o| o.key == "a/b/c.txt").expect("file");
    assert!(
        crate::files::storage_time::parse_iso_ms(&file.last_modified).is_some(),
        "{:?}",
        file.last_modified
    );
    assert_eq!(
        store.get(&access, "fehlt.txt", 10, soon()).await.err(),
        Some(StorageError::NotFound)
    );
}

#[tokio::test]
async fn a_server_without_quota_gives_no_space() {
    let access = test_server::start("Ablage").await;
    let space = store().space(&access, soon()).await.expect("space");
    assert_eq!(space, None, "the in-memory server reports no quota");
}
