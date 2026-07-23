//! Cross-language tests for [`super`] — the fixture in
//! `src-tauri/tests/fixtures/space_id_vectors.json` is the shared source of
//! truth with the TS implementation (`src/utils/auth/spaceId.ts`). If a vector
//! disagrees, the *implementation* is wrong, not the fixture.

use super::{derive_space_id, verify_space_id_binding, NONCE_LEN};
use serde_json::Value;

const FIXTURE_JSON: &str = include_str!("../../../tests/fixtures/space_id_vectors.json");

const ROOT_DID_A: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const ROOT_DID_B: &str = "did:key:z6MkuFCe3s5eAo3iiVjxkr4Y17H2Uu55T8yg9zC6cnyfyGkK";

fn parse_nonce(hex_str: &str) -> [u8; NONCE_LEN] {
    let bytes = hex::decode(hex_str).expect("fixture nonce_hex is valid hex");
    assert_eq!(bytes.len(), NONCE_LEN, "fixture nonce must be 16 bytes");
    let mut out = [0u8; NONCE_LEN];
    out.copy_from_slice(&bytes);
    out
}

#[test]
fn matches_fixture_vectors() {
    let doc: Value = serde_json::from_str(FIXTURE_JSON).expect("fixture parses as JSON");
    assert_eq!(
        doc["domain_tag"].as_str(),
        Some(super::DOMAIN_TAG),
        "domain_tag drift between fixture and Rust impl",
    );
    let vectors = doc["vectors"]
        .as_array()
        .expect("fixture has vectors array");
    assert!(
        !vectors.is_empty(),
        "fixture must contain at least one vector"
    );

    for v in vectors {
        let name = v["name"].as_str().unwrap_or("<unnamed>");
        let root_did = v["root_did"].as_str().expect("vector has root_did");
        let nonce = parse_nonce(v["nonce_hex"].as_str().expect("vector has nonce_hex"));
        let expected = v["expected_space_id"]
            .as_str()
            .expect("vector has expected_space_id");

        let derived = derive_space_id(root_did, &nonce);
        assert_eq!(
            derived, expected,
            "space_id mismatch for vector {name}: Rust impl diverges from TS fixture",
        );
        assert!(
            verify_space_id_binding(&derived, root_did),
            "self-verification failed for vector {name}",
        );
    }
}

#[test]
fn verifies_matching_root() {
    let nonce: [u8; NONCE_LEN] = rand::random();
    let space_id = derive_space_id(ROOT_DID_A, &nonce);
    assert!(verify_space_id_binding(&space_id, ROOT_DID_A));
}

#[test]
fn rejects_unrelated_root() {
    let nonce: [u8; NONCE_LEN] = rand::random();
    let space_id = derive_space_id(ROOT_DID_A, &nonce);
    assert!(!verify_space_id_binding(&space_id, ROOT_DID_B));
}

#[test]
fn rejects_tampered_hash_byte() {
    let nonce: [u8; NONCE_LEN] = rand::random();
    let space_id = derive_space_id(ROOT_DID_A, &nonce);
    let mut bytes = bs58::decode(&space_id)
        .into_vec()
        .expect("derived space_id decodes");
    // Flip a bit in the hash tail (offset >= NONCE_LEN) so nonce stays intact.
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    let tampered = bs58::encode(bytes).into_string();
    assert!(!verify_space_id_binding(&tampered, ROOT_DID_A));
}

#[test]
fn rejects_malformed_input() {
    // Empty string.
    assert!(!verify_space_id_binding("", ROOT_DID_A));
    // Non-base58 character (`0` is not in the Bitcoin alphabet).
    assert!(!verify_space_id_binding("0000invalid0000", ROOT_DID_A));
    // Valid base58 but wrong length (all-`1` decodes to zero bytes).
    assert!(!verify_space_id_binding("11", ROOT_DID_A));
    // Base58 of a 31-byte all-zero buffer — decodes but SPACE_ID_BYTES_LEN mismatch.
    let short = bs58::encode(vec![0u8; super::SPACE_ID_BYTES_LEN - 1]).into_string();
    assert!(!verify_space_id_binding(&short, ROOT_DID_A));
    // Base58 of a 33-byte buffer — decodes but too long.
    let long = bs58::encode(vec![0u8; super::SPACE_ID_BYTES_LEN + 1]).into_string();
    assert!(!verify_space_id_binding(&long, ROOT_DID_A));
}
