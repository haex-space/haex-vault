// src-tauri/src/database/core_max_tx_size_tests.rs
//
// Unit tests for the per-call CRDT write-size guard at `execute_with_crdt`
// (ADR 0001 — max CRDT transaction size). All tests drive the PURE helper
// `write_payload_too_large` with a TINY injected limit so we never have to
// allocate 100 MB to exercise the over-limit path.

use super::*;
use serde_json::json;

#[test]
fn oversized_payload_is_flagged() {
    // A single string param well over the tiny limit must be flagged.
    let big = "x".repeat(1_000);
    let params = vec![json!(big)];
    let limit = 100;

    let result = write_payload_too_large(&params, limit);
    match result {
        Some(bytes) => assert!(
            bytes > limit,
            "reported size {bytes} should exceed the limit {limit}"
        ),
        None => panic!("oversized payload should be flagged"),
    }
}

#[test]
fn under_limit_payload_passes() {
    let params = vec![json!("small"), json!(42)];
    // Generous limit relative to the tiny payload.
    assert_eq!(write_payload_too_large(&params, 10_000), None);
}

#[test]
fn multi_row_insert_params_sum_over_limit() {
    // Several rows' worth of params (as one multi-row INSERT would flatten
    // into a single `params` vec) whose combined size crosses a small limit.
    let chunk = "y".repeat(50);
    let params: Vec<serde_json::Value> = (0..10).map(|_| json!(chunk)).collect();
    let limit = 200; // 10 * ~52 bytes serialized > 200

    assert!(
        write_payload_too_large(&params, limit).is_some(),
        "summed multi-row params should exceed the limit"
    );
}

#[test]
fn empty_params_pass() {
    let params: Vec<serde_json::Value> = vec![];
    assert_eq!(write_payload_too_large(&params, 100), None);
}
