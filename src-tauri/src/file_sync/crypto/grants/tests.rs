//! Row-shape unit tests for `haex_file_grants`.
//!
//! Kept small on purpose — the CRDT chokepoint contract
//! (`execute_with_crdt` / `select_with_crdt`) is covered by the CRDT
//! core tests, so what's worth exercising in isolation here is the
//! `Vec<JsonValue>` → `FileGrantRow` decoder. End-to-end write→read
//! integration lands with the callers in Round F2/F4.

use super::*;
use serde_json::json;

#[test]
fn well_formed_row_parses_all_columns() {
    let row = vec![
        json!("grant-1"),
        json!("content/o/deadbeef"),
        json!("space-alpha"),
        json!("space-alpha/deadbeef.m"),
        json!(7_u64),
        json!("2026-08-27T00:00:00Z"),
    ];
    let parsed = row_to_grant(row).expect("well-formed row");
    assert_eq!(parsed.id, "grant-1");
    assert_eq!(parsed.content_key, "content/o/deadbeef");
    assert_eq!(parsed.space_id, "space-alpha");
    assert_eq!(parsed.sidecar_key, "space-alpha/deadbeef.m");
    assert_eq!(parsed.epoch, 7);
    assert_eq!(parsed.created_at, "2026-08-27T00:00:00Z");
}

#[test]
fn short_row_rejected() {
    let row = vec![json!("grant-1"), json!("content"), json!("space")];
    let err = row_to_grant(row).expect_err("short row must be rejected");
    assert!(
        format!("{err}").contains("expected 6 columns"),
        "got: {err}"
    );
}

#[test]
fn non_string_id_rejected() {
    let row = vec![
        json!(42),
        json!("content"),
        json!("space"),
        json!("sidecar"),
        json!(1_u64),
        json!("ts"),
    ];
    let err = row_to_grant(row).expect_err("non-string id must be rejected");
    assert!(format!("{err}").contains("column `id`"), "got: {err}");
}

#[test]
fn non_integer_epoch_rejected() {
    let row = vec![
        json!("grant-1"),
        json!("content"),
        json!("space"),
        json!("sidecar"),
        json!("nope"),
        json!("ts"),
    ];
    let err = row_to_grant(row).expect_err("non-integer epoch must be rejected");
    assert!(format!("{err}").contains("epoch"), "got: {err}");
}
