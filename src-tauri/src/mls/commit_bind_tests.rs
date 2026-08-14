use ed25519_dalek::SigningKey;

use super::{sign_commit_bind, verify_commit_bind, verify_commit_bind_bytes};
use crate::ucan::did_key_from_public_key;

fn fresh_identity() -> SigningKey {
    SigningKey::from_bytes(&rand::random::<[u8; 32]>())
}

#[test]
fn commit_bind_roundtrips() {
    let id = fresh_identity();
    let commit_bytes = b"pretend-mls-commit-bytes";
    let sig = sign_commit_bind(&id, commit_bytes);
    verify_commit_bind(&id.verifying_key(), commit_bytes, &sig)
        .expect("verify must accept a signature over the exact commit it was made from");
}

#[test]
fn commit_bind_rejects_different_commit() {
    let id = fresh_identity();
    let sig = sign_commit_bind(&id, b"commit-A");
    verify_commit_bind(&id.verifying_key(), b"commit-B", &sig)
        .expect_err("a signature made over commit A must not verify against commit B (replay)");
}

#[test]
fn commit_bind_rejects_wrong_identity() {
    let signer = fresh_identity();
    let commit_bytes = b"pretend-mls-commit-bytes";
    let sig = sign_commit_bind(&signer, commit_bytes);
    let other = fresh_identity();
    verify_commit_bind(&other.verifying_key(), commit_bytes, &sig)
        .expect_err("a signature must not verify against an unrelated identity key");
}

#[test]
fn verify_commit_bind_bytes_roundtrips_via_did() {
    let id = fresh_identity();
    let did = did_key_from_public_key(&id.verifying_key());
    let commit_bytes = b"pretend-mls-commit-bytes";
    let sig = sign_commit_bind(&id, commit_bytes);
    verify_commit_bind_bytes(&did, commit_bytes, &sig.to_bytes())
        .expect("byte-level helper must resolve the DID and verify successfully");
}

#[test]
fn verify_commit_bind_bytes_rejects_malformed_signature() {
    let id = fresh_identity();
    let did = did_key_from_public_key(&id.verifying_key());
    verify_commit_bind_bytes(&did, b"commit", &[1, 2, 3])
        .expect_err("a non-64-byte signature must be rejected as malformed");
}
