//! Vector-driven `walk_prf_chain` + [`validate_token`] tests.
//!
//! Consumes the cross-language fixture at
//! `src-tauri/tests/fixtures/ucan_chain_vectors.json`, which is regenerated
//! from TS (`pnpm run gen:ucan-vectors`) and shared with the `@haex-space/ucan`
//! verifier so both languages agree on the chain-walk semantics byte-for-byte.
//!
//! Each named vector has its own `#[test]` fn so failures point straight at
//! the failing scenario rather than at a single parametrised driver. An
//! integration-scope duplicate lives at `src-tauri/tests/ucan_chain_vectors.rs`
//! — it drives every vector from outside the crate to catch privacy
//! regressions in the public API.

use super::*;
use serde::Deserialize;
use std::path::PathBuf;

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

fn required_capability(v: &Vector) -> CapabilityLevel {
    CapabilityLevel::from_capability_string(&v.capability_needed).unwrap_or_else(|| {
        panic!(
            "unknown capability_needed in vector: {}",
            v.capability_needed
        )
    })
}

fn run(v: &Vector) -> Result<ValidatedUcan, UcanVerifyError> {
    let leaf = &v
        .chain
        .last()
        .expect("chain has at least one node")
        .signed_token;
    validate_token(
        leaf,
        &v.space_id,
        &v.expected_audience,
        required_capability(v),
        MAX_CHAIN_DEPTH_FOR_TESTS,
    )
}

/// Assert the result matches the vector's expected outcome. Kept as a helper
/// so per-test scaffolding is tiny — one `#[test]` fn per vector.
fn assert_matches_expected(v: &Vector, result: Result<ValidatedUcan, UcanVerifyError>) {
    match (&v.expected, result) {
        (
            ExpectedOutcome::Ok {
                resolved_root_did, ..
            },
            Ok(vu),
        ) => {
            assert_eq!(
                &vu.root_did, resolved_root_did,
                "vector {}: resolved root_did mismatch",
                v.name
            );
        }
        (
            ExpectedOutcome::Ok {
                resolved_root_did, ..
            },
            Err(e),
        ) => {
            panic!(
                "vector {}: expected Ok (root={}), got Err({:?})",
                v.name, resolved_root_did, e
            );
        }
        (ExpectedOutcome::Err { error, .. }, Ok(vu)) => {
            panic!(
                "vector {}: expected Err({}), got Ok(root_did={})",
                v.name, error, vu.root_did
            );
        }
        (ExpectedOutcome::Err { error, .. }, Err(actual)) => {
            let actual_name = variant_name(&actual);
            assert_eq!(
                actual_name,
                error.as_str(),
                "vector {}: error variant mismatch (full error: {})",
                v.name,
                actual
            );
        }
    }
}

/// Map a [`UcanVerifyError`] value to the variant name string the fixture
/// uses. Kept explicit (not derived via serde or Debug parsing) so any new
/// variant added to the enum forces a compile-time miss here — a silent
/// mismatch between fixture strings and Rust variants would otherwise leave
/// tests looking green while checking the wrong thing.
fn variant_name(e: &UcanVerifyError) -> &'static str {
    match e {
        UcanVerifyError::MalformedToken(_) => "MalformedToken",
        UcanVerifyError::Signature => "Signature",
        UcanVerifyError::Expired => "Expired",
        UcanVerifyError::AudienceMismatch { .. } => "AudienceMismatch",
        UcanVerifyError::EmptyExpectedAudience => "EmptyExpectedAudience",
        UcanVerifyError::MissingCapability { .. } => "MissingCapability",
        UcanVerifyError::InsufficientCapability { .. } => "InsufficientCapability",
        UcanVerifyError::UnknownCapability(_) => "UnknownCapability",
        UcanVerifyError::ChainTooDeep(_) => "ChainTooDeep",
        UcanVerifyError::ChainBroken => "ChainBroken",
        UcanVerifyError::CapabilityEscalation => "CapabilityEscalation",
        UcanVerifyError::RootNotSelfSigned => "RootNotSelfSigned",
        UcanVerifyError::RootBindingMismatch => "RootBindingMismatch",
        UcanVerifyError::RootBindingMalformed => "RootBindingMalformed",
        UcanVerifyError::WrongSpace => "WrongSpace",
    }
}

// ---------------------------------------------------------------------------
// Happy paths
// ---------------------------------------------------------------------------

#[test]
fn root_only_valid() {
    let v = find_vector("root_only_valid");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

#[test]
fn two_hop_valid() {
    let v = find_vector("two_hop_valid");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

#[test]
fn three_hop_valid() {
    let v = find_vector("three_hop_valid");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

#[test]
fn five_hop_valid_at_max() {
    let v = find_vector("five_hop_valid_at_max");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

// ---------------------------------------------------------------------------
// Depth guard
// ---------------------------------------------------------------------------

#[test]
fn six_hop_exceeds_max() {
    let v = find_vector("six_hop_exceeds_max");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

// ---------------------------------------------------------------------------
// Signature invariants
// ---------------------------------------------------------------------------

#[test]
fn tampered_leaf_signature() {
    let v = find_vector("tampered_leaf_signature");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

#[test]
fn tampered_middle_signature() {
    let v = find_vector("tampered_middle_signature");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

// ---------------------------------------------------------------------------
// Attenuation invariants
// ---------------------------------------------------------------------------

#[test]
fn capability_escalation_read_to_admin() {
    let v = find_vector("capability_escalation_read_to_admin");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

// ---------------------------------------------------------------------------
// Root-binding invariants
// ---------------------------------------------------------------------------

#[test]
fn wrong_root_did_binding_mismatch() {
    let v = find_vector("wrong_root_did_binding_mismatch");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

#[test]
fn root_not_self_signed() {
    let v = find_vector("root_not_self_signed");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

// ---------------------------------------------------------------------------
// Chain continuity invariants
// ---------------------------------------------------------------------------

#[test]
fn chain_broken_aud_mismatch() {
    let v = find_vector("chain_broken_aud_mismatch");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

// ---------------------------------------------------------------------------
// Expiry invariants (leaf + ancestor)
// ---------------------------------------------------------------------------

#[test]
fn expired_leaf() {
    let v = find_vector("expired_leaf");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

#[test]
fn expired_root() {
    let v = find_vector("expired_root");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

// ---------------------------------------------------------------------------
// Space alignment invariants
// ---------------------------------------------------------------------------

#[test]
fn wrong_space_in_delegate() {
    let v = find_vector("wrong_space_in_delegate");
    let r = run(&v);
    assert_matches_expected(&v, r);
}
