//! Tests for [`super::verify_chain_batch_inner`] — the pure core of the
//! `verify_ucan_chain_batch` Tauri command.
//!
//! Fixture-driven: reuses the cross-language vectors at
//! `src-tauri/tests/fixtures/ucan_chain_vectors.json` so the command's
//! outcome vocabulary stays byte-for-byte in sync with the TS verifier
//! and with `chain_tests`. Each helper loads exactly the vector name it
//! needs and asserts a precise Ok / Rejected shape.

use serde::Deserialize;
use std::path::PathBuf;

use super::{verify_chain_batch_inner, VerifyChainRequest, VerifyChainResult, VerifyOutcome};
use crate::ucan::capability_set::Cap;
use crate::ucan::MAX_UCAN_CHAIN_DEPTH_DEFAULT;

const MAX_CHAIN_DEPTH_FOR_TESTS: usize = 5;

#[derive(Debug, Deserialize)]
struct Vectors {
    vectors: Vec<Vector>,
}

#[derive(Debug, Deserialize)]
struct Vector {
    name: String,
    space_id: String,
    expected_audience: String,
    capability_needed: String,
    chain: Vec<ChainNode>,
    expected: ExpectedOutcome,
}

#[derive(Debug, Deserialize)]
struct ChainNode {
    signed_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ExpectedOutcome {
    Ok {
        #[allow(dead_code)]
        ok: bool,
        resolved_root_did: String,
    },
    Err {
        #[allow(dead_code)]
        ok: bool,
        error: String,
    },
}

fn load_vectors() -> Vectors {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ucan_chain_vectors.json");
    let raw =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read fixture {path:?}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse fixture {path:?}: {e}"))
}

fn find_vector(name: &str) -> Vector {
    load_vectors()
        .vectors
        .into_iter()
        .find(|v| v.name == name)
        .unwrap_or_else(|| panic!("fixture vector {name} missing"))
}

fn required_capability(v: &Vector) -> Cap {
    // The fixture keeps the `space/*` wire vocabulary for backward-fixture
    // compatibility (Task 7 regenerates it fully); this local mapping keeps
    // the runner working under the new orthogonal Cap type in the interim.
    match v.capability_needed.as_str() {
        "space/read" => Cap::Read,
        "space/write" => Cap::Write,
        "space/invite" => Cap::Invite,
        "space/admin" => Cap::Admin,
        other => panic!("unknown capability_needed in vector: {other}"),
    }
}

/// Build a batch request from a fixture vector. Uses the last chain
/// entry as the leaf — same convention as `chain_tests::run`. `row_id`
/// and `table_name` are echo-back fields; give them predictable values
/// so tests can assert the response order matches request order.
fn request_from_vector(v: &Vector, row_id: &str, table_name: &str) -> VerifyChainRequest {
    let leaf = v
        .chain
        .last()
        .expect("chain has at least one node")
        .signed_token
        .clone();
    VerifyChainRequest {
        token: leaf,
        expected_space_id: v.space_id.clone(),
        expected_audience: v.expected_audience.clone(),
        capability_needed: required_capability(v),
        row_id: row_id.to_string(),
        table_name: table_name.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Single-vector shapes
// ---------------------------------------------------------------------------

#[test]
fn single_valid_leaf_returns_ok_with_root_did() {
    let v = find_vector("root_only_valid");
    let req = request_from_vector(&v, "row-1", "haex_shares");

    let results = verify_chain_batch_inner(vec![req], MAX_CHAIN_DEPTH_FOR_TESTS);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].row_id, "row-1");
    assert_eq!(results[0].table_name, "haex_shares");

    let expected_root = match &v.expected {
        ExpectedOutcome::Ok {
            resolved_root_did, ..
        } => resolved_root_did.clone(),
        ExpectedOutcome::Err { .. } => panic!("root_only_valid must be an Ok fixture"),
    };
    assert_eq!(
        results[0].outcome,
        VerifyOutcome::Ok {
            root_did: expected_root
        }
    );
}

#[test]
fn single_tampered_signature_returns_signature_rejection() {
    let v = find_vector("tampered_leaf_signature");
    let req = request_from_vector(&v, "row-1", "haex_shares");

    let results = verify_chain_batch_inner(vec![req], MAX_CHAIN_DEPTH_FOR_TESTS);
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].outcome,
        VerifyOutcome::Rejected {
            reason: "Signature".to_string()
        }
    );
}

