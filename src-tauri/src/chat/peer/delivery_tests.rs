//! Where wraps go (spec 072 T035, T046, FR-016, FR-082, US2 scenario 8).

use crate::chat::peer::targets;
use crate::chat::peer::test_support::ChatDevice;
use crate::contacts::card::own_card;

const RELAY: &str = "wss://relay.example.org";

fn target(device: &ChatDevice, vault: &str) -> targets::Target {
    let (me, vault) = (device.me.clone(), vault.to_owned());
    device
        .db
        .read_blocking(move |r| Ok(targets::of(r, &me, &vault)?))
        .expect("target")
}

#[test]
fn the_subscription_listens_on_the_nostr_relays_of_the_own_card() {
    let anna = ChatDevice::genesis(&[RELAY.to_owned(), "wss://zwei.example.org".to_owned()]);
    let (listening, on_card) = anna
        .db
        .read_blocking(|r| {
            Ok((
                targets::own_relays(r)?,
                own_card::stored(r)?.expect("card").card.nostr_relays,
            ))
        })
        .expect("read");
    assert_eq!(listening, on_card, "FR-082");
    assert_eq!(listening.len(), 2);
}

#[test]
fn own_copies_go_to_listed_other_devices_and_never_to_removed_ones() {
    let anna = ChatDevice::genesis(&[RELAY.to_owned()]);
    let own = anna.me.vault_hex.clone();
    assert!(
        target(&anna, &own).devices.is_empty(),
        "a single device has no own copies to send"
    );

    let phone = anna.link();
    let to_own = target(&anna, &own);
    assert_eq!(to_own.devices, [phone.me.keys.device_pubkey]);
    assert_eq!(to_own.relays, [RELAY.to_owned()]);
    assert_eq!(
        target(&phone, &own).devices,
        [anna.me.keys.device_pubkey],
        "and the other way round"
    );

    let removed = phone.me.keys.device_pubkey;
    anna.member.issue_list(|mut list| {
        list.devices
            .retain(|device| device.device_pubkey != removed);
        list
    });
    assert!(
        target(&anna, &own).devices.is_empty(),
        "a removed device gets no wraps"
    );
}

#[tokio::test]
async fn a_contact_card_without_nostr_relays_falls_back_to_the_own_ones() {
    let anna = ChatDevice::genesis(&[RELAY.to_owned()]);
    let ben = ChatDevice::genesis(&[]);
    anna.learn(&ben).await;
    let to_ben = target(&anna, &ben.me.vault_hex);
    assert_eq!(to_ben.devices, [ben.me.keys.device_pubkey]);
    assert_eq!(to_ben.relays, [RELAY.to_owned()]);
    assert!(to_ben.own_relays_instead, "shown as a hint");

    let carla = ChatDevice::genesis(&["wss://carla.example.org".to_owned()]);
    anna.learn(&carla).await;
    let to_carla = target(&anna, &carla.me.vault_hex);
    assert_eq!(to_carla.relays, ["wss://carla.example.org".to_owned()]);
    assert!(!to_carla.own_relays_instead);
    assert!(
        target(&anna, &"c3".repeat(32)).devices.is_empty(),
        "a vault without a card has no devices"
    );
}
