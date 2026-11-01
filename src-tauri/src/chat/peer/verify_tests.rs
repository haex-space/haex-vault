//! Every receiver check of contracts/nostr-chat.md (spec 072 T025, research R5, quickstart A2).

use super::*;
use crate::chat::peer::envelope::{self, Envelope, MessageBody};
use crate::chat::peer::test_support::ChatDevice;
use crate::contacts::card::format::{sign, CardDevice, ContactCard, VERSION as CARD_VERSION};
use crate::sync::signing;

const BEN_SECRET: [u8; 32] = [21; 32];
const BEN_DEVICE: [u8; 32] = [3; 32];
const RELAY: &str = "wss://relay.example.org";

fn ben_vault() -> [u8; 32] {
    signing::xonly_public_key(&BEN_SECRET).expect("a key")
}

fn ben_card(issued_at: u64, devices: &[[u8; 32]]) -> SignedCard {
    let card = ContactCard {
        version: CARD_VERSION,
        vault: ben_vault(),
        issued_at,
        list_generation: 1,
        devices: devices
            .iter()
            .map(|device| CardDevice {
                device_pubkey: *device,
                endpoint_id: [4; 32],
                iroh_relay: None,
            })
            .collect(),
        nostr_relays: vec![RELAY.to_owned()],
        name: "Ben".to_owned(),
        values: vec![],
    };
    sign(card, &BEN_SECRET).expect("sign").0
}

fn from_ben(anna: &ChatDevice, card: &SignedCard) -> Envelope {
    envelope::message(
        "Hallo",
        &hex(&ben_vault()),
        &card.text(),
        To::Direct(anna.me.vault_hex.clone()),
        1,
    )
    .expect("message")
}

fn opened(envelope: &impl serde::Serialize, kind: nostr::event::Kind, device: [u8; 32]) -> Opened {
    Opened {
        sender_device: device,
        kind,
        content: serde_json::to_string(envelope).expect("json"),
    }
}

fn run(anna: &ChatDevice, opened: Opened) -> std::result::Result<Verified, Drop> {
    let own = anna.me.vault;
    anna.db
        .write_blocking(move |tx| Ok(check(tx, &own, &opened)?))
        .expect("check")
}

async fn anna_knowing_ben(issued_at: u64) -> ChatDevice {
    let anna = ChatDevice::genesis(&[RELAY.to_owned()]);
    anna.learn_card(ben_card(issued_at, &[BEN_DEVICE])).await;
    anna
}

#[test]
fn a_wrap_is_opened_once() {
    let anna = ChatDevice::genesis(&[]);
    let first = anna
        .db
        .write_blocking(|tx| Ok(first_sight(tx, "w1", 1)?))
        .expect("first");
    let again = anna
        .db
        .write_blocking(|tx| Ok(first_sight(tx, "w1", 2)?))
        .expect("again");
    assert!(first);
    assert!(!again);
    anna.db
        .write_blocking(|tx| Ok(prune_seen(tx, 1 + SEEN_KEEP_MS + 1)?))
        .expect("prune");
    assert!(anna
        .db
        .write_blocking(|tx| Ok(first_sight(tx, "w1", 3)?))
        .expect("after pruning"));
}

#[tokio::test]
async fn a_message_of_a_contact_passes_and_tells_the_sender() {
    let anna = anna_knowing_ben(10).await;
    let card = ben_card(10, &[BEN_DEVICE]);
    let verified = run(
        &anna,
        opened(&from_ben(&anna, &card), CHAT_KIND, BEN_DEVICE),
    )
    .expect("passes");
    assert_eq!(verified.from, hex(&ben_vault()));
    assert_eq!(verified.sender_device, hex(&BEN_DEVICE));
    assert!(matches!(verified.incoming, Incoming::Message(_)));
}

#[tokio::test]
async fn a_card_with_a_bad_signature_is_dropped() {
    let anna = anna_knowing_ben(10).await;
    let card = ben_card(10, &[BEN_DEVICE]);
    let mut message = from_ben(&anna, &card);
    let mut bytes = card.bytes.clone();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    message.card = SignedCard {
        card: card.card.clone(),
        bytes,
    }
    .text();
    assert_eq!(
        run(&anna, opened(&message, CHAT_KIND, BEN_DEVICE)),
        Err(Drop::BadCard)
    );
}

#[tokio::test]
async fn an_older_card_is_dropped_and_a_newer_one_replaces_the_stored_one() {
    let anna = anna_knowing_ben(10).await;
    let older = ben_card(5, &[BEN_DEVICE]);
    assert_eq!(
        run(
            &anna,
            opened(&from_ben(&anna, &older), CHAT_KIND, BEN_DEVICE)
        ),
        Err(Drop::OlderCard)
    );

    let newer = ben_card(20, &[BEN_DEVICE, [7; 32]]);
    run(&anna, opened(&from_ben(&anna, &newer), CHAT_KIND, [7; 32])).expect("passes");
    let vault = ben_vault();
    let stored = anna
        .db
        .read_blocking(move |r| Ok(received::stored(r, &vault)?))
        .expect("read")
        .expect("card");
    assert_eq!(
        stored.1.card.issued_at, 20,
        "FR-020: the newer card is kept"
    );
}

