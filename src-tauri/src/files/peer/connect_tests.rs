//! One link per target device (T024): an own device found by its device id on the vault's list, at
//! the address presence noted; what has no way yet is refused before any dial.

use std::sync::Arc;

use super::Connections;
use crate::files::peer::client::PeerLinks;
use crate::files::peer::test_support::own_pair;
use crate::files::peer::wire::Mode;
use crate::remote_storage::StorageError;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_own_device_is_linked_once_by_its_device_id() {
    let (main, linked) = own_pair().await;
    linked.node.note_presence(main.node.addr()).await;
    let connections = Connections::new(Arc::clone(&linked.node), Arc::clone(&linked.ctx));
    let target = format!("peer:{}", main.uuid);

    let link = connections.link(&target).await.expect("link");
    assert_eq!(link.welcome.mode, Mode::OwnDevice);
    let again = connections.link(&target).await.expect("again");
    assert!(Arc::ptr_eq(&link, &again), "one connection per device");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_target_without_a_way_is_refused_before_any_dial() {
    let (main, linked) = own_pair().await;
    let connections = Connections::new(Arc::clone(&linked.node), Arc::clone(&linked.ctx));
    for (target, error) in [
        ("share:some-share".to_owned(), StorageError::Unsupported),
        ("storage:s3".to_owned(), StorageError::Unsupported),
        (
            format!("peer:{}", uuid::Uuid::new_v4()),
            StorageError::NotFound,
        ),
        (format!("peer:{}", linked.uuid), StorageError::NotFound),
    ] {
        assert_eq!(
            connections.link(&target).await.err(),
            Some(error),
            "{target}"
        );
    }
    drop(main);
}
