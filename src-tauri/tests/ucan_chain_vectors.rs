//! Integration-scope driver for the cross-language UCAN chain fixture.
//!
//! Consumes `tests/fixtures/ucan_chain_vectors.json` and runs each vector
//! through the crate's **public** [`haex_vault::ucan::validate_token`] API.
//! The unit-level counterpart in `src/ucan/verify/chain_tests.rs` catches
//! logic bugs; this file catches a distinct regression class: any change to
//! `pub` visibility on the UCAN types or to the shape of `ValidatedUcan`
//! that would silently break out-of-crate consumers.

use haex_vault_lib::ucan::{validate_token, CapabilityLevel, UcanVerifyError, ValidatedUcan};
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

fn required_capability(v: &Vector) -> CapabilityLevel {
    CapabilityLevel::from_capability_string(&v.capability_needed).unwrap_or_else(|| {
        panic!(
            "unknown capability_needed in vector {}: {}",
            v.name, v.capability_needed
        )
    })
}

fn run(v: &Vector) -> Result<ValidatedUcan, UcanVerifyError> {
    let leaf = &v
        .chain
        .last()
        .unwrap_or_else(|| panic!("vector {} has empty chain", v.name))
        .signed_token;
    validate_token(
        leaf,
        &v.space_id,
        &v.expected_audience,
        required_capability(v),
        MAX_CHAIN_DEPTH_FOR_TESTS,
    )
}

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

/// Parametrised driver — one `#[test]` iterates every fixture vector and
/// panics with the vector name on the first mismatch. Complements the
/// per-vector unit tests inside the crate (which give sharper failure
/// signals) with a single API-surface smoke.
#[test]
fn all_vectors_match_expected_outcome() {
    let vectors = load_vectors().vectors;
    assert!(
        !vectors.is_empty(),
        "fixture must not be empty — regenerate via `pnpm run gen:ucan-vectors`"
    );

    let mut failures: Vec<String> = Vec::new();
    for v in &vectors {
        let result = run(v);
        match (&v.expected, result) {
            (
                ExpectedOutcome::Ok {
                    resolved_root_did, ..
                },
                Ok(vu),
            ) => {
                if &vu.root_did != resolved_root_did {
                    failures.push(format!(
                        "{}: expected root_did={} got {}",
                        v.name, resolved_root_did, vu.root_did
                    ));
                }
            }
            (
                ExpectedOutcome::Ok {
                    resolved_root_did, ..
                },
                Err(e),
            ) => {
                failures.push(format!(
                    "{}: expected Ok(root={}), got Err({:?})",
                    v.name, resolved_root_did, e
                ));
            }
            (ExpectedOutcome::Err { error, .. }, Ok(vu)) => {
                failures.push(format!(
                    "{}: expected Err({}), got Ok(root={})",
                    v.name, error, vu.root_did
                ));
            }
            (ExpectedOutcome::Err { error, .. }, Err(actual)) => {
                let got = variant_name(&actual);
                if got != error.as_str() {
                    failures.push(format!(
                        "{}: expected Err({}), got Err({}) — full: {}",
                        v.name, error, got, actual
                    ));
                }
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
