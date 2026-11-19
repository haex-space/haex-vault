use std::collections::BTreeMap;

use haex_crdt::rusqlite;
use uuid::Uuid;

use super::*;
use crate::storage::query;
use crate::sync::device_list::{sign_list, DeviceList, ListedDevice, Role};
use crate::sync::keys::random_secret_key;
use crate::sync::signing::xonly_public_key;
use crate::sync::test_support::open_vault;

fn listed(keys: &DeviceKeys, role: Role, tag: u8) -> ListedDevice {
    ListedDevice {
        device_pubkey: keys.device_pubkey,
        endpoint_id: keys.endpoint_id,
        role,
        vault_device_uuid: Uuid::from_bytes([tag; 16]),
        name_sealed: Vec::new(),
        added_at: 0,
    }
}

fn signed_list(devices: Vec<ListedDevice>) -> SignedList {
    let vault_secret = random_secret_key();
    sign_list(
        DeviceList {
            vault: xonly_public_key(&vault_secret).expect("vault key"),
            generation: 1,
            devices,
            removed: Vec::new(),
            issued_by: [0; 32],
            issued_at: 0,
            base_list_hash: None,
        },
        &vault_secret,
    )
    .expect("sign")
}

fn valid(list: &SignedList) -> BTreeMap<[u8; 32], SignedList> {
    BTreeMap::from([(list.hash, list.clone())])
}

fn held_keys(db: &haex_crdt::Database) -> i64 {
    use crate::storage::query::Query;
    query::read(db, |r| {
        r.query_row(
            "SELECT COUNT(*) FROM vault_content_keys_no_sync",
            &[],
            |row| row.get(0),
        )
    })
    .expect("count")
    .unwrap_or(0)
}

#[test]
fn nip44_v2_matches_the_reference_vector() {
    // First `encrypt_decrypt` vector of the NIP-44 v2 test vectors.
    let secret = nostr::key::SecretKey::from_hex(
        "0000000000000000000000000000000000000000000000000000000000000001",
    )
    .expect("sec1");
    let recipient = nostr::key::SecretKey::from_hex(
        "0000000000000000000000000000000000000000000000000000000000000002",
    )
    .expect("sec2");
    let recipient_pubkey = nostr::key::Keys::new(recipient.clone()).public_key();
    let mut nonce = [0u8; 32];
    nonce[31] = 1;
    let payload =
        nip44::encrypt_with_nonce(&secret, &recipient_pubkey, "a", nip44::Nonce::V2(nonce))
            .expect("encrypt");
    assert_eq!(
        payload,
        "AgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABee0G5VSK0/9YypIObAtDKfYEAjD35uVkHyB0F4DwrcNaCXlCWZKaArsGrY6M9wnuTMxWfp1RTN9Xga8no+kF5Vsb"
    );
    let sender_pubkey = nostr::key::Keys::new(secret).public_key();
    assert_eq!(
        nip44::decrypt(&recipient, &sender_pubkey, &payload).expect("decrypt"),
        "a"
    );
}

#[test]
fn the_key_id_is_the_tagged_hash_prefix() {
    let key = [5u8; 32];
    let digest: [u8; 32] = Sha256::digest([b"haex-key-id/v1".as_slice(), &key].concat()).into();
    assert_eq!(key_id(&key), digest[..16]);
}

#[test]
fn every_listed_device_unwraps_its_envelope() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let main = DeviceKeys::generate();
    let linked = DeviceKeys::generate();
    let list = signed_list(vec![
        listed(&main, Role::Main, 1),
        listed(&linked, Role::Linked, 2),
    ]);
    let key = ContentKey::generate(1);

    db.write(|tx| issue_generation(tx, &key, &list, &main, 5))
        .expect("issue");
    assert_eq!(held_keys(&db), 1, "the issuer holds its key right away");

    // The linked device sees the same rows in its own vault copy.
    db.write(|tx| {
        tx.execute("DELETE FROM vault_content_keys_no_sync", &[])?;
        Ok(())
    })
    .expect("forget the key");
    let added = db
        .write(|tx| unwrap_own_envelopes(tx, &linked, &valid(&list)))
        .expect("unwrap");
    assert_eq!(added, 1);
    let current = query::read(&db, |r| current_key(r, &[]))
        .expect("read")
        .expect("a key");
    assert_eq!(current.key_id, key.key_id);
    assert_eq!(current.key.as_slice(), key.key.as_slice());
}

#[test]
fn a_linked_device_cannot_issue_a_generation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let main = DeviceKeys::generate();
    let linked = DeviceKeys::generate();
    let list = signed_list(vec![
        listed(&main, Role::Main, 1),
        listed(&linked, Role::Linked, 2),
    ]);

    db.write(|tx| issue_generation(tx, &ContentKey::generate(1), &list, &linked, 5))
        .expect("the rows can be written");

    let generations = query::read(&db, |r| load_generations(r)).expect("read");
    assert_eq!(generations.len(), 1);
    assert_eq!(
        generations[0].verify(&valid(&list)).err(),
        Some(KeyError::Unauthorized)
    );
    db.write(|tx| {
        tx.execute("DELETE FROM vault_content_keys_no_sync", &[])?;
        Ok(())
    })
    .expect("forget the key");
    let added = db
        .write(|tx| unwrap_own_envelopes(tx, &main, &valid(&list)))
        .expect("unwrap");
    assert_eq!(added, 0, "an unauthorized generation is never unwrapped");
}

