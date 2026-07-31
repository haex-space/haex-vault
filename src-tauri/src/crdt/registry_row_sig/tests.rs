use super::{sign_registry_row, verify_registry_row, RegistryRowSigPayload, DOMAIN_TAG};
use ed25519_dalek::SigningKey;

fn base_payload() -> RegistryRowSigPayload<'static> {
    RegistryRowSigPayload {
        id: "row-1",
        space_id: "space-1",
        table_name: "ext_calendar_v1",
        row_pks: r#"{"id":"evt-42"}"#,
        extension_public_key: "epk",
        extension_name: "calendar",
        category: Some("work"),
        r#type: Some("event"),
        category_label: Some("Work Calendar"),
        type_label: Some("Termin"),
        authored_by_did: "did:key:alice",
        created_at: "2026-07-31T00:00:00Z",
    }
}

#[test]
fn test_registry_row_sig_payload_is_stable_across_reencoding() {
    let payload = base_payload();
    let bytes_a = payload.canonical_encoding();
    let bytes_b = payload.canonical_encoding();
    assert_eq!(bytes_a, bytes_b);

    // The preimage is length-prefixed like `column_sig::preimage::build_preimage`,
    // so the domain tag itself is prefixed with its big-endian u32 length —
    // it does not appear as a bare literal at offset 0.
    let mut expected_prefix = (DOMAIN_TAG.len() as u32).to_be_bytes().to_vec();
    expected_prefix.extend_from_slice(DOMAIN_TAG.as_bytes());
    assert!(bytes_a.starts_with(&expected_prefix));
}

#[test]
fn test_registry_row_sig_payload_null_vs_empty_string_distinguished() {
    let mut with_empty = base_payload();
    with_empty.category = Some("");

    let mut with_none = base_payload();
    with_none.category = None;

    assert_ne!(
        with_empty.canonical_encoding(),
        with_none.canonical_encoding()
    );
}

#[test]
fn canonical_encoding_is_position_sensitive() {
    // Swapping id and space_id VALUES must produce a different encoding
    // (guards against future refactor accidentally reordering push_field calls).
    let mut base = base_payload();
    let base_bytes = base.canonical_encoding();

    base.id = "space-1";
    base.space_id = "row-1";
    let swapped_bytes = base.canonical_encoding();

    assert_ne!(base_bytes, swapped_bytes);
}

#[test]
fn test_registry_row_sig_payload_different_domain_tag_than_column_sig() {
    assert_ne!(DOMAIN_TAG, crate::crdt::column_sig::preimage::DOMAIN_TAG);
    assert_eq!(DOMAIN_TAG, "haex/space-registry-row/v1");
}

#[test]
fn test_sign_and_verify_registry_row_roundtrip() {
    let sk = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let pk = sk.verifying_key();
    let payload = base_payload();

    let sig = sign_registry_row(&payload, &sk);
    assert!(verify_registry_row(&payload, &sig, &pk));
}

#[test]
fn test_verify_fails_for_wrong_key() {
    let sk = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let sk_other = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let pk_other = sk_other.verifying_key();
    let payload = base_payload();

    let sig = sign_registry_row(&payload, &sk);
    assert!(!verify_registry_row(&payload, &sig, &pk_other));
}

#[test]
fn test_verify_fails_for_mutated_payload() {
    let sk = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let pk = sk.verifying_key();
    let mut payload = base_payload();

    let sig = sign_registry_row(&payload, &sk);
    payload.category = Some("private");
    assert!(!verify_registry_row(&payload, &sig, &pk));
}

#[test]
fn test_verify_fails_for_malformed_signature_bytes() {
    let sk = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let pk = sk.verifying_key();
    let payload = base_payload();

    // Signature too short
    assert!(!verify_registry_row(&payload, b"too short", &pk));
    // Signature 64 bytes but not a valid signature
    let bogus = [0u8; 64];
    assert!(!verify_registry_row(&payload, &bogus, &pk));
}
