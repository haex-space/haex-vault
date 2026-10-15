use super::*;

const SECRET: [u8; 32] = [7; 32];

fn card(values: usize) -> ContactCard {
    ContactCard {
        version: VERSION,
        vault: signing::xonly_public_key(&SECRET).expect("a key"),
        issued_at: 1_760_000_000,
        list_generation: 3,
        devices: vec![CardDevice {
            device_pubkey: [1; 32],
            endpoint_id: [2; 32],
            iroh_relay: Some("https://relay.example.org/".to_owned()),
        }],
        nostr_relays: vec!["wss://relay.example.org".to_owned()],
        name: "Ben Krüger".to_owned(),
        values: (0..values)
            .map(|n| CardValue {
                kind: "email".to_owned(),
                label: "work".to_owned(),
                value: format!("ben{n}@example.org"),
            })
            .collect(),
    }
}

fn signed(card: ContactCard) -> SignedCard {
    sign(card, &SECRET).expect("sign").0
}

fn is_invalid(result: Result<SignedCard, HolziError>) -> bool {
    matches!(result, Err(HolziError::CardInvalid { .. }))
}

#[test]
fn a_card_goes_to_text_and_back() {
    let original = signed(card(2));
    let text = original.text();
    assert!(text.starts_with("holzi:card:"));
    let read = parse(&format!("  {text}\n")).expect("parse");
    assert_eq!(read, original);
    assert_eq!(from_bytes(read.bytes.clone()).expect("bytes"), original);
    assert!(qr_svg(&text).expect("qr").contains("<svg"));
}

#[test]
fn a_text_that_is_no_card_is_refused() {
    let text = signed(card(0)).text();
    let encoded = text.strip_prefix(PREFIX).expect("prefix");
    assert!(is_invalid(parse(&format!("holzi:link:{encoded}"))));
    assert!(is_invalid(parse("holzi:card:!!!")));
    let mut bytes = URL_SAFE_NO_PAD.decode(encoded).expect("base64");
    bytes.push(0);
    assert!(is_invalid(parse(&format!(
        "{PREFIX}{}",
        URL_SAFE_NO_PAD.encode(&bytes)
    ))));
    let mut other = card(0);
    other.version = 2;
    let payload = signing::canonical(&other).expect("encode");
    let signature = signing::sign(Domain::ContactCard, &payload, &SECRET).expect("sign");
    let mut forged = (payload.len() as u16).to_be_bytes().to_vec();
    forged.extend_from_slice(&payload);
    forged.extend_from_slice(&signature);
    assert!(is_invalid(from_bytes(forged)), "an unknown version");
}

#[test]
fn one_flipped_bit_anywhere_is_refused() {
    let bytes = signed(card(1)).bytes;
    for index in [2, 10, bytes.len() / 2, bytes.len() - 1] {
        let mut changed = bytes.clone();
        changed[index] ^= 1;
        assert!(is_invalid(from_bytes(changed)), "bit at {index}");
    }
}

#[test]
fn the_limits_hold_on_writing() {
    let mut relays = card(0);
    relays.nostr_relays = (0..11).map(|n| format!("wss://r{n}.example.org")).collect();
    assert!(sign(relays, &SECRET).is_err());
    let mut devices = card(0);
    devices.devices = (0..33)
        .map(|n| CardDevice {
            device_pubkey: [n; 32],
            endpoint_id: [n; 32],
            iroh_relay: None,
        })
        .collect();
    assert!(sign(devices, &SECRET).is_err());
    let mut long = card(0);
    long.name = "x".repeat(1025);
    assert!(sign(long, &SECRET).is_err());
    let mut http = card(0);
    http.nostr_relays = vec!["https://relay.example.org".to_owned()];
    assert!(sign(http, &SECRET).is_err());
}

#[test]
fn values_that_do_not_fit_are_left_out_from_the_end() {
    let (card, omitted) = sign(card(60), &SECRET).expect("sign");
    assert!(card.text().len() <= MAX_TEXT);
    assert!(!omitted.is_empty());
    assert_eq!(card.card.values.len() + omitted.len(), 60);
    assert_eq!(omitted.last().expect("one").value, "ben59@example.org");
    assert_eq!(
        omitted.first().expect("one").value,
        format!("ben{}@example.org", card.card.values.len())
    );
}

#[test]
fn a_card_too_large_without_values_is_not_made() {
    let mut big = card(0);
    big.devices = (0..32)
        .map(|n| CardDevice {
            device_pubkey: [n; 32],
            endpoint_id: [n; 32],
            iroh_relay: Some(format!("https://relay-{n}.example.org/")),
        })
        .collect();
    assert!(matches!(sign(big, &SECRET), Err(HolziError::CardTooLarge)));
}
