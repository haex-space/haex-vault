//! Row-shape unit tests for `haex_s3_shared_access`.
//!
//! F1 stays deliberately small — the helpers write and read `JsonValue`
//! rows through `execute_with_crdt` / `select_with_crdt`, so once the
//! chokepoint contract is trusted (well-covered by the CRDT core tests),
//! what's left worth exercising in isolation is the row-shape
//! validation. The wire-up integration tests land in F2/F3 alongside
//! the callers.

use super::*;
use serde_json::json;

/// A well-formed row round-trips through `row_to_shared_access`.
#[test]
fn well_formed_row_parses_all_columns() {
    let row = vec![
        json!("id-1"),
        json!("space-alpha"),
        json!("backend-x"),
        json!("did:key:zAliceKey"),
        json!("<sealed-cred-base64>"),
        json!(42_u64),
        json!("2026-08-27T00:00:00Z"),
        json!("2026-08-27T00:00:01Z"),
    ];
    let parsed = row_to_shared_access(row).expect("well-formed row");
    assert_eq!(parsed.id, "id-1");
    assert_eq!(parsed.space_id, "space-alpha");
    assert_eq!(parsed.backend_id, "backend-x");
    assert_eq!(parsed.member_did, "did:key:zAliceKey");
    assert_eq!(parsed.encrypted_cred, "<sealed-cred-base64>");
    assert_eq!(parsed.epoch, 42);
    assert_eq!(parsed.expires_at.as_deref(), Some("2026-08-27T00:00:00Z"));
    assert_eq!(parsed.created_at, "2026-08-27T00:00:01Z");
}

/// A row with SQL NULL in `expires_at` decodes to `None` — this is the
/// no-STS-expiry case (long-lived scoped credential).
#[test]
fn null_expires_at_decodes_to_none() {
    let row = vec![
        json!("id-2"),
        json!("space-alpha"),
        json!("backend-x"),
        json!("did:key:zBobKey"),
        json!("<sealed>"),
        json!(1_u64),
        JsonValue::Null,
        json!("2026-08-27T00:00:00Z"),
    ];
    let parsed = row_to_shared_access(row).expect("null expires_at");
    assert!(parsed.expires_at.is_none());
}

/// Wrong number of columns is a hard error — surfaces a schema/query
/// drift rather than a silent partial parse.
#[test]
fn short_row_rejected() {
    let row = vec![json!("id"), json!("space"), json!("backend")];
    let err = row_to_shared_access(row).expect_err("short row must be rejected");
    let msg = format!("{err}");
    assert!(msg.contains("expected 8 columns"), "got: {err}");
}

/// A non-string in a text column is rejected — the SQL type shape and
/// our decoder must agree.
#[test]
fn non_string_text_column_rejected() {
    let row = vec![
        json!(123), // id — should be a string, is a number
        json!("space"),
        json!("backend"),
        json!("did"),
        json!("<sealed>"),
        json!(1_u64),
        JsonValue::Null,
        json!("2026-08-27T00:00:00Z"),
    ];
    let err = row_to_shared_access(row).expect_err("non-string id must be rejected");
    assert!(format!("{err}").contains("column `id`"), "got: {err}");
}

/// The `epoch` column must be an integer — non-integer JSON is a
/// column-type mismatch we want to surface early.
#[test]
fn non_integer_epoch_rejected() {
    let row = vec![
        json!("id"),
        json!("space"),
        json!("backend"),
        json!("did"),
        json!("<sealed>"),
        json!("not-a-number"),
        JsonValue::Null,
        json!("2026-08-27T00:00:00Z"),
    ];
    let err = row_to_shared_access(row).expect_err("non-integer epoch must be rejected");
    assert!(format!("{err}").contains("epoch"), "got: {err}");
}

/// A malformed (non-string, non-null) `expires_at` value is rejected —
/// SQL NULL or a string are the only valid states.
#[test]
fn non_string_non_null_expires_at_rejected() {
    let row = vec![
        json!("id"),
        json!("space"),
        json!("backend"),
        json!("did"),
        json!("<sealed>"),
        json!(1_u64),
        json!(42),
        json!("2026-08-27T00:00:00Z"),
    ];
    let err = row_to_shared_access(row).expect_err("integer expires_at must be rejected");
    assert!(format!("{err}").contains("expires_at"), "got: {err}");
}
