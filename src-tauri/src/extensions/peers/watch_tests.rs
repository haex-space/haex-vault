//! Channels end with their ground (spec 057 T028, FR-008, FR-034, SC-010): the extension stops,
//! its permission goes, the other vault is blocked or the session ends; the other side hears
//! `closed` each time.

use serde_json::json;

use super::*;
use crate::extensions::peers::channel::{Channel, CHANNEL_CLOSED};
use crate::extensions::peers::invite::invite;
use crate::extensions::peers::invited::Answered;
use crate::extensions::peers::lobby::{Openness, Visibility};
use crate::extensions::peers::prompt::INVITATION;
use crate::extensions::peers::test_scene::Scene;
use crate::extensions::peers::test_support::{
    exchange_cards, open_test_channel, probe, Admit, Vault, Window,
};
use crate::sync::keys;

fn ends(window: &Window, channel: &Channel) -> Vec<String> {
    window
        .frame_events(CHANNEL_CLOSED)
        .into_iter()
        .filter(|(_, _, data)| data["channelId"] == channel.id.to_string())
        .map(|(_, _, data)| data["reason"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// Waits until both sides heard of the end, then checks each heard `closed` exactly once.
async fn both_closed(a: &Vault, near: &Channel, b: &Vault, far: &Channel) {
    a.window.wait_until(|w| !ends(w, near).is_empty()).await;
    b.window.wait_until(|w| !ends(w, far).is_empty()).await;
    assert_eq!(ends(&a.window, near), vec!["closed"]);
    assert_eq!(ends(&b.window, far), vec!["closed"]);
}

async fn contacts() -> (Vault, Vault) {
    let (a, b) = (
        Vault::start(Admit::AtOnce).await,
        Vault::start(Admit::AtOnce).await,
    );
    exchange_cards(&a, &b).await;
    (a, b)
}

/// Every place an extension stops calls `end_all`: its last frame closed, disabled or removed
/// here, a development version unloaded, disabled or removed on another device.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ending_everything_of_an_extension_closes_its_channels_on_both_sides() {
    let (a, b) = contacts().await;
    let (near, far) = open_test_channel(&a, &b).await;
    let (second, second_far) = open_test_channel(&a, &b).await;

    a.host.peers.end_all(a.extension_id);
    both_closed(&a, &near, &b, &far).await;
    both_closed(&a, &second, &b, &second_far).await;
    assert!(a.host.peers.extensions().is_empty());
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn revoking_peers_closes_the_channels_of_that_extension() {
    let (a, b) = contacts().await;
    a.grant_peers("granted");
    let (near, far) = open_test_channel(&a, &b).await;
    let device = a.db.device_id();

    recheck(&a.db, &a.host, device, Recheck::ALL).await;
    assert!(ends(&a.window, &near).is_empty(), "granted: nothing ends");

    a.grant_peers("denied");
    recheck(
        &a.db,
        &a.host,
        device,
        Recheck {
            permissions: true,
            contacts: false,
        },
    )
    .await;
    both_closed(&a, &near, &b, &far).await;
    a.shutdown().await;
    b.shutdown().await;
}

/// A lobby is open state of its extension like a channel: revoking `peers` closes it too.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn revoking_peers_closes_the_lobbies_of_that_extension() {
    let a = Vault::start(Admit::AtOnce).await;
    a.grant_peers("granted");
    a.open_lobby("coop", Visibility::Public);
    let device = a.db.device_id();

    recheck(&a.db, &a.host, device, Recheck::ALL).await;
    assert_eq!(
        a.host.peers.openness(),
        Openness::Public,
        "granted: it stays"
    );

    a.grant_peers("denied");
    recheck(&a.db, &a.host, device, Recheck::ALL).await;
    assert_eq!(a.host.peers.openness(), Openness::None);
    a.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn blocking_the_other_vault_closes_its_channels() {
    let (a, b) = contacts().await;
    a.grant_peers("granted");
    let (near, far) = open_test_channel(&a, &b).await;
    let device = a.db.device_id();

    recheck(&a.db, &a.host, device, Recheck::ALL).await;
    assert!(ends(&a.window, &near).is_empty(), "a contact: nothing ends");

    ContactsService::new(a.db.clone())
        .set_blocked(&Caller::User, keys::hex(&b.vault()), true)
        .await
        .expect("block");
    recheck(
        &a.db,
        &a.host,
        device,
        Recheck {
            permissions: false,
            contacts: true,
        },
    )
    .await;
    both_closed(&a, &near, &b, &far).await;
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_end_of_the_session_closes_every_channel() {
    let (a, b) = contacts().await;
    let (near, far) = open_test_channel(&a, &b).await;

    a.host.peers.clear();
    both_closed(&a, &near, &b, &far).await;
    a.shutdown().await;
    b.shutdown().await;
}

#[test]
fn a_change_asks_to_check_what_its_tables_touch() {
    let tables = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        Recheck::of(&tables(&["extension_permissions"])),
        Recheck {
            permissions: true,
            contacts: false
        }
    );
    assert_eq!(
        Recheck::of(&tables(&["haex_contacts_blocked_vaults", "chat_threads"])),
        Recheck {
            permissions: false,
            contacts: true
        }
    );
    assert!(!Recheck::of(&tables(&["chat_threads"])).any());
}

/// Only a denial ends channels: a channel an invitation opened for an extension whose `peers` nobody
/// has decided survives a change of any permission, whether the row is missing or says "ask", and
/// a later denial ends it on both sides.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_undecided_peers_permission_keeps_an_invited_channel_until_it_is_denied() {
    let scene = Scene::start().await;
    let (a, b1) = (&scene.a, &scene.b1);
    let id = invite(
        &a.host.peers,
        a.extension_id,
        probe(),
        &scene.b_on_a,
        &json!({}),
    )
    .await
    .expect("invite");
    b1.window
        .wait_until(|w| !w.window_events(INVITATION).is_empty())
        .await;
    assert_eq!(b1.host.peers.answer_invitation(id, true), Answered::Open);
    a.window
        .wait_until(|w| !w.frame_events("peers:invite-status").is_empty())
        .await;
    let near = a.host.peers.remotes()[0].0;
    let near = a.host.peers.channel(a.extension_id, near).expect("channel");
    let far = loop {
        if let Some((id, _)) = b1.host.peers.remotes().first() {
            break b1
                .host
                .peers
                .channel(b1.extension_id, *id)
                .expect("channel");
        }
        tokio::task::yield_now().await;
    };
    let device = b1.db.device_id();

    recheck(&b1.db, &b1.host, device, Recheck::ALL).await;
    assert!(ends(&b1.window, &far).is_empty(), "no row: it stays");
    b1.grant_peers("ask");
    recheck(&b1.db, &b1.host, device, Recheck::ALL).await;
    assert!(ends(&b1.window, &far).is_empty(), "ask: it stays");

    b1.grant_peers("denied");
    recheck(&b1.db, &b1.host, device, Recheck::ALL).await;
    both_closed(b1, &far, a, &near).await;
    scene.shutdown().await;
}
