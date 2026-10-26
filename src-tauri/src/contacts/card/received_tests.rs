//! The stored cards of other vaults (spec 060 research R5, FR-025a): the hash is the key, several
//! cards belong to one vault, the addresses are their union, the newest card sets the threshold, and
//! a row that does not fit its content is never used.

use haex_crdt::rusqlite::params;

use super::format::{self, CardDevice, CardValue, ContactCard, SignedCard, VERSION};
use super::*;
use crate::contacts::model::{ContactInput, Head};
use crate::contacts::service::test_support::{card_vault, TestVault};
use crate::contacts::service::Caller;
use crate::sync::signing;

const SECRET: [u8; 32] = [9; 32];
const OTHER_SECRET: [u8; 32] = [10; 32];

fn vault_of(secret: &[u8; 32]) -> [u8; 32] {
    signing::xonly_public_key(secret).expect("a key")
}

fn device(n: u8) -> CardDevice {
    CardDevice {
        device_pubkey: [n; 32],
        endpoint_id: [n + 100; 32],
        iroh_relay: Some(format!("https://relay-{n}.example.org/")),
    }
}

/// A card of the vault of `secret`.
fn card_of(
    secret: &[u8; 32],
    generation: u64,
    at: u64,
    devices: &[u8],
    relays: &[&str],
) -> SignedCard {
    let card = ContactCard {
        version: VERSION,
        vault: vault_of(secret),
        issued_at: at,
        list_generation: generation,
        devices: devices.iter().map(|n| device(*n)).collect(),
        nostr_relays: relays.iter().map(|url| (*url).to_owned()).collect(),
        name: "Ben".to_owned(),
        values: vec![CardValue {
            kind: "email".to_owned(),
            label: "work".to_owned(),
            value: "ben@example.org".to_owned(),
        }],
    };
    format::sign(card, secret).expect("sign").0
}

async fn contact(vault: &TestVault) -> String {
    vault
        .service
        .create(
            &Caller::User,
            &[],
            ContactInput {
                head: Head {
                    given_name: Some("Ben".to_owned()),
                    ..Head::default()
                },
                ..ContactInput::default()
            },
        )
        .await
        .expect("create")
        .id
}

fn save_all(vault: &TestVault, contact_id: &str, cards: &[&SignedCard]) -> Vec<bool> {
    let (contact_id, cards): (String, Vec<SignedCard>) = (
        contact_id.to_owned(),
        cards.iter().map(|c| (*c).clone()).collect(),
    );
    vault
        .db
        .write_blocking(move |tx| {
            cards
                .iter()
                .map(|card| save(tx, &contact_id, card).map_err(Into::into))
                .collect::<haex_crdt::Result<Vec<bool>>>()
        })
        .expect("save")
}

fn rows(vault: &TestVault) -> i64 {
    vault
        .db
        .read_blocking(|q| {
            Ok(
                q.query_row("SELECT COUNT(*) FROM haex_contacts_cards", &[], |r| {
                    r.get::<_, i64>(0)
                })?
                .unwrap_or(0),
            )
        })
        .expect("count")
}

fn newest(vault: &TestVault, secret: &[u8; 32]) -> Option<SignedCard> {
    let key = vault_of(secret);
    vault
        .db
        .read_blocking(move |q| Ok(stored(q, &key)?.map(|(_, card)| card)))
        .expect("stored")
}

fn addresses_of_vault(vault: &TestVault, secret: &[u8; 32]) -> Addresses {
    let key = vault_of(secret);
    vault
        .db
        .read_blocking(move |q| Ok(addresses(q, &key)?))
        .expect("addresses")
}

#[tokio::test]
async fn the_same_card_is_one_row_and_a_second_card_of_the_vault_is_another() {
    let mara = card_vault();
    let ben = contact(&mara).await;
    let one = card_of(&SECRET, 1, 1_000, &[1], &["wss://a.example.org"]);
    let two = card_of(&SECRET, 2, 2_000, &[1, 2], &["wss://a.example.org"]);
    assert_eq!(
        save_all(&mara, &ben, &[&one, &one, &two]),
        [true, false, true]
    );
    assert_eq!(
        rows(&mara),
        2,
        "the same card twice is one row, a new card is a new row"
    );
    assert_ne!(one.hash(), two.hash());
}

#[tokio::test]
async fn the_newest_card_is_the_one_with_the_highest_generation_then_the_later_time() {
    let mara = card_vault();
    let ben = contact(&mara).await;
    let old = card_of(&SECRET, 1, 9_000, &[1], &[]);
    let later_time = card_of(&SECRET, 2, 1_000, &[1, 2], &[]);
    let later_still = card_of(&SECRET, 2, 2_000, &[1, 2, 3], &[]);
    save_all(&mara, &ben, &[&old, &later_still, &later_time]);
    let found = newest(&mara, &SECRET).expect("a card");
    assert_eq!(found.hash(), later_still.hash());
    let of_contact = mara
        .db
        .read_blocking({
            let ben = ben.clone();
            move |q| Ok(stored_of_contact(q, &ben)?)
        })
        .expect("of contact")
        .expect("a card");
    assert_eq!(of_contact.hash(), later_still.hash());
}