#[tokio::test]
async fn a_seal_key_that_is_not_on_the_card_is_dropped() {
    let anna = anna_knowing_ben(10).await;
    let card = ben_card(10, &[BEN_DEVICE]);
    assert_eq!(
        run(&anna, opened(&from_ben(&anna, &card), CHAT_KIND, [9; 32])),
        Err(Drop::UnknownDevice)
    );
}

#[tokio::test]
async fn a_from_that_is_not_the_cards_vault_is_dropped() {
    let anna = anna_knowing_ben(10).await;
    let card = ben_card(10, &[BEN_DEVICE]);
    let mut message = from_ben(&anna, &card);
    message.from = hex(&[8; 32]);
    assert_eq!(
        run(&anna, opened(&message, CHAT_KIND, BEN_DEVICE)),
        Err(Drop::WrongSender)
    );
}

#[tokio::test]
async fn a_blocked_vault_is_dropped() {
    let anna = anna_knowing_ben(10).await;
    let ben = hex(&ben_vault());
    anna.db
        .write_blocking(move |tx| Ok(received::block(tx, &ben)?))
        .expect("block");
    let card = ben_card(10, &[BEN_DEVICE]);
    let verdict = run(
        &anna,
        opened(&from_ben(&anna, &card), CHAT_KIND, BEN_DEVICE),
    );
    assert!(
        matches!(verdict, Err(Drop::Blocked | Drop::UnknownSender)),
        "{verdict:?}"
    );
}

#[tokio::test]
async fn a_message_to_another_vault_or_from_a_stranger_is_dropped() {
    let anna = anna_knowing_ben(10).await;
    let card = ben_card(10, &[BEN_DEVICE]);
    let mut message = from_ben(&anna, &card);
    message.to = To::Direct(hex(&[6; 32]));
    assert_eq!(
        run(&anna, opened(&message, CHAT_KIND, BEN_DEVICE)),
        Err(Drop::NotForUs)
    );

    let stranger = ChatDevice::genesis(&[]);
    assert_eq!(
        run(
            &stranger,
            opened(&from_ben(&stranger, &card), CHAT_KIND, BEN_DEVICE)
        ),
        Err(Drop::UnknownSender),
        "until US5 a vault without a stored card is dropped"
    );
}

#[tokio::test]
async fn an_unknown_version_kind_or_type_is_dropped() {
    let anna = anna_knowing_ben(10).await;
    let card = ben_card(10, &[BEN_DEVICE]);
    let mut message = from_ben(&anna, &card);
    message.v = 2;
    assert_eq!(
        run(&anna, opened(&message, CHAT_KIND, BEN_DEVICE)),
        Err(Drop::UnknownVersion)
    );
    let message = from_ben(&anna, &card);
    assert_eq!(
        run(
            &anna,
            opened(&message, nostr::event::Kind::Custom(24110), BEN_DEVICE)
        ),
        Err(Drop::UnknownKind)
    );
    let poll = serde_json::to_string(&message)
        .expect("json")
        .replace("\"message\"", "\"poll\"");
    let unknown = Opened {
        sender_device: BEN_DEVICE,
        kind: CHAT_KIND,
        content: poll,
    };
    assert_eq!(run(&anna, unknown), Err(Drop::Malformed));
}

#[tokio::test]
async fn a_receipt_for_the_own_vault_passes() {
    let anna = anna_knowing_ben(10).await;
    let card = ben_card(10, &[BEN_DEVICE]);
    let receipt = Receipt {
        v: VERSION,
        from: hex(&ben_vault()),
        card: card.text(),
        thread: To::Direct(anna.me.vault_hex.clone()),
        delivered: vec!["e1".into()],
        read: vec![],
    };
    let verified = run(&anna, opened(&receipt, RECEIPT_KIND, BEN_DEVICE)).expect("passes");
    assert!(matches!(verified.incoming, Incoming::Receipt(_)));
}

#[test]
fn an_own_message_passes_only_from_a_listed_own_device() {
    let anna = ChatDevice::genesis(&[]);
    let own_card = anna.card();
    let message = Envelope {
        body: envelope::Body::Message(MessageBody {
            text: "an Ben".into(),
        }),
        ..envelope::message(
            "x",
            &anna.me.vault_hex,
            &own_card.text(),
            To::Direct(hex(&ben_vault())),
            1,
        )
        .expect("message")
    };
    let listed = run(
        &anna,
        opened(&message, CHAT_KIND, anna.me.keys.device_pubkey),
    );
    assert!(listed.is_ok(), "{listed:?}");
    assert_eq!(
        run(&anna, opened(&message, CHAT_KIND, [9; 32])),
        Err(Drop::UnknownDevice)
    );
}
