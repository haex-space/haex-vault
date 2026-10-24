//! A channel between two vaults on loopback (spec 057 T022, T023): fast and reliable messages, their
//! limits, and how each side hears of the end.

use iroh::endpoint::ApplicationClose;

use super::*;
use crate::extensions::peers::test_support::{open_test_channel, Admit, Vault, Window, LIMIT};

fn decoded(data: &Value) -> Vec<u8> {
    STANDARD
        .decode(data["data"].as_str().expect("base64 data"))
        .expect("valid base64")
}

/// The messages of `channel` the window got, in order: `(bytes, reliable)`.
fn messages(window: &Window, channel: &Channel) -> Vec<(Vec<u8>, bool)> {
    window
        .frame_events(MESSAGE)
        .into_iter()
        .filter(|(_, _, data)| data["channelId"] == channel.id.to_string())
        .map(|(_, _, data)| (decoded(&data), data["reliable"] == true))
        .collect()
}

fn ends(window: &Window, channel: &Channel) -> Vec<String> {
    window
        .frame_events(CHANNEL_CLOSED)
        .into_iter()
        .filter(|(_, _, data)| data["channelId"] == channel.id.to_string())
        .map(|(_, _, data)| data["reason"].as_str().unwrap_or_default().to_owned())
        .collect()
}

