use super::*;

#[test]
fn only_the_phone_kinds_go_to_a_phone() {
    for kind in [PHONE, EMAIL, ADDRESS, WEBSITE, IM, SIP, EVENT, RELATION] {
        assert!(goes_to_phone(kind), "{kind}");
    }
    for kind in [
        CUSTOM,
        HOLZI_VAULT,
        HOLZI_NOSTR_RELAY,
        HOLZI_IROH,
        "future_kind",
    ] {
        assert!(!goes_to_phone(kind), "{kind}");
    }
}

#[test]
fn unknown_kinds_are_not_known_and_never_validated_as_writable() {
    assert!(!is_known("future_kind"));
    assert!(validate("future_kind", "home", "x").is_err());
}

#[test]
fn labels_are_checked_per_kind() {
    assert!(validate(PHONE, "mobile", "0170 1234567").is_ok());
    assert!(validate(PHONE, "fax_work", "0170").is_ok());
    assert!(validate(EMAIL, "fax_work", "a@b.de").is_err());
    assert!(validate(EVENT, "birthday", r#"{"date":"1990-05-17"}"#).is_ok());
    assert!(validate(RELATION, "spouse", "Ben").is_ok());
    assert!(validate(PHONE, "custom:Ferienhaus", "0170").is_ok());
    assert!(validate(PHONE, "custom:", "0170").is_err());
    assert!(validate(PHONE, "nonsense", "0170").is_err());
}

#[test]
fn a_custom_value_needs_a_name() {
    assert!(validate(CUSTOM, "custom:Kundennummer", "4711").is_ok());
    assert!(validate(CUSTOM, "home", "4711").is_err());
}

#[test]
fn empty_and_too_long_values_are_refused() {
    assert!(validate(PHONE, "home", "  ").is_err());
    assert!(validate(CUSTOM, "custom:x", &"a".repeat(MAX_VALUE_CHARS + 1)).is_err());
    assert!(validate(CUSTOM, "custom:x", &"a".repeat(MAX_VALUE_CHARS)).is_ok());
}

#[test]
fn emails_need_an_at() {
    assert!(validate(EMAIL, "home", "anna@example.org").is_ok());
    assert!(validate(EMAIL, "home", "anna").is_err());
}

#[test]
fn addresses_are_objects_of_known_parts_with_some_text() {
    assert!(validate(
        ADDRESS,
        "home",
        r#"{"street":"Hauptstr. 1","city":"Berlin"}"#
    )
    .is_ok());
    assert!(validate(ADDRESS, "home", r#"{"street":"","city":" "}"#).is_err());
    assert!(validate(ADDRESS, "home", r#"{"planet":"Mars"}"#).is_err());
    assert!(validate(ADDRESS, "home", r#"{"city":3}"#).is_err());
    assert!(validate(ADDRESS, "home", "Hauptstr. 1").is_err());
}

#[test]
fn messengers_need_a_service_and_a_handle() {
    assert!(validate(IM, "other", r#"{"service":"Signal","handle":"anna.01"}"#).is_ok());
    assert!(validate(IM, "other", r#"{"service":"Signal"}"#).is_err());
}

#[test]
fn events_take_dates_with_and_without_a_year() {
    assert!(validate(EVENT, "birthday", r#"{"date":"1990-05-17"}"#).is_ok());
    assert!(validate(EVENT, "birthday", r#"{"date":"--05-17"}"#).is_ok());
    assert!(validate(EVENT, "birthday", r#"{"date":"1990-13-01"}"#).is_err());
    assert!(validate(EVENT, "birthday", r#"{"date":"17.05.1990"}"#).is_err());
    assert!(validate(EVENT, "birthday", r#"{"date":"90-05-17"}"#).is_err());
}

#[test]
fn a_vault_identity_must_be_an_npub() {
    let keys = nostr::key::Keys::generate();
    let npub = {
        use nostr::nips::nip19::ToBech32;
        keys.public_key().to_bech32().expect("bech32")
    };
    assert!(validate(HOLZI_VAULT, "other", &npub).is_ok());
    assert!(validate(HOLZI_VAULT, "other", &keys.public_key().to_hex()).is_err());
    assert!(validate(HOLZI_VAULT, "other", "npub1nonsense").is_err());
}

#[test]
fn nostr_relays_are_websocket_addresses() {
    assert!(validate(HOLZI_NOSTR_RELAY, "other", "wss://relay.example.org").is_ok());
    assert!(validate(HOLZI_NOSTR_RELAY, "other", "https://relay.example.org").is_err());
}

#[test]
fn iroh_addresses_are_an_endpoint_id_with_an_optional_server() {
    let id = "ab".repeat(32);
    assert!(validate(HOLZI_IROH, "other", &id).is_ok());
    assert!(validate(
        HOLZI_IROH,
        "other",
        &format!("{id}@https://relay.example.org")
    )
    .is_ok());
    assert!(validate(
        HOLZI_IROH,
        "other",
        &format!("{id}@wss://relay.example.org")
    )
    .is_err());
    assert!(validate(HOLZI_IROH, "other", &id[..63]).is_err());
    assert!(validate(HOLZI_IROH, "other", &"zz".repeat(32)).is_err());
}
