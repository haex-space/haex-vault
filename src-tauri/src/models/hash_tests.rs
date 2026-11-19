use std::io::Write;

use super::{is_valid_sha256_hex, sha256_file};

#[test]
fn hashes_known_content_to_the_expected_digest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("model.gguf");
    std::fs::File::create(&path)
        .expect("create")
        .write_all(b"hello world")
        .expect("write");

    let digest = sha256_file(&path).expect("hash");
    // sha256("hello world"), a well-known test vector.
    assert_eq!(
        digest,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

#[test]
fn hashes_content_spanning_multiple_read_buffers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("model.gguf");
    // Larger than the 1 MiB read buffer so the loop runs more than once.
    let content = vec![0xAB_u8; 3 * 1024 * 1024 + 17];
    std::fs::write(&path, &content).expect("write");

    let via_helper = sha256_file(&path).expect("hash");

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let expected: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    assert_eq!(via_helper, expected);
}

#[test]
fn missing_file_is_an_io_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("does-not-exist.gguf");
    let err = sha256_file(&missing).expect_err("missing file must error");
    assert!(matches!(err, crate::error::HaexError::Io { .. }));
}

#[test]
fn validates_sha256_hex_shape() {
    assert!(is_valid_sha256_hex(
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    ));
    // Too short.
    assert!(!is_valid_sha256_hex("abc"));
    // Uppercase is rejected — the stored form is always lowercase.
    assert!(!is_valid_sha256_hex(
        "B94D27B9934D3E08A52E52D7DA7DACEFBD0B9C15FA30C6C15D8ECB37A89F4A8"
    ));
    // Non-hex characters.
    assert!(!is_valid_sha256_hex(
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
    ));
}
