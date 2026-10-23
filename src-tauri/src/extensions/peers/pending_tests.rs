//! An accepted invitation whose extension has no frame (spec 057 T041, FR-016, research R9): the
//! window opens the extension, and the channel's first event and its messages wait for a frame.

use std::time::Duration;

use serde_json::json;

use super::*;
use crate::extensions::notifications::{NotificationResponse, CLICKED};
use crate::extensions::peers::channel::MESSAGE;
use crate::extensions::peers::invite::{invite, CHANNEL_OPENED};
use crate::extensions::peers::prompt::INVITATION;
use crate::extensions::peers::state::Timing;
use crate::extensions::peers::test_scene::Scene;
use crate::extensions::peers::test_support::{probe, Vault, LIMIT};

fn waiting(frames: &Frames, held: &mut PendingOpens, channel: Uuid) {
    held.insert(
        channel,
        Uuid::nil(),
        frames.clone(),
        CHANNEL_OPENED,
        json!("opened"),
        CancellationToken::new(),
    );
}

/// Waits until `held` holds, polling without a sleep, at most [`LIMIT`].
async fn until(held: impl Fn() -> bool) {
    tokio::time::timeout(LIMIT, async {
        while !held() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("it happened in time");
}

/// B has no frame; A invites; B's notification is accepted by its button.
async fn accepted_without_a_frame(scene: &Scene) -> Uuid {
    scene.b1.close_frames();
    let id = invite(
        &scene.a.host.peers,
        scene.a.extension_id,
        probe(),
        &scene.b_on_a,
        &json!({ "mode": "duel" }),
    )
    .await
    .expect("invite");
    scene.b1.desktop.wait_shown(1, LIMIT).await;
    scene
        .b1
        .desktop
        .answer(0, NotificationResponse::Button("accept".to_owned()));
    scene
        .a
        .window
        .wait_until(|w| !w.frame_events("peers:invite-status").is_empty())
        .await;
    // B has made the channel when its window is asked to open the extension.
    scene
        .b1
        .window
        .wait_until(|w| !w.window_events(CLICKED).is_empty())
        .await;
    id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_extension_is_opened_and_hears_the_channel_first() {
    let scene = Scene::start().await;
    let (a, b1) = (&scene.a, &scene.b1);
    accepted_without_a_frame(&scene).await;

    // holzi's window is asked to open the extension; the notification was the only prompt.
    b1.window
        .wait_until(|w| !w.window_events(CLICKED).is_empty())
        .await;
    assert_eq!(
        b1.window.window_events(CLICKED)[0],
        json!({ "extensionId": b1.extension_id.to_string() })
    );
    assert!(b1.window.window_events(INVITATION).is_empty());
    assert_eq!(b1.window.all_frame_events(), 0, "no frame to tell yet");

    // A talks right away; the messages wait with the announcement.
    let near = a.host.peers.remotes()[0].0;
    let near = a.host.peers.channel(a.extension_id, near).expect("channel");
    near.send_reliable(b"first".to_vec()).expect("send");
    let far = b1.host.peers.remotes()[0].0;
    until(|| b1.host.peers.held(far) == 2).await;

    // The frame reports its handshake: the announcement goes first, then the message.
    let frame = b1.open_frame(b1.extension_id);
    let events: Vec<String> = b1
        .window
        .window_events("extension-frame-event")
        .iter()
        .map(|e| e["type"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(events, [CHANNEL_OPENED, MESSAGE]);
    assert!(b1
        .window
        .window_events("extension-frame-event")
        .iter()
        .all(|e| e["frame"] == frame));
    assert_eq!(b1.host.peers.held(far), 0);
    scene.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_extension_with_a_frame_hears_the_channel_at_once() {
    let scene = Scene::start().await;
    let id = invite(
        &scene.a.host.peers,
        scene.a.extension_id,
        probe(),
        &scene.b_on_a,
        &json!({}),
    )
    .await
    .expect("invite");
    scene
        .b1
        .window
        .wait_until(|w| !w.window_events(INVITATION).is_empty())
        .await;
    scene.b1.host.peers.answer_invitation(id, true);
    scene
        .b1
        .window
        .wait_until(|w| !w.frame_events(CHANNEL_OPENED).is_empty())
        .await;

    assert!(scene.b1.window.window_events(CLICKED).is_empty());
    scene.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_channel_that_finds_no_frame_closes() {
    let scene = Scene::start().await;
    scene.b1.host.peers.set_timing(Timing {
        pending_open: Duration::from_millis(300),
        ..Timing::default()
    });
    accepted_without_a_frame(&scene).await;

    // The inviting side hears the end like any other: `closed`.
    scene
        .a
        .window
        .wait_until(|w| !w.frame_events("peers:channel-closed").is_empty())
        .await;
    assert_eq!(
        scene.a.window.frame_events("peers:channel-closed")[0].2["reason"],
        "closed"
    );
    assert!(scene.b1.host.peers.remotes().is_empty());
    scene.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ending_the_extension_drops_what_waits() {
    let scene = Scene::start().await;
    accepted_without_a_frame(&scene).await;
    let far = scene.b1.host.peers.remotes()[0].0;
    assert_eq!(scene.b1.host.peers.held(far), 1);

    scene.b1.host.peers.end_all(scene.b1.extension_id);
    assert_eq!(scene.b1.host.peers.held(far), 0);
    scene.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn held_events_are_bounded_and_fast_messages_go_first() {
    let vault = Vault::start(crate::extensions::peers::test_support::Admit::AtOnce).await;
    let mut held = PendingOpens::default();
    let channel = Uuid::new_v4();
    waiting(&vault.ctx.frames, &mut held, channel);

    for n in 1..MAX_HELD {
        assert_eq!(held.hold(channel, MESSAGE, json!(n), false), Hold::Held);
    }
    assert_eq!(held.held(channel), MAX_HELD);
    // Full of fast messages and the announcement: a reliable one pushes the oldest fast one out.
    assert_eq!(held.hold(channel, MESSAGE, json!("r"), true), Hold::Held);
    assert_eq!(held.held(channel), MAX_HELD);

    // Once only events that must stay are left, a fast message is let go and a reliable one is
    // too much.
    for _ in 0..MAX_HELD {
        held.hold(channel, MESSAGE, json!("r"), true);
    }
    assert_eq!(held.hold(channel, MESSAGE, json!("f"), false), Hold::Held);
    assert_eq!(held.held(channel), MAX_HELD);
    assert_eq!(
        held.hold(channel, MESSAGE, json!("r"), true),
        Hold::Overflow
    );
    // A channel that waits for nothing gets its message back.
    assert_eq!(
        held.hold(Uuid::new_v4(), MESSAGE, json!("x"), true),
        Hold::Pass(json!("x"))
    );
    vault.shutdown().await;
}
