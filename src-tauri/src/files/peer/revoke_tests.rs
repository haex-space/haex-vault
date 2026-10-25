//! Revoking and downgrading (spec 056 US4, T061, SC-004): a revoke ends a read in progress at once
//! and refuses the next request; a downgrade refuses the next write and stops an upload in progress
//! without a half file.

use tokio::io::AsyncReadExt;

use super::contact_tests::{files_of, shared, soon};
use crate::files::share::served::apply;
use crate::files::share::store::{self, FileShareRow};
use crate::files::share::FileShareLevel;
use crate::remote_storage::{ObjectStore, StorageError};

/// More than the stream windows hold, so the serving device is still sending when the revoke comes.
const LARGE: usize = 24 * 1024 * 1024;

/// The one share Anna serves.
fn the_share(anna: &crate::files::peer::test_support::Device) -> FileShareRow {
    let device = anna.uuid.clone();
    anna.db
        .read_blocking(move |q| Ok(store::shares_served_by(q, &device)?))
        .expect("shares")
        .remove(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_revoke_ends_a_read_in_progress_and_refuses_the_next_request() {
    let (anna, ben, links, storage_id) = shared(FileShareLevel::Read).await;
    let content: Vec<u8> = (0..LARGE).map(|i| (i % 251) as u8).collect();
    anna.write("Fotos/film.mkv", &content);
    let (files, store) = files_of(&ben, links, &storage_id).await;
    let access = files.access().clone();

    let mut reader = store
        .get_range(&access, "film.mkv", 0, LARGE as u64, soon())
        .await
        .expect("read starts");
    let mut first = vec![0; 64 * 1024];
    reader
        .read_exact(&mut first)
        .await
        .expect("the first bytes");

    anna.revoke(&the_share(&anna).id, ben.member.vault);
    apply(&anna.ctx, &mut std::collections::HashSet::new()).await;

    let mut rest = Vec::new();
    let read = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        reader.read_to_end(&mut rest),
    )
    .await
    .expect("ends at once");
    assert!(
        read.is_err(),
        "the stream ends with an error, not as a whole file"
    );
    assert!(first.len() + rest.len() < LARGE);
    assert_eq!(
        store.head(&access, "film.mkv", soon()).await,
        Err(StorageError::MissingRight),
        "FR-024: the next request"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_downgrade_stops_an_upload_and_refuses_the_next_write() {
    let (anna, ben, links, storage_id) = shared(FileShareLevel::ReadWrite).await;
    let (files, store) = files_of(&ben, links, &storage_id).await;
    let access = files.access().clone();
    let upload = store
        .begin_upload(&access, "neu.jpg", soon())
        .await
        .expect("begin");
    store
        .upload_part(&access, "neu.jpg", &upload, 1, b"halb".to_vec(), soon())
        .await
        .expect("part");

    anna.grant(&the_share(&anna).id, ben.member.vault, FileShareLevel::Read);
    apply(&anna.ctx, &mut std::collections::HashSet::new()).await;

    assert!(store
        .finish_upload(&access, "neu.jpg", &upload, &[], soon())
        .await
        .is_err());
    assert!(!anna.root().join("Fotos/neu.jpg").exists(), "no half file");
    assert_eq!(
        store.put(&access, "b.jpg", b"x".to_vec(), soon()).await,
        Err(StorageError::MissingRight)
    );
    assert_eq!(
        store
            .get(&access, "a.jpg", 100, soon())
            .await
            .expect("reading goes on"),
        b"aaaa"
    );
}
