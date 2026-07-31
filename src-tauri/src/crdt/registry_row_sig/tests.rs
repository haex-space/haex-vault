use super::{RegistryRowSigPayload, DOMAIN_TAG};

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
fn test_registry_row_sig_payload_field_order_is_deterministic() {
    // Encoding must be the same regardless of the field ORDER used to
    // construct the payload. Rust's named-field struct literals are already
    // order-independent, so this test documents the invariant rather than
    // exercising a real risk: `canonical_encoding` walks fields in the fixed
    // order defined by the impl, not in source-literal order.
    let forward = RegistryRowSigPayload {
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
    };
    let reordered = RegistryRowSigPayload {
        created_at: "2026-07-31T00:00:00Z",
        authored_by_did: "did:key:alice",
        type_label: Some("Termin"),
        category_label: Some("Work Calendar"),
        r#type: Some("event"),
        category: Some("work"),
        extension_name: "calendar",
        extension_public_key: "epk",
        row_pks: r#"{"id":"evt-42"}"#,
        table_name: "ext_calendar_v1",
        space_id: "space-1",
        id: "row-1",
    };

    assert_eq!(forward.canonical_encoding(), reordered.canonical_encoding());
}

#[test]
fn test_registry_row_sig_payload_different_domain_tag_than_column_sig() {
    assert_ne!(DOMAIN_TAG, crate::crdt::column_sig::preimage::DOMAIN_TAG);
    assert_eq!(DOMAIN_TAG, "haex/space-registry-row/v1");
}
