use super::sign::sign_column;
use super::verify::{verify_column_sig, VerifyColumnSigError};
use ed25519_dalek::SigningKey;

fn random_key() -> SigningKey {
    let seed: [u8; 32] = rand::random();
    SigningKey::from_bytes(&seed)
}

fn author_did_for(key: &SigningKey) -> String {
    crate::ucan::verify::did_key_from_public_key(&key.verifying_key())
}

#[test]
fn verify_accepts_valid_sig() {
    let key = random_key();
    let did = author_did_for(&key);
    let sig = sign_column(&key, b"S", b"T", b"P", b"C", b"H", did.as_bytes(), b"V");
    assert!(verify_column_sig(b"S", b"T", b"P", b"C", b"H", &did, b"V", &sig.to_bytes()).is_ok());
}

#[test]
fn verify_rejects_tampered_value() {
    let key = random_key();
    let did = author_did_for(&key);
    let sig = sign_column(&key, b"S", b"T", b"P", b"C", b"H", did.as_bytes(), b"V");
    let err = verify_column_sig(
        b"S",
        b"T",
        b"P",
        b"C",
        b"H",
        &did,
        b"TAMPERED",
        &sig.to_bytes(),
    )
    .unwrap_err();
    assert!(matches!(err, VerifyColumnSigError::InvalidSignature));
}

#[test]
fn verify_rejects_wrong_space_id() {
    let key = random_key();
    let did = author_did_for(&key);
    let sig = sign_column(&key, b"S1", b"T", b"P", b"C", b"H", did.as_bytes(), b"V");
    let err =
        verify_column_sig(b"S2", b"T", b"P", b"C", b"H", &did, b"V", &sig.to_bytes()).unwrap_err();
    assert!(matches!(err, VerifyColumnSigError::InvalidSignature));
}

#[test]
fn verify_rejects_malformed_did() {
    let bad_sig = [0u8; 64];
    let err =
        verify_column_sig(b"S", b"T", b"P", b"C", b"H", "not-a-did", b"V", &bad_sig).unwrap_err();
    assert!(matches!(err, VerifyColumnSigError::MalformedDid(_)));
}

#[test]
fn verify_rejects_value_over_limit() {
    let did = "did:key:z6MkFake";
    let bad_sig = [0u8; 64];
    let big = vec![0u8; crate::crdt::column_sig::limits::MAX_VALUE_BYTES_LEN + 1];
    let err = verify_column_sig(b"S", b"T", b"P", b"C", b"H", did, &big, &bad_sig).unwrap_err();
    assert!(matches!(
        err,
        VerifyColumnSigError::ValueBytesTooLarge { .. }
    ));
}
