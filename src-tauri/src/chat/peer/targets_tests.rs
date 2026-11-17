//! Which vault a device belongs to (spec 095 T006): the own vault, a contact or an open request,
//! never this device, a removed device or a blocked vault.

use super::*;
use crate::chat::peer::delivery::now_ms;
use crate::chat::peer::test_support::ChatDevice;
use crate::sync::keys::DeviceKeys;

const RELAY: &str = "wss://relay.example.org";

fn vault_of(device: &ChatDevice, of: [u8; 32]) -> Option<String> {
    let me = device.me.clone();
    device
        .db
        .read_blocking(move |r| Ok(vault_of_device(r, &me, &of)?))
        .expect("read")
}

#[test]
fn the_own_other_devices_belong_to_the_own_vault_until_removed() {
    let anna = ChatDevice::genesis(&[RELAY.to_owned()]);
    assert_eq!(
        vault_of(&anna, anna.me.keys.device_pubkey),
        None,
        "this device is no peer"
    );
    let phone = anna.link();
    let removed = phone.me.keys.device_pubkey;
    assert_eq!(vault_of(&anna, removed), Some(anna.me.vault_hex.clone()));

    anna.member.issue_list(|mut list| {
        list.devices
            .retain(|device| device.device_pubkey != removed);
        list
    });
    assert_eq!(vault_of(&anna, removed), None, "a removed device");
}

#[tokio::test]
async fn a_contact_device_belongs_to_the_contact_until_it_is_blocked() {
    let anna = ChatDevice::genesis(&[RELAY.to_owned()]);
    let ben = ChatDevice::genesis(&[RELAY.to_owned()]);
    anna.learn(&ben).await;
    let ben_device = ben.me.keys.device_pubkey;
    assert_eq!(vault_of(&anna, ben_device), Some(ben.me.vault_hex.clone()));
    assert_eq!(
        vault_of(&anna, DeviceKeys::generate().device_pubkey),
        None,
        "a device on no card"
    );

    let vault = ben.me.vault_hex.clone();
    anna.db
        .write_blocking(move |tx| Ok(received::block(tx, &vault)?))
        .expect("block");
    assert_eq!(vault_of(&anna, ben_device), None, "a blocked vault");
}

#[test]
fn a_device_on_the_card_of_an_open_request_belongs_to_that_vault() {
    let anna = ChatDevice::genesis(&[RELAY.to_owned()]);
    let stranger = ChatDevice::genesis(&[RELAY.to_owned()]);
    let card = stranger.card();
    let (own, peer) = (anna.me.vault_hex.clone(), stranger.me.vault_hex.clone());
    anna.db
        .write_blocking(move |tx| {
            let thread = store::open_direct(tx, &own, &peer, now_ms())?;
            Ok(store::mark_request(tx, thread, &peer, &card)?)
        })
        .expect("request");
    assert_eq!(
        vault_of(&anna, stranger.me.keys.device_pubkey),
        Some(stranger.me.vault_hex.clone())
    );

    let vault = stranger.me.vault_hex.clone();
    anna.db
        .write_blocking(move |tx| Ok(received::block(tx, &vault)?))
        .expect("block");
    assert_eq!(vault_of(&anna, stranger.me.keys.device_pubkey), None);
}
