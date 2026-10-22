//! Channels by extension (spec 057 T018, FR-006, FR-041).

use super::*;
use crate::extensions::peers::channel::{open, Opening, Origin};
use crate::extensions::peers::test_support::{install, open_test_channel, Admit, Vault};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_channel_of_another_extension_is_answered_like_a_missing_one() {
    let (a, b) = (
        Vault::start(Admit::AtOnce).await,
        Vault::start(Admit::AtOnce).await,
    );
    let (near, _far) = open_test_channel(&a, &b).await;
    let other = install(&a.db, "other-probe");

    let foreign = a.host.peers.channel(other, near.id).err();
    let missing = a.host.peers.channel(a.extension_id, Uuid::new_v4()).err();
    assert_eq!(foreign, Some(PeerError::ChannelUnknown));
    assert_eq!(foreign, missing);
    assert_eq!(
        a.host.peers.close(other, near.id),
        Err(PeerError::ChannelUnknown)
    );
    // The foreign attempt left the channel alone.
    assert_eq!(
        a.host.peers.channel(a.extension_id, near.id).map(|c| c.id),
        Ok(near.id)
    );
    assert_eq!(a.host.peers.extensions(), BTreeSet::from([a.extension_id]));
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_extension_gets_at_most_its_limit_of_channels() {
    let (a, b) = (
        Vault::start(Admit::AtOnce).await,
        Vault::start(Admit::AtOnce).await,
    );
    for _ in 0..CHANNELS_PER_EXTENSION {
        open_test_channel(&a, &b).await;
    }
    // A third vault, still below the limit itself, takes the one more channel.
    let c = Vault::start(Admit::AtOnce).await;
    let (connection, send, recv, verified) = a.dial(&c).await;
    let refused = open(
        Opening {
            extension_id: a.extension_id,
            remote: verified.expect("verified"),
            origin: Origin::Test,
            connection: connection.clone(),
            send,
            recv,
        },
        &a.ctx.frames,
        &a.node.tasks().tracker,
    );
    assert!(matches!(
        refused,
        Err(PeerError::Limit {
            limit: CHANNELS_PER_EXTENSION
        })
    ));
    assert!(
        connection.close_reason().is_some(),
        "the refused connection is closed"
    );
    assert_eq!(a.host.peers.remotes().len(), CHANNELS_PER_EXTENSION);
    a.shutdown().await;
    b.shutdown().await;
    c.shutdown().await;
}
