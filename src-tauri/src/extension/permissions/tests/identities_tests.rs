// src-tauri/src/extension/permissions/tests/identities_tests.rs
//!
//! Unit tests for the identity-access enforcement primitives in
//! `permissions::identities`: the `private_key`-masking read projection and the
//! contact-only-write validator.

use crate::extension::error::ExtensionError;
use crate::extension::permissions::identities::{
    project_identity_read, validate_contact_insert, ContactInsert, IdentityRow,
};

fn own_identity_row() -> IdentityRow {
    // An OWNED identity has a private_key set.
    IdentityRow {
        id: "id-own".to_string(),
        did: "did:key:zOwn".to_string(),
        name: "Alice".to_string(),
        source: "own".to_string(),
        private_key: Some("SUPER-SECRET-PRIVATE-KEY".to_string()),
        avatar: Some("avatar-data".to_string()),
        avatar_options: Some(r#"{"shape":"circle"}"#.to_string()),
        notes: Some("my own identity".to_string()),
    }
}

fn contact_row() -> IdentityRow {
    // A CONTACT has private_key NULL.
    IdentityRow {
        id: "id-contact".to_string(),
        did: "did:key:zBob".to_string(),
        name: "Bob".to_string(),
        source: "contact".to_string(),
        private_key: None,
        avatar: Some("bob-avatar".to_string()),
        avatar_options: None,
        notes: Some("a friend".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Read projection — NEVER leaks private_key (own identities AND contacts).
// ---------------------------------------------------------------------------

#[test]
fn read_projection_strips_private_key_for_own_identity() {
    let row = own_identity_row();
    let view = project_identity_read(&row);

    // The DTO carries the public fields…
    assert_eq!(view.id, "id-own");
    assert_eq!(view.did, "did:key:zOwn");
    assert_eq!(view.name, "Alice");
    assert_eq!(view.source, "own");
    assert_eq!(view.avatar.as_deref(), Some("avatar-data"));
    assert_eq!(
        view.avatar_options.as_deref(),
        Some(r#"{"shape":"circle"}"#)
    );
    assert_eq!(view.notes.as_deref(), Some("my own identity"));

    // …and the serialized form must NEVER contain the private key.
    let json = serde_json::to_string(&view).expect("serialize");
    assert!(
        !json.contains("private"),
        "read view must not expose any private_key field: {json}"
    );
    assert!(
        !json.contains("SUPER-SECRET-PRIVATE-KEY"),
        "read view leaked the private key value: {json}"
    );
}

#[test]
fn read_projection_strips_private_key_for_contact() {
    let row = contact_row();
    let view = project_identity_read(&row);

    assert_eq!(view.id, "id-contact");
    assert_eq!(view.did, "did:key:zBob");
    assert_eq!(view.name, "Bob");
    assert_eq!(view.source, "contact");
    assert_eq!(view.avatar.as_deref(), Some("bob-avatar"));
    assert_eq!(view.notes.as_deref(), Some("a friend"));

    let json = serde_json::to_string(&view).expect("serialize");
    assert!(
        !json.contains("private"),
        "contact read view must not expose private_key: {json}"
    );
}

// ---------------------------------------------------------------------------
// Contact-only write validator.
// ---------------------------------------------------------------------------

fn valid_contact() -> ContactInsert {
    ContactInsert {
        did: "did:key:zCarol".to_string(),
        name: "Carol".to_string(),
        source: "contact".to_string(),
        avatar: None,
        avatar_options: None,
        notes: None,
    }
}

#[test]
fn contact_insert_accepts_valid_contact() {
    // A NEW contact: source='contact', private_key implicitly NULL (the struct
    // has no private_key field at all).
    assert!(validate_contact_insert(&valid_contact()).is_ok());
}

#[test]
fn contact_insert_rejects_owned_identity_create() {
    // source='own' is an attempt to create an owned identity → SecurityViolation.
    let mut insert = valid_contact();
    insert.source = "own".to_string();
    let err = validate_contact_insert(&insert).unwrap_err();
    assert!(matches!(err, ExtensionError::SecurityViolation { .. }));
}

#[test]
fn contact_insert_rejects_arbitrary_source() {
    // Any source other than exactly "contact" is rejected.
    let mut insert = valid_contact();
    insert.source = "system".to_string();
    let err = validate_contact_insert(&insert).unwrap_err();
    assert!(matches!(err, ExtensionError::SecurityViolation { .. }));
}

#[test]
fn contact_insert_rejects_empty_did() {
    let mut insert = valid_contact();
    insert.did = "   ".to_string();
    let err = validate_contact_insert(&insert).unwrap_err();
    assert!(matches!(err, ExtensionError::SecurityViolation { .. }));
}

#[test]
fn contact_insert_rejects_empty_name() {
    let mut insert = valid_contact();
    insert.name = "".to_string();
    let err = validate_contact_insert(&insert).unwrap_err();
    assert!(matches!(err, ExtensionError::SecurityViolation { .. }));
}

// The write payload type ([`ContactInsert`]) structurally forbids supplying a
// `private_key` (no such field) and forbids targeting an existing row (no `id`
// field) — so "reject non-null private_key", "reject update", and "reject
// delete" are enforced by the type, not a runtime branch. The serde shape below
// documents/guards that: a payload carrying `privateKey` or `id` does not
// deserialize into a ContactInsert that smuggles those through.
#[test]
fn contact_insert_payload_has_no_private_key_or_id_field() {
    // Even if a malicious caller sends extra fields, they are dropped — the
    // resulting struct only holds contact-safe fields.
    let raw = serde_json::json!({
        "did": "did:key:zMallory",
        "name": "Mallory",
        "source": "contact",
        "privateKey": "ATTACKER-SUPPLIED-KEY",
        "id": "existing-row-id"
    });
    let insert: ContactInsert = serde_json::from_value(raw).expect("deserialize");
    // The smuggled fields did not land anywhere; the value validates as a
    // plain new contact.
    assert!(validate_contact_insert(&insert).is_ok());
    let reser = serde_json::to_value(&insert).expect("serialize");
    assert!(reser.get("privateKey").is_none());
    assert!(reser.get("id").is_none());
}
