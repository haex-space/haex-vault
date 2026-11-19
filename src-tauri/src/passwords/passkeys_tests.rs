//! Tests for passkeys as data (spec 034, FR-004, research R12): list, rename and delete without
//! ever showing a key, the derived id that keeps a credential unique, and the derivation of the
//! public key from the private one for ES256, EdDSA and RS256 with keys made at run time.

// The tests read raw columns that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use base64::Engine;
use haex_crdt::Database;
use pkcs8::{EncodePrivateKey, EncodePublicKey};
use zeroize::Zeroizing;

use super::ids::passkey_id;
use super::items;
use super::model::ItemInput;
use super::passkeys::{self, DeriveError, InsertOutcome, PasskeyInput};
use super::test_support::{open_test_vault, MARKER};
use crate::error::HaexError;
use crate::storage::query;

const STANDARD: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;
const URL_SAFE: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

fn write<R>(
    db: &Database,
    f: impl FnOnce(&mut haex_crdt::CrdtTransaction<'_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    db.write(|tx| f(tx).map_err(haex_crdt::Error::from))
        .map_err(HaexError::from)
}

fn item(db: &Database) -> String {
    write(db, |tx| items::create_item(tx, &ItemInput::default(), None)).expect("item")
}

fn input(item_id: Option<&str>, credential: &str) -> PasskeyInput {
    PasskeyInput {
        item_id: item_id.map(str::to_string),
        credential_id: credential.to_string(),
        relying_party_id: "example.com".to_string(),
        relying_party_name: Some("Example".to_string()),
        user_name: Some("alice".to_string()),
        user_display_name: None,
        user_handle: "dXNlcg==".to_string(),
        private_key: Zeroizing::new(format!("{MARKER}-private")),
        public_key: "public".to_string(),
        algorithm: -7,
        sign_count: 3,
        is_discoverable: true,
        icon: None,
        color: None,
        nickname: Some("Laptop".to_string()),
        created_at: Some("2026-10-01T10:00:00.000Z".to_string()),
        last_used_at: None,
    }
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|conn| Ok(conn.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
}

#[test]
fn a_passkey_is_listed_without_its_keys() {
    let (_dir, db) = open_test_vault();
    let item_id = item(&db);
    let outcome = write(&db, |tx| {
        passkeys::insert(tx, &input(Some(&item_id), "Y3JlZDE="))
    })
    .expect("insert");
    assert!(matches!(outcome, InsertOutcome::Created(_)));
    let list = query::read(&db, |q| {
        passkeys::list_for_item(q, &item_id).map_err(Into::into)
    })
    .expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].relying_party_id, "example.com");
    assert_eq!(list[0].relying_party_name.as_deref(), Some("Example"));
    assert_eq!(list[0].user_name.as_deref(), Some("alice"));
    assert_eq!(list[0].nickname.as_deref(), Some("Laptop"));
    assert_eq!(list[0].algorithm, -7);
    let json = serde_json::to_string(&list).expect("json");
    assert!(!json.contains(MARKER) && !json.contains("public"), "{json}");
}

#[test]
fn the_same_credential_inserted_twice_is_one_row() {
    let (_dir, db) = open_test_vault();
    let item_id = item(&db);
    let first = write(&db, |tx| {
        passkeys::insert(tx, &input(Some(&item_id), "Y3JlZDE="))
    })
    .expect("first");
    let second = write(&db, |tx| {
        passkeys::insert(tx, &input(Some(&item_id), "Y3JlZDE="))
    })
    .expect("second");
    match (first, second) {
        (InsertOutcome::Created(id), InsertOutcome::Duplicate) => {
            assert_eq!(id, passkey_id("Y3JlZDE=").to_string());
        }
        other => panic!("expected Created then Duplicate, got {other:?}"),
    }
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_passkeys"),
        1
    );
}

#[test]
fn rename_changes_only_the_nickname() {
    let (_dir, db) = open_test_vault();
    let item_id = item(&db);
    let InsertOutcome::Created(id) = write(&db, |tx| {
        passkeys::insert(tx, &input(Some(&item_id), "Y3JlZDE="))
    })
    .expect("insert") else {
        panic!("created");
    };
    write(&db, |tx| passkeys::rename(tx, &id, Some("Phone"))).expect("rename");
    let list = query::read(&db, |q| {
        passkeys::list_for_item(q, &item_id).map_err(Into::into)
    })
    .expect("list");
    assert_eq!(list[0].nickname.as_deref(), Some("Phone"));
    assert_eq!(list[0].relying_party_id, "example.com");
    assert_eq!(list[0].user_name.as_deref(), Some("alice"));
    let missing = write(&db, |tx| passkeys::rename(tx, "nope", Some("x")));
    assert!(matches!(missing, Err(HaexError::PasswordsNotFound)));
}

