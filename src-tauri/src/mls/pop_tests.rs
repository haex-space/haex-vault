use ed25519_dalek::SigningKey;

use super::{sign_pop, verify_pop};

fn fresh_identity() -> SigningKey {
    SigningKey::from_bytes(&rand::random::<[u8; 32]>())
}

#[test]
fn pop_roundtrips() {
    let id = fresh_identity();
    let mls_sig_pub: [u8; 32] = rand::random();
    let did = "did:key:zGOOD";
    let sig = sign_pop(&id, &mls_sig_pub, did);
    verify_pop(&id.verifying_key(), &mls_sig_pub, did, &sig)
        .expect("verify_pop must accept a signature over the exact inputs it was made from");
}

#[test]
fn pop_rejects_wrong_mls_key() {
    let id = fresh_identity();
    let mls_sig_pub: [u8; 32] = rand::random();
    let did = "did:key:zGOOD";
    let sig = sign_pop(&id, &mls_sig_pub, did);
    let other: [u8; 32] = rand::random();
    verify_pop(&id.verifying_key(), &other, did, &sig)
        .expect_err("verify_pop must reject a signature made over a different MLS key");
}

#[test]
fn pop_rejects_wrong_did() {
    let id = fresh_identity();
    let mls_sig_pub: [u8; 32] = rand::random();
    let sig = sign_pop(&id, &mls_sig_pub, "did:key:zGOOD");
    verify_pop(&id.verifying_key(), &mls_sig_pub, "did:key:zEVIL", &sig)
        .expect_err("verify_pop must reject a signature made over a different DID");
}

#[test]
fn pop_rejects_wrong_identity() {
    let signer = fresh_identity();
    let mls_sig_pub: [u8; 32] = rand::random();
    let did = "did:key:zGOOD";
    let sig = sign_pop(&signer, &mls_sig_pub, did);
    let other_identity = fresh_identity();
    verify_pop(&other_identity.verifying_key(), &mls_sig_pub, did, &sig)
        .expect_err("verify_pop must reject a signature verified against a different identity key");
}