async fn pair() -> (Vault, Vault) {
    (
        Vault::start(Admit::AtOnce).await,
        Vault::start(Admit::AtOnce).await,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fast_message_of_the_limit_arrives_and_a_larger_one_is_refused() {
    let (a, b) = pair().await;
    let (near, far) = open_test_channel(&a, &b).await;
    assert!(
        near.max_datagram_size()
            .is_some_and(|size| size >= FAST_BYTES),
        "loopback carries {:?}",
        near.max_datagram_size()
    );

    let largest: Vec<u8> = (0..FAST_BYTES).map(|i| (i % 251) as u8).collect();
    near.send_fast(&largest).expect("send");
    b.window.wait_until(|w| !messages(w, &far).is_empty()).await;
    assert_eq!(messages(&b.window, &far), vec![(largest, false)]);

    assert_eq!(
        near.send_fast(&vec![0; FAST_BYTES + 1]),
        Err(PeerError::TooLarge { limit: FAST_BYTES })
    );
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reliable_messages_arrive_whole_and_in_order_up_to_the_limit() {
    let (a, b) = pair().await;
    let (near, far) = open_test_channel(&a, &b).await;

    let sent: Vec<Vec<u8>> = vec![
        b"first".to_vec(),
        vec![1; RELIABLE_BYTES],
        Vec::new(),
        b"last".to_vec(),
    ];
    for data in &sent {
        near.send_reliable(data.clone()).expect("queue");
    }
    b.window
        .wait_until(|w| messages(w, &far).len() == sent.len())
        .await;
    let expected: Vec<(Vec<u8>, bool)> = sent.into_iter().map(|data| (data, true)).collect();
    assert_eq!(messages(&b.window, &far), expected);

    assert_eq!(
        near.send_reliable(vec![0; RELIABLE_BYTES + 1]),
        Err(PeerError::TooLarge {
            limit: RELIABLE_BYTES
        })
    );
    // The other way round works the same.
    far.send_reliable(b"back".to_vec()).expect("queue");
    a.window
        .wait_until(|w| !messages(w, &near).is_empty())
        .await;
    assert_eq!(messages(&a.window, &near), vec![(b"back".to_vec(), true)]);
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closing_tells_both_sides_closed_exactly_once() {
    let (a, b) = pair().await;
    let (near, far) = open_test_channel(&a, &b).await;

    a.host.peers.close(a.extension_id, near.id).expect("close");
    b.window.wait_until(|w| !ends(w, &far).is_empty()).await;
    a.window.wait_until(|w| !ends(w, &near).is_empty()).await;
    // Whatever arrives late (the connection's own end) is not told again.
    far.connection.closed().await;
    near.connection.closed().await;
    assert_eq!(ends(&a.window, &near), vec!["closed"]);
    assert_eq!(ends(&b.window, &far), vec!["closed"]);

    assert_eq!(near.send_fast(b"x"), Err(PeerError::ChannelClosed));
    assert_eq!(
        near.send_reliable(b"x".to_vec()),
        Err(PeerError::ChannelClosed)
    );
    assert_eq!(far.send_fast(b"x"), Err(PeerError::ChannelClosed));
    // Both sides forgot the channel: its id is unknown now.
    assert!(matches!(
        a.host.peers.channel(a.extension_id, near.id),
        Err(PeerError::ChannelUnknown)
    ));
    assert!(matches!(
        b.host.peers.channel(b.extension_id, far.id),
        Err(PeerError::ChannelUnknown)
    ));
    a.shutdown().await;
    b.shutdown().await;
}

/// The vault on the other side closes (research R11): its endpoint closes every connection, and
/// this side hears `closed`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_other_vault_closing_ends_the_channel_as_closed() {
    let (a, b) = pair().await;
    let (near, _far) = open_test_channel(&a, &b).await;

    b.shutdown().await;
    a.window.wait_until(|w| !ends(w, &near).is_empty()).await;
    assert_eq!(ends(&a.window, &near), vec!["closed"]);
    a.shutdown().await;
}

/// A side that breaks the protocol on the stream loses the channel: `lost` here, the connection
/// closed for the other side.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_frame_that_is_no_message_ends_the_channel_as_lost() {
    let (a, b) = pair().await;
    let (connection, mut send, _recv, verified) = a.dial(&b).await;
    verified.expect("verified");
    let far = b.accepted().await;

    let mut garbage = 3u32.to_be_bytes().to_vec();
    garbage.extend_from_slice(&[0xff, 0xff, 0xff]);
    tokio::io::AsyncWriteExt::write_all(&mut send, &garbage)
        .await
        .expect("write");
    b.window.wait_until(|w| !ends(w, &far).is_empty()).await;
    assert_eq!(ends(&b.window, &far), vec!["lost"]);
    let closed = tokio::time::timeout(LIMIT, connection.closed())
        .await
        .expect("closed in time");
    assert!(
        matches!(closed, ConnectionError::ApplicationClosed(ref close) if close.error_code == ErrorCode::Protocol.as_u32().into()),
        "{closed:?}"
    );
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_oversized_reliable_message_ends_the_channel_as_lost() {
    let (a, b) = pair().await;
    let (connection, mut send, _recv, verified) = a.dial(&b).await;
    verified.expect("verified");
    let far = b.accepted().await;

    wire::write(
        &mut send,
        &Message::Reliable {
            v: VERSION,
            data: vec![0; RELIABLE_BYTES + 1],
        },
        CHANNEL_FRAME_LIMIT,
    )
    .await
    .expect("write");
    b.window.wait_until(|w| !ends(w, &far).is_empty()).await;
    assert_eq!(ends(&b.window, &far), vec!["lost"]);
    let closed = tokio::time::timeout(LIMIT, connection.closed())
        .await
        .expect("closed in time");
    assert!(
        matches!(closed, ConnectionError::ApplicationClosed(ref close) if close.error_code == ErrorCode::Protocol.as_u32().into()),
        "{closed:?}"
    );
    a.shutdown().await;
    b.shutdown().await;
}

#[test]
fn a_connection_end_maps_to_closed_timeout_or_lost() {
    let application = ConnectionError::ApplicationClosed(ApplicationClose {
        error_code: 3u32.into(),
        reason: Default::default(),
    });
    assert_eq!(end_reason(&application), EndReason::Closed);
    let lost = ConnectionError::ApplicationClosed(ApplicationClose {
        error_code: super::LOST_CODE.into(),
        reason: Default::default(),
    });
    assert_eq!(end_reason(&lost), EndReason::Lost);
    assert_eq!(
        end_reason(&ConnectionError::LocallyClosed),
        EndReason::Closed
    );
    assert_eq!(end_reason(&ConnectionError::TimedOut), EndReason::Timeout);
    for other in [
        ConnectionError::Reset,
        ConnectionError::VersionMismatch,
        ConnectionError::CidsExhausted,
    ] {
        assert_eq!(end_reason(&other), EndReason::Lost, "{other:?}");
    }
    assert_eq!(
        [EndReason::Closed, EndReason::Lost, EndReason::Timeout].map(EndReason::as_str),
        ["closed", "lost", "timeout"]
    );
}