#[test]
fn delete_removes_one_row_and_leaves_the_others() {
    let (_dir, db) = open_test_vault();
    let item_id = item(&db);
    let InsertOutcome::Created(first) = write(&db, |tx| {
        passkeys::insert(tx, &input(Some(&item_id), "Y3JlZDE="))
    })
    .expect("one") else {
        panic!("created");
    };
    write(&db, |tx| {
        passkeys::insert(tx, &input(Some(&item_id), "Y3JlZDI="))
    })
    .expect("two");
    write(&db, |tx| passkeys::delete(tx, &first)).expect("delete");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_passkeys"),
        1
    );
    assert!(matches!(
        write(&db, |tx| passkeys::delete(tx, &first)),
        Err(HaexError::PasswordsNotFound)
    ));
}

#[test]
fn a_passkey_without_an_entry_is_listed_under_none() {
    let (_dir, db) = open_test_vault();
    let item_id = item(&db);
    write(&db, |tx| passkeys::insert(tx, &input(None, "b3JwaGFu"))).expect("insert");
    let list = query::read(&db, |q| {
        passkeys::list_for_item(q, &item_id).map_err(Into::into)
    })
    .expect("list");
    assert!(list.is_empty());
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_passkeys WHERE item_id IS NULL"
        ),
        1
    );
}

fn random_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).expect("random");
    seed
}

#[test]
fn es256_public_key_is_derived_from_the_private_key() {
    let secret = p256::SecretKey::from_slice(&random_seed()).expect("key");
    let private = secret.to_pkcs8_der().expect("pkcs8");
    let expected = secret.public_key().to_public_key_der().expect("spki");
    let derived = passkeys::derive_public_key(-7, private.as_bytes()).expect("derive");
    assert_eq!(derived, expected.as_bytes());
}

#[test]
fn eddsa_public_key_is_derived_from_the_private_key() {
    let signing = ed25519_dalek::SigningKey::from_bytes(&random_seed());
    let private = signing.to_pkcs8_der().expect("pkcs8");
    let expected = signing.verifying_key().to_public_key_der().expect("spki");
    let derived = passkeys::derive_public_key(-8, private.as_bytes()).expect("derive");
    assert_eq!(derived, expected.as_bytes());
}

#[test]
fn rs256_public_key_is_derived_from_the_private_key() {
    use rsa::pkcs8::{EncodePrivateKey as _, EncodePublicKey as _};
    let mut rng = getrandom::rand_core::UnwrapErr(getrandom::SysRng);
    let key = rsa::RsaPrivateKey::new(&mut rng, 2048).expect("rsa key");
    let private = key.to_pkcs8_der().expect("pkcs8");
    let expected = rsa::RsaPublicKey::from(&key)
        .to_public_key_der()
        .expect("spki");
    let derived = passkeys::derive_public_key(-257, private.as_bytes()).expect("derive");
    assert_eq!(derived, expected.as_bytes());
}

#[test]
fn an_unknown_algorithm_or_an_unreadable_key_is_an_error_without_the_key() {
    let secret = p256::SecretKey::from_slice(&random_seed()).expect("key");
    let private = secret.to_pkcs8_der().expect("pkcs8");
    assert_eq!(
        passkeys::derive_public_key(-35, private.as_bytes()),
        Err(DeriveError::UnsupportedAlgorithm)
    );
    assert_eq!(
        passkeys::derive_public_key(-7, b"not a key at all"),
        Err(DeriveError::UnreadableKey)
    );
    // The wrong algorithm for a readable key is unreadable as that algorithm.
    assert_eq!(
        passkeys::derive_public_key(-8, private.as_bytes()),
        Err(DeriveError::UnreadableKey)
    );
    let text = format!("{:?}", DeriveError::UnreadableKey);
    assert!(!text.contains(MARKER));
}

#[test]
fn the_base64_form_accepts_standard_and_url_safe_text() {
    let secret = p256::SecretKey::from_slice(&random_seed()).expect("key");
    let private = secret.to_pkcs8_der().expect("pkcs8");
    let expected = STANDARD.encode(
        secret
            .public_key()
            .to_public_key_der()
            .expect("spki")
            .as_bytes(),
    );
    for text in [
        STANDARD.encode(private.as_bytes()),
        URL_SAFE.encode(private.as_bytes()),
    ] {
        assert_eq!(
            passkeys::derive_public_key_base64(-7, &text).as_deref(),
            Ok(expected.as_str())
        );
    }
    assert_eq!(
        passkeys::derive_public_key_base64(-7, "%%%"),
        Err(DeriveError::UnreadableKey)
    );
}

#[test]
fn the_input_prints_no_private_key() {
    let printed = format!("{:?}", input(None, "Y3JlZA=="));
    assert!(!printed.contains(MARKER), "{printed}");
}
