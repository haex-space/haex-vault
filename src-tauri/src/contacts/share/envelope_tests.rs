use super::*;
use crate::contacts::card::format::{CardDevice, CardValue, ContactCard};
use crate::sync::signing;

const THEIR_SECRET: [u8; 32] = [7; 32];
const OWN_SECRET: [u8; 32] = [9; 32];

fn vault_of(secret: &[u8; 32]) -> [u8; 32] {
    signing::xonly_public_key(secret).expect("a key")
}

fn own() -> [u8; 32] {
    vault_of(&OWN_SECRET)
}

fn card_of(secret: &[u8; 32], devices: u8) -> SignedCard {
    let card = ContactCard {
        version: format::VERSION,
        vault: vault_of(secret),
        issued_at: 1_760_000_000,
        list_generation: 3,
        devices: (0..devices)
            .map(|n| CardDevice {
                device_pubkey: [n; 32],
                endpoint_id: [n.wrapping_add(100); 32],
                iroh_relay: Some("https://relay.example.org/".to_owned()),
            })
            .collect(),
        nostr_relays: vec!["wss://relay.example.org".to_owned()],
        name: "Ben Krüger".to_owned(),
        values: vec![CardValue {
            kind: "email".to_owned(),
            label: "work".to_owned(),
            value: "ben@example.org".to_owned(),
        }],
    };
    format::sign_with(card, secret, None).expect("sign").0
}

fn value(kind: &str, label: &str, text: &str) -> ShareValue {
    ShareValue {
        kind: kind.to_owned(),
        label: label.to_owned(),
        value: text.to_owned(),
    }
}

fn envelope() -> ShareEnvelope {
    let mut envelope = ShareEnvelope::new("Ben Krüger".to_owned());
    envelope.name_parts.given = Some("Ben".to_owned());
    envelope.name_parts.family = Some("Krüger".to_owned());
    envelope.head.nickname = Some("Benny".to_owned());
    envelope.head.note = Some("Kennengelernt auf der Messe".to_owned());
    envelope.values = vec![
        value("email", "work", "ben@example.org"),
        value("phone", "mobile", "+49 170 1234567"),
    ];
    envelope.card = Some(card_of(&THEIR_SECRET, 2).bytes);
    envelope
}

fn is_share_invalid<T: std::fmt::Debug>(result: Result<T>) -> bool {
    matches!(result, Err(HolziError::ShareInvalid { .. }))
}

fn text_of(envelope: &ShareEnvelope) -> String {
    let bytes = postcard::to_stdvec(envelope).expect("encodes");
    format!("{PREFIX_LINK}{}", URL_SAFE_NO_PAD.encode(bytes))
}

#[test]
fn an_envelope_goes_to_text_and_back() {
    let sent = envelope();
    let text = encode(&sent).expect("encodes");
    assert!(text.starts_with("haex-vault://contact/"));
    let read = decode(&text, &own()).expect("decodes");
    assert_eq!(read.envelope, sent);
    assert_eq!(read.skipped, 0);
    assert!(!read.legacy_card);
    let card = read.card.expect("the card is read");
    assert_eq!(card.card.vault, vault_of(&THEIR_SECRET));
    assert_eq!(card.bytes, sent.card.expect("a card"));
}

#[test]
fn surrounding_blanks_do_not_matter() {
    let text = encode(&envelope()).expect("encodes");
    assert!(decode(&format!("  {text}\n"), &own()).is_ok());
}

#[test]
fn an_envelope_without_a_card_is_normal() {
    let mut sent = envelope();
    sent.card = None;
    let read = decode(&encode(&sent).expect("encodes"), &own()).expect("decodes");
    assert!(read.card.is_none());
}

#[test]
fn a_card_of_thirty_two_devices_fits_the_envelope() {
    let mut sent = envelope();
    sent.card = Some(card_of(&THEIR_SECRET, 32).bytes);
    let read = decode(&encode(&sent).expect("encodes"), &own()).expect("decodes");
    assert_eq!(read.card.expect("a card").card.devices.len(), 32);
}

#[test]
fn the_limits_hold_on_writing() {
    let mut many = envelope();
    many.values = (0..=MAX_VALUES)
        .map(|n| value("email", "work", &format!("ben{n}@example.org")))
        .collect();
    assert!(is_share_invalid(encode(&many)));

    let mut long_value = envelope();
    long_value.values = vec![value("custom", "custom:x", &"a".repeat(4_097))];
    assert!(is_share_invalid(encode(&long_value)));

    let mut long_note = envelope();
    long_note.head.note = Some("a".repeat(4_097));
    assert!(is_share_invalid(encode(&long_note)));

    let mut long_name = envelope();
    long_name.name = "a".repeat(1_025);
    assert!(is_share_invalid(encode(&long_name)));

    let mut version = envelope();
    version.version = 2;
    assert!(is_share_invalid(encode(&version)));
}

#[test]
fn a_text_over_the_limit_is_too_large_and_never_cut() {
    let mut big = envelope();
    big.card = None;
    big.values = (0..MAX_VALUES)
        .map(|_| value("custom", "custom:x", &"ä".repeat(4_000)))
        .collect();
    assert!(matches!(encode(&big), Err(HolziError::ShareTooLarge)));
}

