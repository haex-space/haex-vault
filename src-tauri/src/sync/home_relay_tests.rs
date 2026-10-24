use super::*;
use crate::contacts::card::format::{parse, CardDevice, ContactCard};
use crate::contacts::card::own_card;
use crate::sync::presence::endpoint_addr_of;
use crate::sync::test_support::Member;

const HOME: &str = "https://home.relay.example/";
const OTHER_HOME: &str = "https://other.relay.example/";

/// The address `member`'s endpoint has once it reached the relay `relay`, or no relay at all.
fn address(member: &Member, relay: Option<&str>) -> EndpointAddr {
    endpoint_addr_of(
        &member.keys.endpoint_id,
        relay,
        &["10.0.0.2:4000".to_owned()],
    )
    .expect("an endpoint address")
}

fn card_of(member: &Member) -> ContactCard {
    let card = crate::storage::query::read(member.device.db(), |r| {
        own_card::read(r).map_err(haex_crdt::Error::from)
    })
    .expect("read the own card");
    parse(card.text.as_deref().expect("a card"))
        .expect("a valid card")
        .card
}

fn listed<'a>(card: &'a ContactCard, member: &Member) -> &'a CardDevice {
    card.devices
        .iter()
        .find(|device| device.device_pubkey == member.keys.device_pubkey)
        .expect("the device is on the card")
}

fn note_own(member: &Member, relay: Option<&str>) -> bool {
    super::note_own(
        &member.device.replica,
        &member.keys.device_pubkey,
        &address(member, relay),
    )
    .expect("note the own address")
}

/// The vault's own list change, as the places that sign a list renew the card.
fn renew_card(member: &Member) {
    member
        .device
        .db()
        .write(|tx| own_card::refresh(tx).map_err(haex_crdt::Error::from))
        .expect("renew the card");
}

#[test]
fn a_vault_of_one_device_names_its_own_device_and_relay_on_the_card() {
    let main = Member::genesis();
    let before = card_of(&main);
    assert_eq!(before.devices.len(), 1);
    assert_eq!(listed(&before, &main).iroh_relay, None);

    assert!(note_own(&main, Some(HOME)));

    let after = card_of(&main);
    assert_eq!(after.devices.len(), 1, "no device is added, only its relay");
    assert_eq!(listed(&after, &main).endpoint_id, main.keys.endpoint_id);
    assert_eq!(listed(&after, &main).iroh_relay.as_deref(), Some(HOME));
    assert!(after.issued_at > before.issued_at);
}

#[test]
fn an_unchanged_relay_leaves_the_card_alone() {
    let main = Member::genesis();
    assert!(note_own(&main, Some(HOME)));
    let first = card_of(&main);

    assert!(!note_own(&main, Some(HOME)));

    assert_eq!(card_of(&main), first);
}

#[test]
fn a_changed_home_relay_renews_the_card() {
    let main = Member::genesis();
    assert!(note_own(&main, Some(HOME)));
    let before = card_of(&main);

    assert!(note_own(&main, Some(OTHER_HOME)));

    let after = card_of(&main);
    assert_eq!(
        listed(&after, &main).iroh_relay.as_deref(),
        Some(OTHER_HOME)
    );
    assert!(after.issued_at > before.issued_at);
}

#[test]
fn an_endpoint_without_a_relay_keeps_the_last_one_on_the_card() {
    let main = Member::genesis();
    assert!(note_own(&main, Some(HOME)));
    let before = card_of(&main);

    assert!(!note_own(&main, None));

    assert_eq!(card_of(&main), before);
}

#[test]
fn a_second_device_appears_beside_the_own_one_with_the_relay_it_was_heard_at() {
    let main = Member::genesis();
    let other = Member::join(&main);
    assert!(note_own(&main, Some(HOME)));
    main.add(&other);
    renew_card(&main);

    let listed_only = card_of(&main);
    assert_eq!(listed_only.devices.len(), 2);
    assert_eq!(
        listed(&listed_only, &main).iroh_relay.as_deref(),
        Some(HOME),
        "the own relay stays when the list changes"
    );
    assert_eq!(listed(&listed_only, &other).iroh_relay, None);

    let heard_at = address(&other, Some(OTHER_HOME));
    let renewed = main
        .device
        .db()
        .write(|tx| record_device(tx, &other.keys.device_pubkey, 1_000, Some(&heard_at)))
        .expect("record the second device");
    assert!(renewed);

    let heard = card_of(&main);
    assert_eq!(
        listed(&heard, &other).iroh_relay.as_deref(),
        Some(OTHER_HOME)
    );
    assert_eq!(listed(&heard, &main).iroh_relay.as_deref(), Some(HOME));
    assert!(heard.issued_at > listed_only.issued_at);

    let again = main
        .device
        .db()
        .write(|tx| record_device(tx, &other.keys.device_pubkey, 2_000, Some(&heard_at)))
        .expect("record the second device again");
    assert!(!again, "the same relay again is no change");
    assert_eq!(card_of(&main), heard);
}

#[test]
fn a_linked_device_records_its_relay_but_cannot_renew_the_card() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    let before = card_of(&linked);

    assert!(note_own(&linked, Some(HOME)));

    assert_eq!(card_of(&linked), before);
}
