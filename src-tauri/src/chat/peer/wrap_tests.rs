//! The chat wrap's shape (spec 072 T024, contracts/nostr-chat.md "Gift wrap") and the envelope's
//! limits (research R2).

use nostr::event::Kind;
use nostr::types::Timestamp;

use super::*;
use crate::chat::peer::envelope::{self, To, CHAT_KIND, MAX_RUMOR_BYTES};
use crate::sync::keys::{hex, DeviceKeys};

fn tag_value(event: &Event, kind: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|tag| tag.kind() == kind)
        .and_then(|tag| tag.content())
        .map(str::to_owned)
}

#[test]
fn a_chat_wrap_is_a_stored_gift_wrap_to_one_device_that_expires() {
    let sender = DeviceKeys::generate();
    let recipient = DeviceKeys::generate();
    let now = Timestamp::now().as_secs();

    let event = wrap(
        CHAT_KIND,
        "{}",
        &sender.device_secret,
        &recipient.device_pubkey,
    )
    .expect("wrap");

    assert_eq!(
        event.kind,
        Kind::GiftWrap,
        "kind 1059, stored by Nostr-Relays"
    );
    assert_ne!(
        event.pubkey.to_bytes(),
        sender.device_pubkey,
        "one-time key"
    );
    assert_eq!(tag_value(&event, "p"), Some(hex(&recipient.device_pubkey)));
    let expiration: u64 = tag_value(&event, "expiration")
        .expect("expiration")
        .parse()
        .expect("seconds");
    assert!(expiration >= now + EXPIRES_AFTER_SECS);
    assert!(expiration <= Timestamp::now().as_secs() + EXPIRES_AFTER_SECS);
    let created = event.created_at.as_secs();
    assert!(created <= Timestamp::now().as_secs());
    assert!(created + BACKDATE_SECS >= now, "at most 48 h back");
}

#[test]
fn the_recipient_device_opens_it_and_learns_the_sealing_device() {
    let sender = DeviceKeys::generate();
    let recipient = DeviceKeys::generate();
    let event = wrap(
        CHAT_KIND,
        "hallo",
        &sender.device_secret,
        &recipient.device_pubkey,
    )
    .expect("wrap");

    let opened = unwrap(&event, &recipient.device_secret).expect("open");
    assert_eq!(opened.sender_device, sender.device_pubkey);
    assert_eq!(opened.kind, CHAT_KIND);
    assert_eq!(opened.content, "hallo");

    let other = DeviceKeys::generate();
    assert!(
        unwrap(&event, &other.device_secret).is_err(),
        "only that device"
    );
}

#[test]
fn the_largest_rumor_fits_the_wrap_limit() {
    let sender = DeviceKeys::generate();
    let recipient = DeviceKeys::generate();
    let content = "x".repeat(MAX_RUMOR_BYTES);
    let event = wrap(
        CHAT_KIND,
        &content,
        &sender.device_secret,
        &recipient.device_pubkey,
    )
    .expect("a 24 KiB rumor fits");
    assert!(event.as_json().len() <= MAX_WRAP_BYTES);

    let too_big = "x".repeat(MAX_WRAP_BYTES);
    assert!(wrap(
        CHAT_KIND,
        &too_big,
        &sender.device_secret,
        &recipient.device_pubkey
    )
    .is_err());
}

#[test]
fn a_rumor_over_24_kib_or_a_text_over_4000_characters_is_refused() {
    let at = 1_761_900_000_000;
    let text = "ä".repeat(envelope::MAX_TEXT_CHARS);
    let message = envelope::message(&text, "aa", "holzi:card:x", To::Direct("bb".into()), at)
        .expect("4,000 characters are fine");
    envelope::encode(&message).expect("fits in 24 KiB");

    let longer = format!("{text}ä");
    assert!(envelope::message(&longer, "aa", "card", To::Direct("bb".into()), at).is_err());

    let huge_card = "c".repeat(MAX_RUMOR_BYTES);
    let message = envelope::message("hi", "aa", &huge_card, To::Direct("bb".into()), at)
        .expect("the text is fine");
    assert!(
        envelope::encode(&message).is_err(),
        "the rumor is too large"
    );
}

#[test]
fn the_envelope_reads_back_and_an_unknown_type_does_not() {
    let message = envelope::message(
        "Kommst du?",
        "aa",
        "holzi:card:x",
        To::Direct("bb".into()),
        5,
    )
    .expect("message");
    let json = envelope::encode(&message).expect("json");
    assert!(json.contains("\"type\":\"message\""));
    assert!(json.contains("\"to\":{\"direct\":\"bb\"}"));
    let back: envelope::Envelope = serde_json::from_str(&json).expect("decode");
    assert_eq!(back, message);

    let unknown = json.replace("\"message\"", "\"poll\"");
    assert!(serde_json::from_str::<envelope::Envelope>(&unknown).is_err());
}
