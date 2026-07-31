use super::{sign_registry_row, RegistryRowSigPayload, DOMAIN_TAG};
use ed25519_dalek::{SigningKey, Verifier};

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

// `verify_registry_row` doesn't exist yet (added alongside `verify.rs`), so
// these exercise `sign_registry_row` against raw `ed25519_dalek` verification
// rather than the project's own verify helper — mirrors
// `column_sig::sign_tests`, which does the same for `sign_column`.

#[test]
fn sign_registry_row_produces_a_verifiable_signature() {
    let sk = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let payload = base_payload();

    let sig_bytes = sign_registry_row(&payload, &sk);
    let sig_arr: [u8; 64] = sig_bytes.try_into().expect("64-byte signature");
    let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);

    sk.verifying_key()
        .verify(&payload.canonical_encoding(), &sig)
        .expect("signature verifies against the canonical encoding");
}

#[test]
fn sign_registry_row_differs_for_different_keys() {
    let sk_a = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let sk_b = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let payload = base_payload();

    let sig_a = sign_registry_row(&payload, &sk_a);
    let sig_b = sign_registry_row(&payload, &sk_b);
    assert_ne!(sig_a, sig_b);
}
