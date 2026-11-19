//! Tests for the entries (spec 034, US1, FR-001/002/003/026, research R7 and R15) against a real
//! temporary vault: creating, the overview without secrets, the detail with flags, partial updates
//! with the conflict check, and the TOTP values that may arrive invalid.

// The tests read raw columns that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::items::{self, otp_params_of};
use super::model::{ItemInput, ItemPatch, KeyValueInput, KeyValuePatch, OtpState, Patch};
use super::test_support::{open_test_vault, MARKER};
use crate::error::HaexError;
use crate::storage::query;

fn write<R>(
    db: &Database,
    f: impl FnOnce(&mut haex_crdt::CrdtTransaction<'_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    db.write(|tx| f(tx).map_err(haex_crdt::Error::from))
        .map_err(HaexError::from)
}

fn read<R>(
    db: &Database,
    f: impl FnOnce(&mut query::Reader<'_, '_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    query::read(db, |q| f(q).map_err(haex_crdt::Error::from)).map_err(HaexError::from)
}

fn full_input() -> ItemInput {
    ItemInput {
        title: Some("Mail".to_string()),
        username: Some("alice".to_string()),
        password: Some(format!("{MARKER}-password")),
        note: Some(format!("{MARKER}-note")),
        url: Some("https://example.invalid".to_string()),
        icon: Some("lucide:mail".to_string()),
        color: Some("#336699".to_string()),
        expires_at: Some("2030-01-31".to_string()),
        otp_secret: Some("JBSWY3DPEHPK3PXP".to_string()),
        otp_digits: Some(8),
        otp_period: Some(60),
        otp_algorithm: Some("SHA256".to_string()),
        autofill_aliases: Some("{\"username\":[\"login\"]}".to_string()),
        tags: vec!["Work".to_string(), "work ".to_string(), "Bank".to_string()],
        key_values: vec![
            KeyValueInput {
                key: "PIN".to_string(),
                value: Some(format!("{MARKER}-pin")),
            },
            KeyValueInput {
                key: "  ".to_string(),
                value: Some("dropped".to_string()),
            },
        ],
    }
}

fn create(db: &Database, input: &ItemInput) -> String {
    write(db, |tx| items::create_item(tx, input, None)).expect("create")
}

fn raw(db: &Database, sql: &str, id: &str) -> Option<String> {
    db.with_connection(|conn| {
        Ok(conn.query_row(sql, params![id], |r| r.get::<_, Option<String>>(0))?)
    })
    .expect("raw read")
}

fn detail(db: &Database, id: &str) -> super::model::ItemDetail {
    read(db, |q| items::get_item(q, id))
        .expect("get")
        .expect("exists")
}

#[test]
fn create_item_stores_all_fields_and_returns_an_id() {
    let (_dir, db) = open_test_vault();
    let id = create(&db, &full_input());
    let d = detail(&db, &id);
    assert_eq!(d.header.id, id);
    assert_eq!(d.header.title.as_deref(), Some("Mail"));
    assert_eq!(d.header.username.as_deref(), Some("alice"));
    assert_eq!(d.header.url.as_deref(), Some("https://example.invalid"));
    assert_eq!(d.header.icon.as_deref(), Some("lucide:mail"));
    assert_eq!(d.header.color.as_deref(), Some("#336699"));
    assert_eq!(d.header.expires_at.as_deref(), Some("2030-01-31"));
    assert_eq!(d.note.as_deref(), Some(&*format!("{MARKER}-note")));
    assert_eq!(
        d.autofill_aliases.as_deref(),
        Some("{\"username\":[\"login\"]}")
    );
    assert_eq!((d.otp_digits, d.otp_period), (Some(8), Some(60)));
    assert_eq!(d.otp_algorithm.as_deref(), Some("SHA256"));
    assert!(d.header.has_password && d.header.has_totp && d.has_otp_secret);
    assert_eq!(d.otp_state, OtpState::Valid);
    // "Work" and "work " are one tag.
    let mut tags: Vec<_> = d.header.tags.iter().map(|t| t.name.clone()).collect();
    tags.sort();
    assert_eq!(tags, vec!["Bank".to_string(), "Work".to_string()]);
    // The custom field with an empty key was dropped, the other stored.
    assert_eq!(d.key_values.len(), 1);
    assert_eq!(d.key_values[0].key.as_deref(), Some("PIN"));
    assert!(d.key_values[0].has_value);
    // Timestamps are RFC 3339 with milliseconds.
    for stamp in [&d.header.created_at, &d.header.updated_at] {
        let text = stamp.as_deref().expect("stamp");
        assert_eq!(text.len(), 24, "{text}");
        assert!(text.ends_with('Z'));
    }
}

#[test]
fn an_empty_or_missing_title_is_stored_as_it_is() {
    let (_dir, db) = open_test_vault();
    let none = create(&db, &ItemInput::default());
    let empty = create(
        &db,
        &ItemInput {
            title: Some(String::new()),
            username: Some("bob".to_string()),
            ..ItemInput::default()
        },
    );
    let overview = read(&db, |q| items::load_overview(q)).expect("overview");
    let title_of = |id: &str| {
        overview
            .headers
            .iter()
            .find(|h| h.id == id)
            .expect("listed")
            .title
            .clone()
    };
    assert_eq!(title_of(&none), None);
    assert_eq!(title_of(&empty), Some(String::new()));
}

#[test]
fn the_overview_carries_no_secret_note_or_value() {
    let (_dir, db) = open_test_vault();
    let id = create(&db, &full_input());
    write(&db, |tx| {
        tx.execute(
            "INSERT INTO haex_passwords_groups (id, name) VALUES ('g1', 'Folder')",
            &[],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_groups (id, name) VALUES ('gone', 'Soon deleted')",
            &[],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_group_items (item_id, group_id) VALUES (?1, 'gone')",
            params![id],
        )?;
        Ok(())
    })
    .expect("seed");
    // A sync applies a remote delete with foreign keys off: the link then names a folder that
    // is not there.
    db.with_connection(|conn| {
        conn.execute_batch("PRAGMA foreign_keys = OFF")?;
        conn.execute("DELETE FROM haex_passwords_groups WHERE id = 'gone'", [])?;
        conn.execute_batch("PRAGMA foreign_keys = ON")?;
        Ok(())
    })
    .expect("dangling link");
    let overview = read(&db, |q| items::load_overview(q)).expect("overview");
    let json = serde_json::to_string(&overview).expect("json");
    assert!(!json.contains(MARKER), "{json}");
    let header = &overview.headers[0];
    assert!(header.has_password && header.has_totp);
    assert_eq!(header.passkey_count, 0);
    assert_eq!(
        header.group_id, None,
        "an unknown group id counts as the root"
    );
    assert_eq!(overview.groups.len(), 1);
    let work = overview
        .tags
        .iter()
        .find(|t| t.name == "Work")
        .expect("tag");
    assert_eq!(work.item_count, 1);
}

#[test]
fn get_item_reports_flags_for_secrets_and_the_custom_values() {
    let (_dir, db) = open_test_vault();
    let id = create(&db, &full_input());
    let json = serde_json::to_string(&detail(&db, &id)).expect("json");
    for secret in ["-password", "JBSWY3DPEHPK3PXP"] {
        assert!(!json.contains(secret), "{secret} leaked: {json}");
    }
    // The note and the custom values are part of the detail (custom values show unmasked,
    // spec 034 FR-005 as amended); password and TOTP secret are flags only.
    assert!(json.contains("-pin"));
    assert!(json.contains("hasPassword"));
    assert!(read(&db, |q| items::get_item(q, "missing"))
        .expect("get")
        .is_none());
}

#[test]
fn a_patch_without_the_password_leaves_it_unchanged() {
    let (_dir, db) = open_test_vault();
    let id = create(&db, &full_input());
    let before = detail(&db, &id).header.updated_at.expect("stamp");
    let patch = ItemPatch {
        title: Patch::Set("Renamed".to_string()),
        ..ItemPatch::default()
    };
    write(&db, |tx| items::update_item(tx, &id, &before, &patch)).expect("update");
    assert_eq!(
        raw(
            &db,
            "SELECT password FROM haex_passwords_item_details WHERE id = ?1",
            &id
        )
        .as_deref(),
        Some(&*format!("{MARKER}-password"))
    );
    assert_eq!(detail(&db, &id).header.title.as_deref(), Some("Renamed"));
}

#[test]
fn a_patch_replaces_or_clears_the_password() {
    let (_dir, db) = open_test_vault();
    let id = create(&db, &full_input());
    let mut stamp = detail(&db, &id).header.updated_at.expect("stamp");
    let set = ItemPatch {
        password: Patch::Set("new-password".to_string()),
        ..ItemPatch::default()
    };
    stamp = write(&db, |tx| items::update_item(tx, &id, &stamp, &set)).expect("set");
    assert_eq!(
        raw(
            &db,
            "SELECT password FROM haex_passwords_item_details WHERE id = ?1",
            &id
        )
        .as_deref(),
        Some("new-password")
    );
    let clear = ItemPatch {
        password: Patch::Clear,
        ..ItemPatch::default()
    };
    write(&db, |tx| items::update_item(tx, &id, &stamp, &clear)).expect("clear");
    assert_eq!(
        raw(
            &db,
            "SELECT password FROM haex_passwords_item_details WHERE id = ?1",
            &id
        ),
        None
    );
    assert!(!detail(&db, &id).header.has_password);
}

#[test]
fn custom_fields_without_a_value_keep_it_and_empty_keys_are_dropped() {
    let (_dir, db) = open_test_vault();
    let id = create(&db, &full_input());
    let d = detail(&db, &id);
    let field_id = d.key_values[0].id.clone();
    let stamp = d.header.updated_at.expect("stamp");
    let patch = ItemPatch {
        key_values: Some(vec![
            KeyValuePatch {
                id: Some(field_id.clone()),
                key: "PIN code".to_string(),
                value: None,
            },
            KeyValuePatch {
                id: None,
                key: String::new(),
                value: Some("dropped".to_string()),
            },
            KeyValuePatch {
                id: None,
                key: "Recovery".to_string(),
                value: Some("r-1".to_string()),
            },
        ]),
        ..ItemPatch::default()
    };
    write(&db, |tx| items::update_item(tx, &id, &stamp, &patch)).expect("update");
    let after = detail(&db, &id);
    assert_eq!(after.key_values.len(), 2);
    let kept = after
        .key_values
        .iter()
        .find(|f| f.id == field_id)
        .expect("kept");
    assert_eq!(kept.key.as_deref(), Some("PIN code"));
    assert_eq!(
        raw(
            &db,
            "SELECT value FROM haex_passwords_item_key_values WHERE id = ?1",
            &field_id
        )
        .as_deref(),
        Some(&*format!("{MARKER}-pin")),
        "a missing value keeps the stored one"
    );
    // A list without a stored field removes it.
    let stamp = after.header.updated_at.expect("stamp");
    let only_new = ItemPatch {
        key_values: Some(vec![]),
        ..ItemPatch::default()
    };
    write(&db, |tx| items::update_item(tx, &id, &stamp, &only_new)).expect("replace");
    assert!(detail(&db, &id).key_values.is_empty());
}

#[test]
fn updated_at_changes_on_every_update() {
    let (_dir, db) = open_test_vault();
    let id = create(&db, &full_input());
    let first = detail(&db, &id).header.updated_at.expect("stamp");
    let patch = ItemPatch::default();
    let second = write(&db, |tx| items::update_item(tx, &id, &first, &patch)).expect("update");
    assert_ne!(first, second);
    assert!(second > first, "{first} < {second}");
    assert_eq!(second.len(), 24);
    assert_eq!(
        detail(&db, &id).header.updated_at.as_deref(),
        Some(&*second)
    );
}

#[test]
fn a_stale_token_or_a_missing_entry_is_a_conflict() {
    let (_dir, db) = open_test_vault();
    let id = create(&db, &full_input());
    let first = detail(&db, &id).header.updated_at.expect("stamp");
    let patch = ItemPatch {
        title: Patch::Set("A".to_string()),
        ..ItemPatch::default()
    };
    write(&db, |tx| items::update_item(tx, &id, &first, &patch)).expect("first update");
    // The token is now stale.
    let stale = write(&db, |tx| items::update_item(tx, &id, &first, &patch));
    assert!(matches!(
        stale,
        Err(HaexError::PasswordsConflict { reason }) if reason == "changed"
    ));
    let missing = write(&db, |tx| items::update_item(tx, "nope", &first, &patch));
    assert!(matches!(
        missing,
        Err(HaexError::PasswordsConflict { reason }) if reason == "deleted"
    ));
    assert_eq!(detail(&db, &id).header.title.as_deref(), Some("A"));
}

#[test]
fn an_otp_secret_is_normalised_and_validated() {
    let (_dir, db) = open_test_vault();
    let id = create(&db, &ItemInput::default());
    let stamp = detail(&db, &id).header.updated_at.expect("stamp");
    let good = ItemPatch {
        otp_secret: Patch::Set("jbsw y3dp ehpk 3pxp".to_string()),
        ..ItemPatch::default()
    };
    let stamp = write(&db, |tx| items::update_item(tx, &id, &stamp, &good)).expect("good");
    assert_eq!(
        raw(
            &db,
            "SELECT otp_secret FROM haex_passwords_item_details WHERE id = ?1",
            &id
        )
        .as_deref(),
        Some("JBSWY3DPEHPK3PXP")
    );
    let bad = ItemPatch {
        otp_secret: Patch::Set("JBSWY3DPEHPK3PX!".to_string()),
        title: Patch::Set("must not be written".to_string()),
        ..ItemPatch::default()
    };
    let refused = write(&db, |tx| items::update_item(tx, &id, &stamp, &bad));
    assert!(matches!(
        refused,
        Err(HaexError::InvalidInput { reason }) if reason == "otpSecret"
    ));
    assert_eq!(detail(&db, &id).header.title, None, "nothing was written");
    // The same on create: an invalid secret is refused, nothing is stored.
    let created = write(&db, |tx| {
        items::create_item(
            tx,
            &ItemInput {
                otp_secret: Some("not base32 !".to_string()),
                ..ItemInput::default()
            },
            None,
        )
    });
    assert!(matches!(created, Err(HaexError::InvalidInput { .. })));
    let overview = read(&db, |q| items::load_overview(q)).expect("overview");
    assert_eq!(overview.headers.len(), 1);
}

#[test]
fn a_value_that_arrives_invalid_is_reported_and_can_be_fixed() {
    let (_dir, db) = open_test_vault();
    let id = create(&db, &full_input());
    // What a sync from another device or an import can bring.
    write(&db, |tx| {
        tx.execute(
            "UPDATE haex_passwords_item_details \
             SET otp_secret = 'not base32 !!', otp_digits = 0, otp_algorithm = 'MD5' WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    })
    .expect("simulate sync");
    let d = detail(&db, &id);
    assert_eq!(d.otp_state, OtpState::Invalid, "get_item does not fail");
    assert!(d.has_otp_secret);
    let code = read(&db, |q| otp_params_of(q, &id).map(|_| ()));
    assert!(matches!(code, Err(HaexError::InvalidInput { .. })));
    // An update that does not touch the TOTP is not blocked by it.
    let stamp = d.header.updated_at.expect("stamp");
    let rename = ItemPatch {
        title: Patch::Set("Still editable".to_string()),
        ..ItemPatch::default()
    };
    let stamp = write(&db, |tx| items::update_item(tx, &id, &stamp, &rename)).expect("rename");
    assert_eq!(
        detail(&db, &id).otp_state,
        OtpState::Invalid,
        "nothing changed silently"
    );
    // Replacing the secret fixes it.
    let fix = ItemPatch {
        otp_secret: Patch::Set("JBSWY3DPEHPK3PXP".to_string()),
        otp_digits: Patch::Set(6),
        otp_algorithm: Patch::Set("SHA1".to_string()),
        ..ItemPatch::default()
    };
    let stamp = write(&db, |tx| items::update_item(tx, &id, &stamp, &fix)).expect("fix");
    assert_eq!(detail(&db, &id).otp_state, OtpState::Valid);
    // Clearing works as well.
    let clear = ItemPatch {
        otp_secret: Patch::Clear,
        ..ItemPatch::default()
    };
    write(&db, |tx| items::update_item(tx, &id, &stamp, &clear)).expect("clear");
    let cleared = detail(&db, &id);
    assert_eq!(cleared.otp_state, OtpState::None);
    assert!(!cleared.header.has_totp);
}

#[test]
fn null_digits_period_and_algorithm_read_as_the_defaults() {
    let (_dir, db) = open_test_vault();
    let id = create(
        &db,
        &ItemInput {
            otp_secret: Some("JBSWY3DPEHPK3PXP".to_string()),
            ..ItemInput::default()
        },
    );
    write(&db, |tx| {
        tx.execute(
            "UPDATE haex_passwords_item_details \
             SET otp_digits = NULL, otp_period = NULL, otp_algorithm = NULL WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    })
    .expect("null the parameters");
    assert_eq!(detail(&db, &id).otp_state, OtpState::Valid);
    let params = read(&db, |q| otp_params_of(q, &id)).expect("params");
    assert_eq!((params.digits, params.period), (6, 30));
    assert_eq!(params.algorithm.as_str(), "SHA1");
}

#[test]
fn get_item_lists_passkeys_without_their_keys() {
    let (_dir, db) = open_test_vault();
    let id = create(&db, &ItemInput::default());
    write(&db, |tx| {
        tx.execute(
            "INSERT INTO haex_passwords_passkeys \
             (id, item_id, credential_id, relying_party_id, relying_party_name, user_name, \
              user_handle, private_key, public_key, algorithm, nickname) \
             VALUES ('pk1', ?1, 'Y3JlZA==', 'example.com', 'Example', 'alice', 'dXNlcg==', \
                     ?2, 'public', -7, 'Laptop')",
            params![id, format!("{MARKER}-private")],
        )?;
        Ok(())
    })
    .expect("passkey");
    let d = detail(&db, &id);
    assert_eq!(d.passkeys.len(), 1);
    assert_eq!(d.passkeys[0].relying_party_id, "example.com");
    assert_eq!(d.passkeys[0].nickname.as_deref(), Some("Laptop"));
    assert_eq!(d.header.passkey_count, 1);
    assert!(!serde_json::to_string(&d).expect("json").contains(MARKER));
}