#[test]
fn a_generation_for_an_unknown_list_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let main = DeviceKeys::generate();
    let list = signed_list(vec![listed(&main, Role::Main, 1)]);
    db.write(|tx| issue_generation(tx, &ContentKey::generate(1), &list, &main, 5))
        .expect("issue");

    let generations = query::read(&db, |r| load_generations(r)).expect("read");
    assert_eq!(
        generations[0].verify(&BTreeMap::new()).err(),
        Some(KeyError::UnknownList)
    );
}

#[test]
fn an_envelope_with_a_tampered_sender_is_ignored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let main = DeviceKeys::generate();
    let linked = DeviceKeys::generate();
    let list = signed_list(vec![
        listed(&main, Role::Main, 1),
        listed(&linked, Role::Linked, 2),
    ]);
    let key = ContentKey::generate(1);

    db.write(|tx| issue_generation(tx, &key, &list, &main, 5))
        .expect("issue");
    db.write(|tx| {
        tx.execute("DELETE FROM vault_content_keys_no_sync", &[])?;
        tx.execute(
            "UPDATE vault_key_envelopes SET sender = ?1 WHERE key_id = ?2",
            rusqlite::params![[0u8; 32].as_slice(), key.key_id.as_slice()],
        )?;
        Ok(())
    })
    .expect("tamper envelope");

    let added = db
        .write(|tx| unwrap_own_envelopes(tx, &linked, &valid(&list)))
        .expect("a malformed envelope does not abort the write");
    assert_eq!(added, 0);
}

#[test]
fn from_hex_rejects_non_ascii_input() {
    assert_eq!(from_hex("éé"), Err(KeyError::Malformed));
}

#[test]
fn the_current_key_skips_generations_wrapped_for_a_removed_device() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let main = DeviceKeys::generate();
    let gone = DeviceKeys::generate();
    let first = signed_list(vec![
        listed(&main, Role::Main, 1),
        listed(&gone, Role::Linked, 2),
    ]);
    let second = signed_list(vec![listed(&main, Role::Main, 1)]);
    let old = ContentKey::generate(1);
    let new = ContentKey::generate(2);
    db.write(|tx| {
        issue_generation(tx, &old, &first, &main, 5)?;
        issue_generation(tx, &new, &second, &main, 6)
    })
    .expect("issue");

    let current = query::read(&db, |r| current_key(r, &[gone.device_pubkey]))
        .expect("read")
        .expect("a key");
    assert_eq!(current.generation, 2);

    let only_old = query::read(&db, |r| current_key(r, &[main.device_pubkey])).expect("read");
    assert!(
        only_old.is_none(),
        "every generation reached the removed device"
    );
}

/// A skipped higher generation is listened on as well, but does not take
/// the place of a lower generation used by a device that was away.
#[test]
fn presence_listens_on_lower_generations_and_on_skipped_higher_ones() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let main = DeviceKeys::generate();
    let gone = DeviceKeys::generate();
    let with_gone = signed_list(vec![
        listed(&main, Role::Main, 1),
        listed(&gone, Role::Linked, 2),
    ]);
    let without = signed_list(vec![listed(&main, Role::Main, 1)]);
    let keys: Vec<ContentKey> = (1..=6).map(ContentKey::generate).collect();
    db.write(|tx| {
        for key in &keys {
            let list = if key.generation == 6 {
                &with_gone
            } else {
                &without
            };
            issue_generation(tx, key, list, &main, 5)?;
        }
        Ok(())
    })
    .expect("issue");

    let current = query::read(&db, |r| current_key(r, &[gone.device_pubkey]))
        .expect("read")
        .expect("a key");
    assert_eq!(current.generation, 5);
    let listening = query::read(&db, |r| listening_keys(r, &current, 2)).expect("read");
    let listening: Vec<[u8; 32]> = listening.iter().map(|key| **key).collect();
    assert_eq!(listening, vec![*keys[3].key, *keys[2].key, *keys[5].key]);
}

#[test]
fn a_sealed_name_opens_only_for_its_device() {
    let key = ContentKey::generate(3);
    let device = [1u8; 32];
    let sealed = seal_name(&key, &device, "Laptop");

    assert_eq!(open_name(&key, &device, &sealed).as_deref(), Ok("Laptop"));
    assert_eq!(sealed_name_key_id(&sealed), Some(key.key_id));
    assert_eq!(
        open_name(&key, &[2u8; 32], &sealed),
        Err(KeyError::SealedName)
    );
    assert_eq!(
        open_name(&ContentKey::generate(3), &device, &sealed),
        Err(KeyError::SealedName)
    );
}
