//! Integration-scope driver for the cross-language column-sig fixture
//! (Phase 1, Task I2). Consumes `tests/fixtures/column_sig_vectors.json`
//! and calls `verify_column_sig` through the crate's **public** API. The
//! unit-level counterpart lives in `src/crdt/column_sig/verify_tests.rs`;
//! this file catches a distinct regression class:
//!
//!   1. `pub` visibility drift on `verify_column_sig` /
//!      `VerifyColumnSigError` that would silently break out-of-crate
//!      consumers.
//!   2. TS ↔ Rust canonicalisation drift — every valid vector's
//!      `valueBytes` field was produced by the TS generator; if Rust's
//!      preimage layout or byte encoding drifts, the sig no longer
//!      verifies and the test fails with the vector's name.
//!
//! Regenerate the fixture via `pnpm run gen:column-sig-vectors`.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use haex_vault_lib::crdt::column_sig::verify::{verify_column_sig, VerifyColumnSigError};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct Fixture {
    vectors: Vec<Vector>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vector {
    name: String,
    space_id: String,
    table_name: String,
    row_pks: String,
    column_name: String,
    hlc: String,
    author_did: String,
    value_bytes: String,
    sig: String,
    expected: String,
}

fn load_fixture() -> Fixture {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/column_sig_vectors.json");
    let raw =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read fixture {path:?}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse fixture {path:?}: {e}"))
}

fn decode(v: &Vector, field: &str, s: &str) -> Vec<u8> {
    BASE64
        .decode(s)
        .unwrap_or_else(|e| panic!("{}: base64-decode {field}: {e}", v.name))
}

fn run(v: &Vector) -> Result<(), VerifyColumnSigError> {
    let value_bytes = decode(v, "valueBytes", &v.value_bytes);
    let sig_bytes = decode(v, "sig", &v.sig);
    verify_column_sig(
        v.space_id.as_bytes(),
        v.table_name.as_bytes(),
        v.row_pks.as_bytes(),
        v.column_name.as_bytes(),
        v.hlc.as_bytes(),
        &v.author_did,
        &value_bytes,
        &sig_bytes,
    )
}

/// The three `verify_rejected_*` scenarios all collapse to
/// `VerifyColumnSigError::InvalidSignature` in the current verifier
/// implementation — the field that was perturbed is documented in the
/// vector's `name` and `expected` tag for readability, but at the crypto
/// layer they all fail the same Ed25519 check. Should a future variant
/// differentiate them (e.g. an explicit space-mismatch pre-check), this
/// match becomes richer; for now, one shared reject arm is honest and
/// avoids duplicating the same assertion three times.
fn is_expected_reject_variant(err: &VerifyColumnSigError) -> bool {
    matches!(err, VerifyColumnSigError::InvalidSignature)
}

/// Single parametrised driver so a failure surfaces the vector name at the
/// top of the panic message rather than being buried in per-test output.
#[test]
fn all_column_sig_vectors_match_expected() {
    let vectors = load_fixture().vectors;
    assert!(
        !vectors.is_empty(),
        "fixture must not be empty — regenerate via `pnpm run gen:column-sig-vectors`"
    );

    let mut failures: Vec<String> = Vec::new();
    for v in &vectors {
        let result = run(v);
        match (v.expected.as_str(), &result) {
            ("verify_ok", Ok(())) => {}
            ("verify_ok", Err(e)) => {
                failures.push(format!("{}: expected verify_ok, got Err({:?})", v.name, e));
            }
            (
                "verify_rejected_sig" | "verify_rejected_wrong_space" | "verify_rejected_wrong_did",
                Ok(()),
            ) => {
                failures.push(format!(
                    "{}: expected {} but sig unexpectedly verified",
                    v.name, v.expected
                ));
            }
            (
                "verify_rejected_sig" | "verify_rejected_wrong_space" | "verify_rejected_wrong_did",
                Err(e),
            ) => {
                if !is_expected_reject_variant(e) {
                    failures.push(format!(
                        "{}: expected InvalidSignature (via {}), got {:?}",
                        v.name, v.expected, e
                    ));
                }
            }
            (other, _) => {
                failures.push(format!("{}: unknown expected tag {other:?}", v.name));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "{}/{} fixture vectors mismatched:\n{}",
            failures.len(),
            vectors.len(),
            failures.join("\n")
        );
    }
}

/// Guards against a subtle multi-space regression: if a future refactor
/// dropped `space_id` (or `author_did`) from the preimage's length-prefixed
/// input, two vectors that only differ in those fields would produce the
/// SAME signature. The generator already asserts sig inequality at gen
/// time; this test reasserts it after the fixture has round-tripped
/// through JSON so a stale checked-in file cannot hide the drift either.
#[test]
fn multi_space_vectors_have_distinct_sigs() {
    let vectors = load_fixture().vectors;
    let a = vectors
        .iter()
        .find(|v| v.name == "multi_space_primary_valid")
        .expect("multi_space_primary_valid vector must be present");
    let b = vectors
        .iter()
        .find(|v| v.name == "multi_space_secondary_valid")
        .expect("multi_space_secondary_valid vector must be present");
    assert_ne!(
        a.sig, b.sig,
        "multi-space vectors must not share a sig — space_id/author_did missing from preimage?"
    );
    assert_ne!(
        a.space_id, b.space_id,
        "multi-space vectors must declare distinct space_ids"
    );
    assert_ne!(
        a.author_did, b.author_did,
        "multi-space vectors must declare distinct author DIDs"
    );
}
