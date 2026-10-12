//! One store for every protocol (spec 050 research R1): each call reaches the store of the
//! connection's protocol, and a protocol without a store answers `Unsupported`.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use super::protocol::ProtocolStore;
use super::test_support::{access, FakeStore, Op};
use super::{Access, Protocol, RemoteStore, StorageError};

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

fn on(protocol: Protocol) -> Access {
    let mut access = access("ziel");
    access.location.protocol = protocol;
    access
}

#[tokio::test]
async fn each_protocol_reaches_its_store() {
    let (s3, webdav, sftp) = (
        Arc::new(FakeStore::new()),
        Arc::new(FakeStore::new()),
        Arc::new(FakeStore::new()),
    );
    let store = ProtocolStore::new(s3.clone(), Some(webdav.clone()), Some(sftp.clone()));
    for (protocol, key) in [
        (Protocol::S3, "a.txt"),
        (Protocol::WebDav, "b.txt"),
        (Protocol::Sftp, "c.txt"),
    ] {
        store
            .put(&on(protocol), key, b"x".to_vec(), soon())
            .await
            .expect("put");
    }
    assert_eq!(s3.calls(), [(Op::Put, "a.txt".to_owned())]);
    assert_eq!(webdav.calls(), [(Op::Put, "b.txt".to_owned())]);
    assert_eq!(sftp.calls(), [(Op::Put, "c.txt".to_owned())]);

    sftp.without_copy();
    assert!(store.copies_on_provider(&on(Protocol::S3)));
    assert!(!store.copies_on_provider(&on(Protocol::Sftp)));
}

#[tokio::test]
async fn a_protocol_without_a_store_is_unsupported() {
    let s3 = Arc::new(FakeStore::new());
    let store = ProtocolStore::new(s3.clone(), None, None);
    let webdav = on(Protocol::WebDav);
    assert_eq!(
        store.get(&webdav, "a.txt", 10, soon()).await.err(),
        Some(StorageError::Unsupported)
    );
    assert_eq!(
        store.begin_upload(&webdav, "a.txt", soon()).await.err(),
        Some(StorageError::Unsupported)
    );
    assert!(!store.copies_on_provider(&webdav));
    assert!(s3.calls().is_empty(), "nothing goes to S3 instead");
}
