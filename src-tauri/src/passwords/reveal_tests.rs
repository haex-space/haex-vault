//! Tests for revealing and copying values (spec 034, FR-005, FR-006, research R7): a value leaves
//! the backend only through these functions, a missing entry is `PasswordsNotFound`, and neither a
//! result nor an error prints a value.

// The tests read raw columns that the CRDT write path does not expose.
#![allow(clippy::redundant_closure)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::items;
use super::model::{CopyField, ItemInput, KeyValueInput, SecretField};
use super::reveal::{copy_value, reveal, totp_code};
use super::test_support::{open_test_vault, MARKER};
use crate::error::HaexError;
use crate::storage::query;

fn read<R>(
    db: &Database,
    f: impl FnOnce(&mut query::Reader<'_, '_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    query::read(db, |q| f(q).map_err(haex_crdt::Error::from)).map_err(HaexError::from)
}

fn create(db: &Database, input: &ItemInput) -> String {
    db.write(|tx| items::create_item(tx, input, None).map_err(haex_crdt::Error::from))
        .expect("create")
}

fn entry(db: &Database) -> (String, String) {
    let id = create(
        db,
        &ItemInput {
            username: Some("alice".to_string()),
            password: Some(format!("{MARKER}-password")),
            // The RFC 6238 SHA-1 test key "12345678901234567890" in Base32.
            otp_secret: Some("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".to_string()),
            otp_digits: Some(8),
            key_values: vec![KeyValueInput {
                key: "PIN".to_string(),
                value: Some(format!("{MARKER}-pin")),
            }],
            ..ItemInput::default()
        },
    );
    let field = read(db, |q| items::get_item(q, &id))
        .expect("get")
        .expect("exists")
        .key_values[0]
        .id
        .clone();
    (id, field)
}

#[test]
fn reveal_returns_the_stored_secrets() {
    let (_dir, db) = open_test_vault();
    let (id, field) = entry(&db);
    let password = read(&db, |q| reveal(q, &id, &SecretField::Password)).expect("password");
    assert_eq!(password.value.as_str(), format!("{MARKER}-password"));
    let otp = read(&db, |q| reveal(q, &id, &SecretField::OtpSecret)).expect("otp");
    assert_eq!(otp.value.as_str(), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
    let pin = read(&db, |q| {
        reveal(q, &id, &SecretField::KeyValue { id: field })
    })
    .expect("pin");
    assert_eq!(pin.value.as_str(), format!("{MARKER}-pin"));
}

#[test]
fn a_missing_entry_or_field_is_not_found() {
    let (_dir, db) = open_test_vault();
    let (id, _) = entry(&db);
    for result in [
        read(&db, |q| {
            reveal(q, "nope", &SecretField::Password).map(|_| ())
        }),
        read(&db, |q| {
            reveal(
                q,
                &id,
                &SecretField::KeyValue {
                    id: "nope".to_string(),
                },
            )
            .map(|_| ())
        }),
        read(&db, |q| {
            copy_value(q, "nope", &CopyField::Username).map(|_| ())
        }),
        read(&db, |q| totp_code(q, "nope", 59).map(|_| ())),
    ] {
        assert!(
            matches!(result, Err(HaexError::PasswordsNotFound)),
            "{result:?}"
        );
    }
}

#[test]
fn copy_value_returns_username_password_code_and_fields() {
    let (_dir, db) = open_test_vault();
    let (id, field) = entry(&db);
    let get = |f: CopyField| read(&db, |q| copy_value(q, &id, &f)).expect("copy");
    assert_eq!(get(CopyField::Username).as_str(), "alice");
    assert_eq!(
        get(CopyField::Password).as_str(),
        format!("{MARKER}-password")
    );
    assert_eq!(
        get(CopyField::KeyValue { id: field }).as_str(),
        format!("{MARKER}-pin")
    );
    let copied = get(CopyField::Totp);
    let code = copied.as_str();
    assert_eq!(code.len(), 8, "the code of this entry has eight digits");
    assert!(code.chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn copy_value_resolves_url_and_note() {
    let (_dir, db) = open_test_vault();
    let source = create(
        &db,
        &ItemInput {
            username: Some("bob".to_string()),
            ..ItemInput::default()
        },
    );
    let id = create(
        &db,
        &ItemInput {
            url: Some("https://example.invalid".to_string()),
            note: Some(format!("user {{${source}:username}}")),
            ..ItemInput::default()
        },
    );
    let get = |f: CopyField| read(&db, |q| copy_value(q, &id, &f)).expect("copy");
    assert_eq!(get(CopyField::Url).as_str(), "https://example.invalid");
    assert_eq!(get(CopyField::Note).as_str(), "user bob");
}

#[test]
fn the_editor_reads_password_and_custom_values_with_placeholders_unresolved() {
    let (_dir, db) = open_test_vault();
    let source = create(
        &db,
        &ItemInput {
            username: Some("bob".to_string()),
            password: Some("source-pw".to_string()),
            ..ItemInput::default()
        },
    );
    let stored_password = format!("{{${source}:password}}-x");
    let stored_value = format!("{{${source}:username}}");
    let id = create(
        &db,
        &ItemInput {
            password: Some(stored_password.clone()),
            key_values: vec![KeyValueInput {
                key: "Login".to_string(),
                value: Some(stored_value.clone()),
            }],
            ..ItemInput::default()
        },
    );
    let resolved = read(&db, |q| reveal(q, &id, &SecretField::Password)).expect("password");
    assert_eq!(resolved.value.as_str(), "source-pw-x");
    let stored = read(&db, |q| reveal(q, &id, &SecretField::StoredPassword)).expect("stored");
    assert_eq!(stored.value.as_str(), stored_password);
    let detail = read(&db, |q| items::get_item(q, &id))
        .expect("get")
        .expect("exists");
    assert_eq!(
        detail.key_values[0].value.as_deref(),
        Some(stored_value.as_str())
    );
}

#[test]
fn totp_code_follows_rfc_6238_and_reports_the_remaining_time() {
    let (_dir, db) = open_test_vault();
    let (id, _) = entry(&db);
    let at_59 = read(&db, |q| totp_code(q, &id, 59)).expect("code");
    assert_eq!(at_59.code, "94287082");
    assert_eq!(
        (at_59.digits, at_59.period, at_59.remaining_seconds),
        (8, 30, 1)
    );
    let later = read(&db, |q| totp_code(q, &id, 1111111109)).expect("code");
    assert_eq!(later.code, "07081804");
}

#[test]
fn an_invalid_stored_secret_is_invalid_input_not_a_crash() {
    let (_dir, db) = open_test_vault();
    let (id, _) = entry(&db);
    db.write(|tx| {
        tx.execute(
            "UPDATE haex_passwords_item_details SET otp_secret = 'not base32 !!' WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    })
    .expect("simulate sync");
    let result = read(&db, |q| totp_code(q, &id, 59));
    assert!(matches!(result, Err(HaexError::InvalidInput { .. })));
    // The same for the copy of the code.
    let copy = read(&db, |q| copy_value(q, &id, &CopyField::Totp).map(|_| ()));
    assert!(matches!(copy, Err(HaexError::InvalidInput { .. })));
}

#[test]
fn no_debug_print_of_a_result_or_an_error_holds_a_value() {
    let (_dir, db) = open_test_vault();
    let (id, field) = entry(&db);
    let revealed = read(&db, |q| reveal(q, &id, &SecretField::Password)).expect("reveal");
    assert!(!format!("{revealed:?}").contains(MARKER));
    let copied = read(&db, |q| copy_value(q, &id, &CopyField::Password)).expect("copy");
    assert!(!format!("{copied:?}").contains(MARKER));
    let errors = [
        read(&db, |q| {
            reveal(q, "nope", &SecretField::Password).map(|_| ())
        }),
        read(&db, |q| {
            reveal(q, &id, &SecretField::KeyValue { id: field.clone() }).map(|_| ())
        })
        .and_then(|_| read(&db, |q| totp_code(q, "nope", 0).map(|_| ()))),
    ];
    for error in errors {
        let printed = format!("{error:?}");
        assert!(!printed.contains(MARKER), "{printed}");
    }
}