#[test]
fn the_limits_hold_on_reading() {
    let mut many = envelope();
    many.values = (0..=MAX_VALUES)
        .map(|n| value("email", "work", &format!("ben{n}@example.org")))
        .collect();
    assert!(is_share_invalid(decode(&text_of(&many), &own())));

    let mut long_value = envelope();
    long_value.values = vec![value("custom", "custom:x", &"a".repeat(4_097))];
    assert!(is_share_invalid(decode(&text_of(&long_value), &own())));

    let over = format!("{PREFIX_LINK}{}", "A".repeat(MAX_SHARE_TEXT));
    assert!(is_share_invalid(decode(&over, &own())));
}

#[test]
fn a_text_of_another_form_is_refused() {
    let text = encode(&envelope()).expect("encodes");
    let payload = text.strip_prefix(PREFIX_LINK).expect("a prefix");

    assert!(is_share_invalid(decode("hello", &own())));
    assert!(is_share_invalid(decode(
        &format!("haex-vault://other/{payload}"),
        &own()
    )));
    assert!(is_share_invalid(decode(
        &format!("{PREFIX_LINK}not base64url!"),
        &own()
    )));
    let mut bytes = URL_SAFE_NO_PAD.decode(payload).expect("base64url");
    bytes.push(0);
    assert!(is_share_invalid(decode(
        &format!("{PREFIX_LINK}{}", URL_SAFE_NO_PAD.encode(bytes)),
        &own()
    )));
    let mut next = envelope();
    next.version = 2;
    assert!(is_share_invalid(decode(&text_of(&next), &own())));
}

#[test]
fn a_flipped_bit_in_the_card_refuses_the_whole_envelope() {
    let mut sent = envelope();
    let mut card = sent.card.take().expect("a card");
    let middle = card.len() / 2;
    card[middle] ^= 1;
    sent.card = Some(card);
    assert!(matches!(
        decode(&text_of(&sent), &own()),
        Err(HolziError::CardInvalid { .. })
    ));
}

#[test]
fn a_card_of_the_own_vault_is_refused() {
    let mut sent = envelope();
    sent.card = Some(card_of(&OWN_SECRET, 1).bytes);
    assert!(matches!(
        decode(&text_of(&sent), &own()),
        Err(HolziError::CardOwn)
    ));
}

#[test]
fn an_unknown_kind_survives_unchanged() {
    let mut sent = envelope();
    sent.values = vec![value("telepathy", "weird", "think hard")];
    let read = decode(&text_of(&sent), &own()).expect("decodes");
    assert_eq!(read.envelope.values, sent.values);
    assert_eq!(read.skipped, 0);
}

#[test]
fn a_detail_that_does_not_validate_is_skipped_and_counted() {
    let mut sent = envelope();
    sent.values = vec![
        value("email", "work", "not an address"),
        value("email", "work", "ben@example.org"),
        value("phone", "mobile", "   "),
    ];
    let read = decode(&text_of(&sent), &own()).expect("decodes");
    assert_eq!(
        read.envelope.values,
        vec![value("email", "work", "ben@example.org")]
    );
    assert_eq!(read.skipped, 2);
}

#[test]
fn a_holzi_address_as_a_detail_stays_an_unconfirmed_value_and_names_no_vault() {
    let mut sent = envelope();
    sent.card = None;
    let npub = crate::contacts::card::received::npub(&vault_of(&THEIR_SECRET));
    sent.values = vec![
        value("holzi_vault", "other", &npub),
        value("holzi_vault", "other", "nope"),
    ];
    let read = decode(&text_of(&sent), &own()).expect("decodes");
    assert_eq!(
        read.envelope.values,
        vec![value("holzi_vault", "other", &npub)]
    );
    assert_eq!(read.skipped, 1, "an invalid one is left out like any other");
    assert!(read.card.is_none(), "only a signed card confirms a vault");
}

#[test]
fn a_holzi_card_text_reads_as_a_legacy_envelope() {
    let card = card_of(&THEIR_SECRET, 2);
    let read = decode(&card.text(), &own()).expect("decodes");
    assert!(read.legacy_card);
    assert_eq!(read.envelope.name, "Ben Krüger");
    assert_eq!(
        read.envelope.values,
        vec![value("email", "work", "ben@example.org")]
    );
    assert_eq!(read.envelope.card.as_deref(), Some(card.bytes.as_slice()));
    assert_eq!(read.card.expect("a card"), card);
}

#[test]
fn a_holzi_card_text_of_the_own_vault_is_refused() {
    let card = card_of(&OWN_SECRET, 1);
    assert!(matches!(
        decode(&card.text(), &own()),
        Err(HolziError::CardOwn)
    ));
}

#[test]
fn a_short_text_fits_a_qr_code_and_a_long_one_does_not() {
    assert!(fits_qr(&"a".repeat(QR_MAX)));
    assert!(!fits_qr(&"a".repeat(QR_MAX + 1)));
}
