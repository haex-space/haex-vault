use super::preimage::build_preimage;
use super::sign::sign_column;
use ed25519_dalek::{SigningKey, Verifier};

fn random_key() -> SigningKey {
    let seed: [u8; 32] = rand::random();
    SigningKey::from_bytes(&seed)
}

#[test]
fn sign_column_produces_valid_signature() {
    let key = random_key();
    let sig = sign_column(
        &key,
        b"space_A",
        b"tbl",
        b"pk",
        b"col",
        b"hlc-1",
        b"did:key:z6M...",
        b"val_bytes",
    );
    let preimage = build_preimage(
        b"space_A",
        b"tbl",
        b"pk",
        b"col",
        b"hlc-1",
        b"did:key:z6M...",
        b"val_bytes",
    );
    key.verifying_key().verify(&preimage, &sig).expect("verify");
}

#[test]
fn different_spaces_produce_different_sigs() {
    let key = random_key();
    let s1 = sign_column(
        &key, b"space_A", b"tbl", b"pk", b"col", b"hlc", b"did", b"val",
    );
    let s2 = sign_column(
        &key, b"space_B", b"tbl", b"pk", b"col", b"hlc", b"did", b"val",
    );
    assert_ne!(s1.to_bytes(), s2.to_bytes());
}
