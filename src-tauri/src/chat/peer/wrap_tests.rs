//! The chat wrap's shape (spec 072 T024, contracts/nostr-chat.md "Gift wrap") and the envelope's
//! limits (research R2).

use nostr::event::Kind;
use nostr::types::Timestamp;

use super::*;
use crate::chat::peer::delivery::now_ms;
use crate::chat::peer::envelope::{self, To, CHAT_KIND};
use crate::chat::peer::ratchet::wire::Plaintext;
use crate::chat::peer::ratchet::MAX_PLAINTEXT_BYTES;
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
        now_ms(),
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

/// Spec 095 T003: the times come from the clock the caller passes, so tests can move it.
#[test]
fn a_chat_wrap_counts_its_times_from_the_given_clock() {
    let sender = DeviceKeys::generate();
    let recipient = DeviceKeys::generate();
    let at_secs: u64 = 1_700_000_000;
    let event = wrap(
        CHAT_KIND,
        "{}",
        &sender.device_secret,
        &recipient.device_pubkey,
        i64::try_from(at_secs * 1000).expect("ms"),
    )
    .expect("wrap");
    let expiration: u64 = tag_value(&event, "expiration")
        .expect("expiration")
        .parse()
        .expect("seconds");
    assert_eq!(expiration, at_secs + EXPIRES_AFTER_SECS);
    let created = event.created_at.as_secs();
    assert!(created <= at_secs && created + BACKDATE_SECS >= at_secs);
    let opened = unwrap(&event, &recipient.device_secret).expect("open");
    assert_eq!(opened.content, "{}");
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
        now_ms(),
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
fn a_content_over_the_wrap_limit_is_refused() {
    let sender = DeviceKeys::generate();
    let recipient = DeviceKeys::generate();
    let too_big = "x".repeat(MAX_WRAP_BYTES);
    assert!(wrap(
        CHAT_KIND,
        &too_big,
        &sender.device_secret,
        &recipient.device_pubkey,
        now_ms(),
    )
    .is_err());
}

/// The plaintext of a ratchet message is what an event has to fit since spec 095 (the wrap of the
/// largest one is checked in `ratchet/wire_tests.rs`).
fn fits_one_plaintext(event: &envelope::Envelope) -> bool {
    Plaintext {
        kind: CHAT_KIND.as_u16(),
        content: envelope::encode(event).expect("encode"),
    }
    .encode()
    .is_ok()
}

#[test]
fn a_text_at_the_bound_or_a_huge_card_does_not_fit_one_plaintext() {
    let at = 1_761_900_000_000;
    let text = "ä".repeat(envelope::MAX_TEXT_CHARS);
    let message = envelope::message(
        &text,
        Vec::new(),
        "aa",
        "haex:card:x",
        To::Direct("bb".into()),
        at,
    )
    .expect("the bound itself is fine");
    assert!(
        !fits_one_plaintext(&message),
        "but it does not fit one message: such a text goes as a file"
    );

    let longer = format!("{text}ä");
    let error = envelope::message(
        &longer,
        Vec::new(),
        "aa",
        "card",
        To::Direct("bb".into()),
        at,
    )
    .expect_err("over the bound");
    assert!(error.to_string().contains("chat.error.tooLong"), "{error}");

    let huge_card = "c".repeat(MAX_PLAINTEXT_BYTES);
    let message = envelope::message(
        "hi",
        Vec::new(),
        "aa",
        &huge_card,
        To::Direct("bb".into()),
        at,
    )
    .expect("the text is fine");
    assert!(!fits_one_plaintext(&message), "the event is too large");
}

/// Spec 095 T023: chat events travel only as ratchet messages. Only `delivery_send.rs` wraps, and
/// it wraps no chat event or receipt directly. If this fails, a production file found a way around
/// the sessions: route it through `delivery_send.rs` instead.
#[test]
fn only_sending_on_sessions_wraps_and_never_a_chat_event_directly() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/chat/peer");
    let mut files = Vec::new();
    sources_below(&dir, &mut files);
    assert!(
        files.len() > 20,
        "the chat sources were found: {}",
        files.len()
    );
    let mut hits = Vec::new();
    for file in &files {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let text = std::fs::read_to_string(file).unwrap_or_default();
        for (number, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            let wraps = code.contains("wrap::wrap(")
                || (code.contains("wrap_shaped_at(") && name != "wrap.rs");
            let wraps_elsewhere = wraps && name != "delivery_send.rs";
            let names_event_kind = name == "delivery_send.rs"
                && (code.contains("CHAT_KIND") || code.contains("RECEIPT_KIND"));
            if wraps_elsewhere || names_event_kind {
                hits.push(format!(
                    "{}:{}: {}",
                    file.display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "wraps outside the sessions:\n{}",
        hits.join("\n")
    );
}

/// Rust sources below `dir`, tests and test support left out.
fn sources_below(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources_below(&path, out);
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name.ends_with(".rs") && !name.contains("_tests.") && !name.contains("test_support") {
            out.push(path);
        }
    }
}

#[test]
fn the_envelope_reads_back_and_an_unknown_type_does_not() {
    let message = envelope::message(
        "Kommst du?",
        Vec::new(),
        "aa",
        "haex:card:x",
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
