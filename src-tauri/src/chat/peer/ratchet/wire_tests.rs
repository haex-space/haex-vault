//! The ratchet message's layers (spec 095 T005, contracts/nostr-chat-ratchet.md).

use vodozemac::olm::{Account, SessionConfig};

use super::*;
use crate::chat::peer::delivery::now_ms;
use crate::chat::peer::ratchet::RATCHET_KIND;
use crate::chat::peer::wrap::{self, MAX_WRAP_BYTES};
use crate::sync::keys::DeviceKeys;

/// An opening message from a fresh account to a fresh one, with `plaintext`.
fn opening(plaintext: &[u8]) -> OlmMessage {
    let alice = Account::new();
    let mut bob = Account::new();
    bob.generate_fallback_key();
    let start = *bob.fallback_key().values().next().expect("start key");
    let mut session = alice
        .create_outbound_session(SessionConfig::version_1(), bob.curve25519_key(), start)
        .expect("session");
    session.encrypt(plaintext).expect("encrypt")
}

#[test]
fn a_rumor_carries_the_olm_message_and_reads_back() {
    let message = opening(b"hallo");
    let rumor = RatchetRumor::of(&message);
    assert!(rumor.is_opening(), "the first message opens");
    let text = rumor.encode().expect("encode");
    assert!(text.starts_with(r#"{"v":1,"type":0,"body":""#));
    let back = RatchetRumor::decode(&text).expect("decode");
    assert_eq!(back, rumor);
    assert_eq!(back.message().expect("message"), message);
}

#[test]
fn a_rumor_of_another_version_or_type_is_refused() {
    let rumor = RatchetRumor::of(&opening(b"x")).encode().expect("encode");
    assert_eq!(
        RatchetRumor::decode(&rumor.replace(r#""v":1"#, r#""v":2"#)),
        Err(Drop::UnknownVersion)
    );
    assert_eq!(
        RatchetRumor::decode(&rumor.replace(r#""type":0"#, r#""type":2"#)),
        Err(Drop::Malformed)
    );
    assert_eq!(RatchetRumor::decode("nicht json"), Err(Drop::Malformed));
    let bad_body = RatchetRumor {
        v: VERSION,
        message_type: NORMAL,
        body: "%%%".into(),
    };
    assert_eq!(bad_body.message(), Err(Drop::Malformed));
}

#[test]
fn the_keep_alive_plaintext_is_kind_0_with_empty_content() {
    let encoded = Plaintext::keep_alive().encode().expect("encode");
    assert_eq!(encoded.as_str(), r#"{"kind":0,"content":""}"#);
    assert_eq!(
        Plaintext::decode(encoded.as_bytes()).expect("decode"),
        Plaintext::keep_alive()
    );
}

#[test]
fn a_plaintext_of_another_kind_is_refused_both_ways() {
    let other = Plaintext {
        kind: 1,
        content: "{}".into(),
    };
    assert!(other.encode().is_err());
    assert_eq!(
        Plaintext::decode(br#"{"kind":1,"content":""}"#),
        Err(Drop::UnknownKind)
    );
    assert_eq!(Plaintext::decode(b"{"), Err(Drop::Malformed));
    for kind in [24120, 24121] {
        let known = Plaintext {
            kind,
            content: "{}".into(),
        };
        let encoded = known.encode().expect("a chat kind");
        assert_eq!(Plaintext::decode(encoded.as_bytes()), Ok(known));
    }
}

#[test]
fn a_plaintext_over_the_limit_is_too_large() {
    let overhead = r#"{"kind":24120,"content":""}"#.len();
    let fits = Plaintext {
        kind: 24120,
        content: "a".repeat(MAX_PLAINTEXT_BYTES - overhead),
    };
    assert_eq!(fits.encode().expect("fits").len(), MAX_PLAINTEXT_BYTES);
    let over = Plaintext {
        kind: 24120,
        content: "a".repeat(MAX_PLAINTEXT_BYTES - overhead + 1),
    };
    let error = over.encode().expect_err("one byte over");
    assert_eq!(error.to_string(), envelope::too_large().to_string());

    let incoming = serde_json::to_vec(&Plaintext {
        kind: 24120,
        content: "a".repeat(MAX_PLAINTEXT_BYTES),
    })
    .expect("serialize incoming plaintext");
    assert!(incoming.len() > MAX_PLAINTEXT_BYTES);
    assert_eq!(Plaintext::decode(&incoming), Err(Drop::TooLarge));
}

/// Research "Checks done for tasks": the largest plaintext still wraps within the relay limit, also
/// as an opening message, which carries the keys on top.
#[test]
fn an_opening_message_with_the_largest_plaintext_fits_the_wrap_limit() {
    let plaintext = "a".repeat(MAX_PLAINTEXT_BYTES);
    let rumor = RatchetRumor::of(&opening(plaintext.as_bytes()))
        .encode()
        .expect("encode");
    let sender = DeviceKeys::generate();
    let recipient = DeviceKeys::generate();
    let event = wrap::wrap(
        RATCHET_KIND,
        &rumor,
        &sender.device_secret,
        &recipient.device_pubkey,
        now_ms(),
    )
    .expect("fits");
    assert!(event.as_json().len() <= MAX_WRAP_BYTES);
}