#[test]
fn depth_exceeded_returns_chain_too_deep() {
    // `six_hop_exceeds_max` was crafted against a fixture-wide cap of 5;
    // the batch runner uses the same cap for parity with `chain_tests`.
    let v = find_vector("six_hop_exceeds_max");
    let req = request_from_vector(&v, "row-1", "haex_shares");

    let results = verify_chain_batch_inner(vec![req], MAX_CHAIN_DEPTH_FOR_TESTS);
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].outcome,
        VerifyOutcome::Rejected {
            reason: "ChainTooDeep".to_string()
        }
    );
}

#[test]
fn wrong_space_returns_wrong_space_rejection() {
    let v = find_vector("wrong_space_in_delegate");
    let req = request_from_vector(&v, "row-1", "haex_shares");

    let results = verify_chain_batch_inner(vec![req], MAX_CHAIN_DEPTH_FOR_TESTS);
    assert_eq!(results.len(), 1);
    let expected_err = match &v.expected {
        ExpectedOutcome::Err { error, .. } => error.clone(),
        ExpectedOutcome::Ok { .. } => panic!("wrong_space_in_delegate must be an Err fixture"),
    };
    assert_eq!(
        results[0].outcome,
        VerifyOutcome::Rejected {
            reason: expected_err
        }
    );
}

// ---------------------------------------------------------------------------
// Mixed batch
// ---------------------------------------------------------------------------

#[test]
fn mixed_batch_preserves_order_and_isolates_outcomes() {
    // 3 valid + 1 tampered, interleaved. Each request is annotated with
    // a distinct `row_id`; the response order must match the request
    // order and each row_id must round-trip unchanged.
    let good_a = find_vector("root_only_valid");
    let good_b = find_vector("two_hop_valid");
    let bad = find_vector("tampered_leaf_signature");
    let good_c = find_vector("three_hop_valid");

    let batch = vec![
        request_from_vector(&good_a, "row-a", "haex_shares"),
        request_from_vector(&bad, "row-b", "haex_shares"),
        request_from_vector(&good_b, "row-c", "haex_shares"),
        request_from_vector(&good_c, "row-d", "haex_shares"),
    ];

    let results = verify_chain_batch_inner(batch, MAX_CHAIN_DEPTH_FOR_TESTS);
    assert_eq!(results.len(), 4);

    assert_eq!(results[0].row_id, "row-a");
    assert!(matches!(results[0].outcome, VerifyOutcome::Ok { .. }));

    assert_eq!(results[1].row_id, "row-b");
    assert_eq!(
        results[1].outcome,
        VerifyOutcome::Rejected {
            reason: "Signature".to_string()
        }
    );

    assert_eq!(results[2].row_id, "row-c");
    assert!(matches!(results[2].outcome, VerifyOutcome::Ok { .. }));

    assert_eq!(results[3].row_id, "row-d");
    assert!(matches!(results[3].outcome, VerifyOutcome::Ok { .. }));
}

// ---------------------------------------------------------------------------
// Depth defaults
// ---------------------------------------------------------------------------

#[test]
fn default_depth_admits_five_hop_valid() {
    // Regression guard: `MAX_UCAN_CHAIN_DEPTH_DEFAULT` must remain >= 5
    // so a five-hop chain (space-root → admin → sub-admin → member →
    // device) verifies without configuration. If the default is ever
    // lowered below 5, this test flips to `ChainTooDeep` — that's the
    // signal to update this test's guarding comment, not to weaken it.
    let v = find_vector("five_hop_valid_at_max");
    let req = request_from_vector(&v, "row-1", "haex_shares");

    let results = verify_chain_batch_inner(vec![req], MAX_UCAN_CHAIN_DEPTH_DEFAULT as usize);
    assert_eq!(results.len(), 1);
    assert!(
        matches!(results[0].outcome, VerifyOutcome::Ok { .. }),
        "expected Ok at default depth, got {:?}",
        results[0].outcome
    );
}

#[test]
fn empty_batch_returns_empty_response() {
    let results: Vec<VerifyChainResult> =
        verify_chain_batch_inner(vec![], MAX_CHAIN_DEPTH_FOR_TESTS);
    assert!(results.is_empty());
}
