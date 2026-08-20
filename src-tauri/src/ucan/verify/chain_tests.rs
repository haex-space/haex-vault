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
use crate::ucan::capability_set::{cap_from_str, Cap};
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

fn required_capability(v: &Vector) -> Cap {
    // Fixture emits bare cap names (`"read"`, `"write"`, `"invite"`,
    // `"admin"`) since Task 7's regeneration; `cap_from_str` also
    // tolerates a legacy `"space/"` prefix so a partial regen never
    // silently drops these vectors.
    cap_from_str(&v.capability_needed).unwrap_or_else(|_| {
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
        UcanVerifyError::DelegationMissing { .. } => "DelegationMissing",
        UcanVerifyError::DelegationNotDelegatable { .. } => "DelegationNotDelegatable",
        UcanVerifyError::RowCapAttenuation { .. } => "RowCapAttenuation",
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
// Attenuation invariants (orthogonal delegation)
// ---------------------------------------------------------------------------

/// Parent holds only `write`; leaf claims `admin`. Under the orthogonal
/// model this is `DelegationMissing` (the previous hierarchical model
/// classified the same shape as `CapabilityEscalation`).
#[test]
fn delegation_missing_admin_child_from_write_parent() {
    let v = find_vector("delegation_missing_admin_child_from_write_parent");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

/// Parent holds `write` but with `delegatable=false`; leaf claims `write`.
/// `DelegationNotDelegatable` — parent may exercise the cap, not pass it on.
#[test]
fn delegation_not_delegatable_write_under_non_delegatable_parent() {
    let v = find_vector("delegation_not_delegatable_write_under_non_delegatable_parent");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

/// Parent holds `write`, leaf claims `read`. Orthogonally the two are
/// unrelated so `read` is Missing — the hierarchical model would have
/// accepted this as `write ⊇ read`.
#[test]
fn orthogonal_missing_cap_read_child_under_write_parent() {
    let v = find_vector("orthogonal_missing_cap_read_child_under_write_parent");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

/// Parent holds **only** `admin` (delegatable); leaf claims `write`.
///
/// Every other rejecting vector narrows the parent to `write`, so a
/// regression that let `admin` imply the other three caps would leave them
/// all green. This one and its `read` sibling are the vectors that bite:
/// under such an implication both flip from `DelegationMissing` to `Ok`.
#[test]
fn orthogonal_admin_only_parent_cannot_delegate_write() {
    let v = find_vector("orthogonal_admin_only_parent_cannot_delegate_write");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

/// Parent holds **only** `admin` (delegatable); leaf claims `read`. Sibling
/// of the `write` vector above — `admin` implies nothing, not even `read`.
#[test]
fn orthogonal_admin_only_parent_cannot_delegate_read() {
    let v = find_vector("orthogonal_admin_only_parent_cannot_delegate_read");
    let r = run(&v);
    assert_matches_expected(&v, r);
}

/// The D2 role presets, end to end across the language boundary:
/// `owner` root → `admin` preset → `writer` preset, all `Ok`.
///
/// Every set in this chain is generated from a preset row, so if the
/// TypeScript table (`capsFromSingle` / `rolePreset` in
/// `scripts/gen-ucan-chain-vectors.ts`) and the Rust mirror
/// ([`crate::ucan::capability_set::CapabilitySet::role_preset`]) ever
/// disagree on a `delegatable` bit, the chain stops attenuating and this
/// test fails. It also pins that a *delegated* admin really can hand out a
/// writer — the bug the old `read(false) admin(true)` set caused.
#[test]
fn d2_preset_chain_owner_admin_writer() {
    let v = find_vector("d2_preset_chain_owner_admin_writer");
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

// ---------------------------------------------------------------------------
// Wire-shape invariants (cross-language `CapEntry` contract)
// ---------------------------------------------------------------------------

/// Leaf cap entry omits `delegatable`; everything else about the chain is
/// valid. `CapEntry::delegatable` carries no `#[serde(default)]`, so the
/// payload is rejected as `MalformedToken` rather than read as
/// `delegatable: false`.
///
/// This is the vector that pins the wire contract. Nothing else exercised the
/// absent-`delegatable` payload, so a lenient reader could silently accept a
/// shape the other language rejects. Restore `#[serde(default)]` on
/// `delegatable` and this test flips to `Ok` and fails — as does the
/// out-of-crate driver in `src-tauri/tests/ucan_chain_vectors.rs`.
#[test]
fn cap_entry_missing_delegatable_in_leaf() {
    let v = find_vector("cap_entry_missing_delegatable_in_leaf");
    let r = run(&v);
    assert_matches_expected(&v, r);
}