#[tokio::test]
async fn the_addresses_of_a_vault_are_the_union_of_its_cards_each_once() {
    let mara = card_vault();
    let ben = contact(&mara).await;
    let old = card_of(
        &SECRET,
        1,
        1_000,
        &[1, 2],
        &["wss://old.example.org", "wss://both.example.org"],
    );
    let new = card_of(
        &SECRET,
        2,
        2_000,
        &[2, 3],
        &["wss://new.example.org", "wss://both.example.org"],
    );
    save_all(&mara, &ben, &[&old, &new]);
    let found = addresses_of_vault(&mara, &SECRET);
    let endpoints: Vec<[u8; 32]> = found.devices.iter().map(|d| d.endpoint_id).collect();
    assert_eq!(
        endpoints.len(),
        3,
        "device 2 is on both cards and appears once"
    );
    assert!(
        endpoints.contains(&[101; 32])
            && endpoints.contains(&[102; 32])
            && endpoints.contains(&[103; 32])
    );
    assert_eq!(
        found.nostr_relays.len(),
        3,
        "old, new and the one both cards name"
    );
    // The newest card's devices come first, so the card of the window is the newest card's.
    assert_eq!(found.devices[0].endpoint_id, [102; 32]);
    let shown = mara
        .db
        .read_blocking(move |q| Ok(of(q, &ben)?))
        .expect("of")
        .expect("a card");
    assert_eq!(shown.devices.len(), 3);
    assert_eq!(shown.nostr_relays.len(), 3);
}

#[tokio::test]
async fn a_row_that_does_not_fit_its_content_is_ignored_by_every_reader() {
    let mara = card_vault();
    let ben = contact(&mara).await;
    let good = card_of(&SECRET, 1, 1_000, &[1], &[]);
    let other = card_of(&OTHER_SECRET, 1, 1_000, &[5], &[]);
    save_all(&mara, &ben, &[&good]);
    // A card of another vault filed under the vault of `good`, and one under a made-up hash.
    let (vault_of_good, other_bytes, other_hash) =
        (hex(&good.card.vault), other.bytes.clone(), other.hash());
    let ben_id = ben.clone();
    mara.db
        .write_blocking(move |tx| {
            tx.execute(
                "INSERT INTO haex_contacts_cards (hash, vault_pubkey, contact_id, card, \
                 issued_at, received_at) VALUES (?1, ?2, ?3, ?4, 't', 't')",
                params![other_hash, vault_of_good, ben_id, other_bytes],
            )?;
            tx.execute(
                "INSERT INTO haex_contacts_cards (hash, vault_pubkey, contact_id, card, \
                 issued_at, received_at) VALUES ('not-the-hash', ?1, ?2, ?3, 't', 't')",
                params![hex(&other.card.vault), ben_id, other_bytes],
            )?;
            Ok(())
        })
        .expect("insert wrong rows");
    assert_eq!(rows(&mara), 3);
    // Nothing of the other vault shows under the first vault, and the other vault has no card.
    let found = addresses_of_vault(&mara, &SECRET);
    assert_eq!(found.devices.len(), 1, "only the good card counts");
    assert_eq!(found.devices[0].endpoint_id, [101; 32]);
    assert!(newest(&mara, &OTHER_SECRET).is_none());
    let card_contacts = mara
        .db
        .read_blocking(|q| Ok(with_cards(q)?))
        .expect("card contacts");
    assert_eq!(card_contacts, std::collections::BTreeSet::from([ben]));
}

#[tokio::test]
async fn a_row_with_a_broken_signature_is_ignored() {
    let mara = card_vault();
    let ben = contact(&mara).await;
    let good = card_of(&SECRET, 1, 1_000, &[1], &[]);
    let mut broken = good.clone();
    let last = broken.bytes.len() - 1;
    broken.bytes[last] ^= 1;
    let (hash, vault, bytes) = (good.hash(), hex(&good.card.vault), broken.bytes.clone());
    mara.db
        .write_blocking(move |tx| {
            tx.execute(
                "INSERT INTO haex_contacts_cards (hash, vault_pubkey, contact_id, card, \
                 issued_at, received_at) VALUES (?1, ?2, ?3, ?4, 't', 't')",
                params![hash, vault, ben, bytes],
            )?;
            Ok(())
        })
        .expect("insert");
    assert!(
        newest(&mara, &SECRET).is_none(),
        "a broken signature is no card"
    );
}

#[tokio::test]
async fn delete_for_vault_removes_only_that_vaults_cards_and_replace_keeps_one() {
    let mara = card_vault();
    let ben = contact(&mara).await;
    let one = card_of(&SECRET, 1, 1_000, &[1], &[]);
    let two = card_of(&SECRET, 2, 2_000, &[2], &[]);
    let other = card_of(&OTHER_SECRET, 1, 1_000, &[5], &[]);
    save_all(&mara, &ben, &[&one, &two, &other]);
    assert_eq!(rows(&mara), 3);
    let (replacement, ben_id) = (card_of(&SECRET, 3, 3_000, &[3], &[]), ben.clone());
    mara.db
        .write_blocking(move |tx| Ok(replace(tx, &ben_id, &replacement)?))
        .expect("replace");
    assert_eq!(
        rows(&mara),
        2,
        "the vault's two cards became one, the other vault stays"
    );
    assert_eq!(addresses_of_vault(&mara, &SECRET).devices.len(), 1);
    let key = vault_of(&SECRET);
    mara.db
        .write_blocking(move |tx| Ok(delete_for_vault(tx, &key)?))
        .expect("delete");
    assert_eq!(rows(&mara), 1);
    assert!(newest(&mara, &SECRET).is_none());
    assert!(newest(&mara, &OTHER_SECRET).is_some());
}

#[tokio::test]
async fn deleting_the_contact_deletes_its_cards() {
    let mara = card_vault();
    let ben = contact(&mara).await;
    save_all(&mara, &ben, &[&card_of(&SECRET, 1, 1_000, &[1], &[])]);
    mara.service
        .delete(&Caller::User, &[], vec![ben])
        .await
        .expect("delete");
    assert_eq!(rows(&mara), 0);
}
