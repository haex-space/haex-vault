use super::*;
use crate::contacts::access::Caller;
use crate::contacts::card::format::parse;
use crate::contacts::service::test_support::{card_vault, person, value};
use crate::sync::test_support::Member;

/// The own card of a device's database.
fn card_of(db: &haex_crdt::Database) -> OwnCard {
    crate::storage::query::read(db, |r| read(r).map_err(haex_crdt::Error::from)).expect("read")
}

fn read_card(db: &crate::vault_gate::VaultDb) -> OwnCard {
    db.read_blocking(|q| read(q).map_err(haex_crdt::Error::from))
        .expect("read")
}

fn decoded(card: &OwnCard) -> ContactCard {
    parse(card.text.as_deref().expect("a card"))
        .expect("valid")
        .card
}

#[test]
fn a_main_device_signs_the_card_when_the_vault_is_made() {
    let vault = card_vault();
    let card = read_card(&vault.db);
    assert!(card.can_sign);
    let content = decoded(&card);
    let pubkey = vault
        .db
        .read_blocking(|q| keys::vault_pubkey(q))
        .expect("read")
        .expect("an identity");
    assert_eq!(content.vault, pubkey);
    assert_eq!(content.devices.len(), 1);
    assert_eq!(content.list_generation, 1);
    assert!(!content.nostr_relays.is_empty());
    assert!(card.qr_svg.is_some_and(|svg| svg.contains("<svg")));
}

#[test]
fn a_linked_device_shows_the_synced_card_and_cannot_sign() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    let card = card_of(linked.device.db());
    assert!(!card.can_sign);
    assert_eq!(decoded(&card).vault, main.vault);
    linked
        .device
        .db()
        .write(|tx| refresh(tx).map_err(haex_crdt::Error::from))
        .expect("refresh does nothing");
    let again = card_of(linked.device.db());
    assert_eq!(again.text, card.text);
}

#[test]
fn a_new_device_list_renews_the_card() {
    let main = Member::genesis();
    let other = Member::join(&main);
    let before = card_of(main.device.db());
    main.add(&other);
    main.device
        .db()
        .write(|tx| refresh(tx).map_err(haex_crdt::Error::from))
        .expect("refresh");
    let after = card_of(main.device.db());
    let (old, new) = (decoded(&before), decoded(&after));
    assert_eq!(new.devices.len(), 2);
    assert_eq!(new.list_generation, old.list_generation + 1);
    assert!(
        new.issued_at > old.issued_at,
        "a newer card replaces the older"
    );
}

#[test]
fn changed_relays_renew_the_card() {
    let vault = card_vault();
    let before = decoded(&read_card(&vault.db));
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
    let after = decoded(&read_card(&vault.db));
    assert!(after
        .nostr_relays
        .contains(&"wss://relay.example.org".to_owned()));
    assert!(after.issued_at > before.issued_at);
}

#[tokio::test]
async fn the_card_carries_the_name_and_the_chosen_values() {
    let vault = card_vault();
    let ben = vault
        .service
        .create(
            &Caller::User,
            &[],
            person(
                "Ben",
                "Krüger",
                vec![
                    value("email", "work", "ben@example.org"),
                    value("phone", "mobile", "0170 1111"),
                ],
            ),
        )
        .await
        .expect("create");
    let email = ben
        .values
        .iter()
        .find(|v| v.kind == "email")
        .expect("email")
        .id
        .clone();
    let card = vault
        .service
        .own_card_set(&Caller::User, Some(ben.id.clone()), vec![email])
        .await
        .expect("set");
    assert_eq!(card.own_contact.as_deref(), Some(ben.id.as_str()));
    let content = decoded(&card);
    assert_eq!(content.name, "Ben Krüger");
    assert_eq!(
        content
            .values
            .iter()
            .map(|v| v.value.as_str())
            .collect::<Vec<_>>(),
        ["ben@example.org"],
        "only the chosen values; the phone number stays off the card"
    );
}
