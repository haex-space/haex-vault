//! Channels end with their ground (spec 057 T028, FR-008, FR-034, SC-010): the extension stops,
//! its permission goes, the other vault is blocked or the session ends; the other side hears
//! `closed` each time.

use super::*;
use crate::extensions::peers::channel::{Channel, CHANNEL_CLOSED};
use crate::extensions::peers::test_support::{
    exchange_cards, open_test_channel, Admit, Vault, Window,
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
