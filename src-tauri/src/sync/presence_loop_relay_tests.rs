use std::net::Ipv4Addr;

use super::tests::bind_loopback;
use super::*;
use crate::contacts::card::format::parse;
use crate::contacts::card::own_card;
use crate::sync::endpoint::SyncNode;
use crate::sync::presence::day_tag_now;
use crate::sync::test_support::Member;

const LINKED_RELAY: &str = "https://linked.relay.example/";

/// A meeting `signer` published for its own endpoint, naming `relay`, sealed for `receiver`'s
/// mailbox of `day`.
fn meeting_at(receiver: &Member, signer: &DeviceKeys, relay: &str, day: u32) -> Event {
    let content_key = crate::storage::query::read(receiver.device.db(), |r| {
        crate::sync::content_keys::current_key(r, &[])
    })
    .expect("read")
    .expect("a content key");
    let (_, mb_pk) = mailbox_keys(&content_key.key, day).expect("mailbox keys");
    let content = PresenceContent::own(
        signer.device_pubkey,
        signer.endpoint_id,
        Some(relay.to_owned()),
        vec![(Ipv4Addr::LOCALHOST, 4000).into()],
        2,
    );
    build(signer, &content, &mb_pk).expect("build")
}

fn card_relay_of(member: &Member, device: &DeviceKeys) -> (Option<String>, u64) {
    let text = crate::storage::query::read(member.device.db(), |r| {
        own_card::stored_text(r).map_err(haex_crdt::Error::from)
    })
    .expect("read the own card");
    let card = parse(text.as_deref().expect("a card"))
        .expect("a valid card")
        .card;
    let relay = card
        .devices
        .iter()
        .find(|listed| listed.device_pubkey == device.device_pubkey)
        .expect("the device is on the card")
        .iroh_relay
        .clone();
    (relay, card.issued_at)
}

/// The vault's own list change, as the places that sign a list renew the card.
fn renew_card(member: &Member) {
    member
        .device
        .db()
        .write(|tx| own_card::refresh(tx).map_err(haex_crdt::Error::from))
        .expect("renew the card");
}

async fn hear(main: &Member, node: &SyncNode, event: &Event) -> bool {
    handle_incoming(
        node,
        &main.device.replica,
        &main.keys,
        main.vault,
        day_tag_now(),
        event,
    )
    .await
}

#[tokio::test]
async fn a_meeting_that_names_a_new_home_relay_renews_the_own_card() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    renew_card(&main);
    let node = bind_loopback(&main).await;
    let event = meeting_at(&main, &linked.keys, LINKED_RELAY, day_tag_now());
    let (_, issued_before) = card_relay_of(&main, &linked.keys);

    assert!(hear(&main, &node, &event).await);

    let (relay, issued_after) = card_relay_of(&main, &linked.keys);
    assert_eq!(relay.as_deref(), Some(LINKED_RELAY));
    assert!(issued_after > issued_before);
}

#[tokio::test]
async fn a_repeated_meeting_at_the_same_relay_leaves_the_own_card_alone() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    renew_card(&main);
    let node = bind_loopback(&main).await;
    let day = day_tag_now();
    assert!(
        hear(
            &main,
            &node,
            &meeting_at(&main, &linked.keys, LINKED_RELAY, day)
        )
        .await
    );
    let (_, issued) = card_relay_of(&main, &linked.keys);

    assert!(
        hear(
            &main,
            &node,
            &meeting_at(&main, &linked.keys, LINKED_RELAY, day)
        )
        .await
    );

    assert_eq!(card_relay_of(&main, &linked.keys).1, issued);
}
