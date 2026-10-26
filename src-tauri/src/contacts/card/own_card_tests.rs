use super::*;
use crate::contacts::card::format::parse;
use crate::contacts::service::test_support::card_vault;
use crate::sync::test_support::Member;

/// The stored own card of a device's database, as text.
fn text_of(db: &haex_crdt::Database) -> Option<String> {
    crate::storage::query::read(db, |r| stored_text(r).map_err(haex_crdt::Error::from))
        .expect("read")
}

fn read_text(db: &crate::vault_gate::VaultDb) -> Option<String> {
    db.read_blocking(|q| stored_text(q).map_err(haex_crdt::Error::from))
        .expect("read")
}

fn decoded(text: Option<String>) -> ContactCard {
    parse(text.as_deref().expect("a card")).expect("valid").card
}

fn device_keys(card: &ContactCard) -> Vec<[u8; 32]> {
    card.devices
        .iter()
        .map(|device| device.device_pubkey)
        .collect()
}

#[test]
fn a_main_device_signs_the_card_when_the_vault_is_made() {
    let vault = card_vault();
    let content = decoded(read_text(&vault.db));
    let pubkey = vault
        .db
        .read_blocking(|q| keys::vault_pubkey(q))
        .expect("read")
        .expect("an identity");
    assert_eq!(content.vault, pubkey);
    assert_eq!(content.devices.len(), 1);
    assert_eq!(content.list_generation, 1);
    assert!(!content.nostr_relays.is_empty());
}

#[test]
fn a_linked_device_holds_the_synced_card_and_cannot_sign() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    let text = text_of(linked.device.db());
    assert_eq!(decoded(text.clone()).vault, main.vault);
    linked
        .device
        .db()
        .write(|tx| refresh(tx).map_err(haex_crdt::Error::from))
        .expect("refresh does nothing");
    assert_eq!(text_of(linked.device.db()), text);

    let built = crate::storage::query::read(linked.device.db(), |r| {
        build_for_share(r, &[], &[]).map_err(haex_crdt::Error::from)
    })
    .expect("build");
    assert!(built.is_none(), "no vault secret, no card");
}

#[test]
fn a_new_device_list_renews_the_card() {
    let main = Member::genesis();
    let other = Member::join(&main);
    let before = decoded(text_of(main.device.db()));
    main.add(&other);
    main.device
        .db()
        .write(|tx| refresh(tx).map_err(haex_crdt::Error::from))
        .expect("refresh");
    let after = decoded(text_of(main.device.db()));
    assert_eq!(after.devices.len(), 2);
    assert_eq!(after.list_generation, before.list_generation + 1);
    assert!(
        after.issued_at > before.issued_at,
        "a newer card replaces the older"
    );
}

#[test]
fn changed_relays_renew_the_card() {
    let vault = card_vault();
    let before = decoded(read_text(&vault.db));
    vault
        .db
        .write_blocking(|tx| {
            let mut config = servers::read(tx)?;
            config
                .nostr_relays
                .push("wss://relay.example.org".to_owned());
            servers::write(tx, &config)
        })
        .expect("relays");
    let after = decoded(read_text(&vault.db));
    assert!(after
        .nostr_relays
        .contains(&"wss://relay.example.org".to_owned()));
    assert!(after.issued_at > before.issued_at);
}

#[test]
fn a_card_without_a_filter_is_what_refresh_stores() {
    let vault = card_vault();
    let stored = decoded(read_text(&vault.db));
    let built = vault
        .db
        .read_blocking(|q| build_card(q, None, None).map_err(haex_crdt::Error::from))
        .expect("build")
        .expect("a card");
    assert_eq!(built.vault, stored.vault);
    assert_eq!(built.list_generation, stored.list_generation);
    assert_eq!(built.devices, stored.devices);
    assert_eq!(built.nostr_relays, stored.nostr_relays);
    assert_eq!(built.name, stored.name);
    assert!(built.values.is_empty());
}

#[test]
fn a_card_for_a_share_carries_only_the_chosen_devices_and_relays_and_no_values() {
    let main = Member::genesis();
    let other = Member::join(&main);
    main.add(&other);
    main.device
        .db()
        .write(|tx| {
            let mut config = servers::read(tx)?;
            config.nostr_relays = vec![
                "wss://one.example.org".to_owned(),
                "wss://two.example.org".to_owned(),
            ];
            servers::write(tx, &config)
        })
        .expect("relays");
    let stored = text_of(main.device.db());
    let all = decoded(stored.clone());
    assert_eq!(all.devices.len(), 2);
    assert!(all.nostr_relays.len() >= 2);
    let (one, relay) = (all.devices[1].device_pubkey, all.nostr_relays[0].clone());

    let signed = crate::storage::query::read(main.device.db(), |r| {
        build_for_share(r, &[one], &[]).map_err(haex_crdt::Error::from)
    })
    .expect("build")
    .expect("a main device signs");
    let card = format::from_bytes(signed.bytes).expect("a valid card").card;
    assert_eq!(device_keys(&card), [one]);
    assert!(card.nostr_relays.is_empty());
    assert!(card.values.is_empty());
    assert_eq!(card.vault, all.vault, "the identity always goes along");

    let signed = crate::storage::query::read(main.device.db(), |r| {
        build_for_share(r, &[], std::slice::from_ref(&relay)).map_err(haex_crdt::Error::from)
    })
    .expect("build")
    .expect("a main device signs");
    assert_eq!(signed.card.nostr_relays, [relay]);
    assert!(signed.card.devices.is_empty());

    assert_eq!(text_of(main.device.db()), stored, "a share stores nothing");
}

#[test]
fn a_card_for_a_share_is_never_older_than_the_stored_one() {
    let vault = card_vault();
    let stored = decoded(read_text(&vault.db));
    let signed = vault
        .db
        .read_blocking(|q| build_for_share(q, &[], &[]).map_err(haex_crdt::Error::from))
        .expect("build")
        .expect("a card");
    assert!(signed.card.issued_at > stored.issued_at);
}

#[test]
fn a_device_or_relay_the_vault_does_not_have_is_refused() {
    let vault = card_vault();
    let stranger = vault
        .db
        .read_blocking(|q| build_for_share(q, &[[9; 32]], &[]).map_err(haex_crdt::Error::from));
    assert!(stranger.is_err());
    let relay = vault.db.read_blocking(|q| {
        build_for_share(q, &[], &["wss://nope.example.org".to_owned()])
            .map_err(haex_crdt::Error::from)
    });
    assert!(relay.is_err());
}
