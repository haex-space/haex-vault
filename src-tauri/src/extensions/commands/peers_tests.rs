//! The overflow report of a frame's queue (spec 057 T067, FR-038): the channel ends with `lost`
//! on both sides, and only for the frame of the extension that owns it.

use super::*;
use crate::extensions::peers::channel::{CHANNEL_CLOSED, MESSAGE};
use crate::extensions::peers::test_support::{install, open_test_channel, Admit, Vault};

fn reasons(vault: &Vault) -> Vec<String> {
    vault
        .window
        .frame_events(CHANNEL_CLOSED)
        .into_iter()
        .map(|(_, _, data)| data["reason"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_overflow_ends_the_channel_with_lost_on_both_sides_once() {
    let a = Vault::start(Admit::AtOnce).await;
    let b = Vault::start(Admit::AtOnce).await;
    let (near, _far) = open_test_channel(&a, &b).await;
    let frame = a.host.frames.of_extension(a.extension_id)[0].frame.clone();

    frame_overflow(&a.host, &frame, &near.id.to_string());
    frame_overflow(&a.host, &frame, &near.id.to_string());
    for vault in [&a, &b] {
        vault
            .window
            .wait_until(|w| !w.frame_events(CHANNEL_CLOSED).is_empty())
            .await;
        assert_eq!(reasons(vault), ["lost"]);
    }
    assert!(a.host.peers.channel(a.extension_id, near.id).is_err());
    assert!(b.window.frame_events(MESSAGE).is_empty());
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_overflow_of_another_extension_a_missing_frame_or_a_bad_id_ends_nothing() {
    let a = Vault::start(Admit::AtOnce).await;
    let b = Vault::start(Admit::AtOnce).await;
    let (near, _far) = open_test_channel(&a, &b).await;
    let other = install(&a.db, "other-probe");
    let foreign = a.open_frame(other);
    let own = a.host.frames.of_extension(a.extension_id)[0].frame.clone();
    let id = near.id.to_string();

    frame_overflow(&a.host, &foreign, &id);
    frame_overflow(&a.host, "no-such-frame", &id);
    frame_overflow(&a.host, &own, "not-an-id");
    frame_overflow(&a.host, &own, &Uuid::new_v4().to_string());
    assert!(!near.is_closed());
    assert!(a.window.frame_events(CHANNEL_CLOSED).is_empty());
    a.shutdown().await;
    b.shutdown().await;
}
