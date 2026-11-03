//! `HXCF` v1 (spec 072 T057): round trips across the segment edges, and every way of damaging a
//! file fails: cut off, segments swapped, a byte flipped, the last flag moved, another plaintext.

use super::*;

fn key() -> FileKey {
    FileKey {
        key: [7; 32],
        nonce_prefix: [9; 8],
    }
}

fn bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 % 251) as u8).collect()
}

#[test]
fn files_of_every_size_round_trip_and_have_the_announced_length() {
    for len in [0, 1, SEGMENT, SEGMENT + 1, 10 * SEGMENT] {
        let plain = bytes(len);
        let sealed = encrypt(&key(), &plain).expect("encrypt");
        assert_eq!(sealed.len() as u64, ciphertext_len(len as u64), "{len}");
        let opened = decrypt(&key().key, &sealed, &sha256_hex(&plain)).expect("decrypt");
        assert_eq!(opened, plain, "{len}");
    }
}

#[test]
fn bytes_arriving_in_small_pieces_decrypt_the_same() {
    let plain = bytes(2 * SEGMENT + 77);
    let sealed = encrypt(&key(), &plain).expect("encrypt");
    let mut decryptor = Decryptor::new(&key().key, &sha256_hex(&plain));
    let mut opened = Vec::new();
    for piece in sealed.chunks(4093) {
        opened.extend(decryptor.push(piece).expect("push"));
    }
    opened.extend(decryptor.finish().expect("finish"));
    assert_eq!(opened, plain);
}

#[test]
fn a_cut_off_file_fails() {
    let plain = bytes(3 * SEGMENT);
    let sealed = encrypt(&key(), &plain).expect("encrypt");
    let hash = sha256_hex(&plain);
    // Without the last segment: the one before it is no last segment.
    let cut = &sealed[..HEADER_LEN + 2 * (SEGMENT + TAG_LEN)];
    assert!(decrypt(&key().key, cut, &hash).is_err());
    assert!(decrypt(&key().key, &sealed[..HEADER_LEN], &hash).is_err());
    assert!(decrypt(&key().key, &sealed[..5], &hash).is_err());
}

#[test]
fn swapped_segments_fail() {
    let plain = bytes(3 * SEGMENT);
    let sealed = encrypt(&key(), &plain).expect("encrypt");
    let size = SEGMENT + TAG_LEN;
    let first = HEADER_LEN..HEADER_LEN + size;
    let second = HEADER_LEN + size..HEADER_LEN + 2 * size;
    let mut swapped = sealed[..HEADER_LEN].to_vec();
    swapped.extend_from_slice(&sealed[second]);
    swapped.extend_from_slice(&sealed[first]);
    swapped.extend_from_slice(&sealed[HEADER_LEN + 2 * size..]);
    assert!(decrypt(&key().key, &swapped, &sha256_hex(&plain)).is_err());
}

#[test]
fn a_flipped_byte_fails_anywhere() {
    let plain = bytes(SEGMENT + 10);
    let sealed = encrypt(&key(), &plain).expect("encrypt");
    let hash = sha256_hex(&plain);
    for at in [
        0,
        4,
        6,
        12,
        HEADER_LEN,
        HEADER_LEN + SEGMENT,
        sealed.len() - 1,
    ] {
        let mut flipped = sealed.clone();
        flipped[at] ^= 1;
        assert!(decrypt(&key().key, &flipped, &hash).is_err(), "byte {at}");
    }
}

#[test]
fn a_last_segment_sealed_as_not_last_fails() {
    // A one-segment file whose segment is sealed without the last flag: valid GCM, wrong AAD.
    let plain = bytes(100);
    let file = key();
    let mut forged = encrypt(&file, &[]).expect("header")[..HEADER_LEN].to_vec();
    let sealed = file
        .cipher()
        .encrypt(
            &Nonce::from(nonce(&file.nonce_prefix, 0)),
            Payload {
                msg: &plain,
                aad: &aad(false),
            },
        )
        .expect("seal");
    forged.extend_from_slice(&sealed);
    assert!(decrypt(&file.key, &forged, &sha256_hex(&plain)).is_err());
}

#[test]
fn another_plaintext_hash_or_another_key_fails() {
    let plain = bytes(1000);
    let sealed = encrypt(&key(), &plain).expect("encrypt");
    assert!(decrypt(&key().key, &sealed, &sha256_hex(b"other")).is_err());
    let other = FileKey {
        key: [8; 32],
        nonce_prefix: [9; 8],
    };
    assert!(decrypt(&other.key, &sealed, &sha256_hex(&plain)).is_err());
}

#[test]
fn generated_keys_differ_and_lengths_follow_the_contract() {
    let a = FileKey::generate().expect("key");
    let b = FileKey::generate().expect("key");
    assert_ne!(a, b);
    assert_eq!(format!("{a:?}"), "FileKey(..)", "the key is never printed");
    assert_eq!(ciphertext_len(0), (HEADER_LEN + TAG_LEN) as u64);
    assert_eq!(
        ciphertext_len(MAX_PLAINTEXT),
        MAX_PLAINTEXT + HEADER_LEN as u64 + TAG_LEN as u64 * MAX_PLAINTEXT.div_ceil(SEGMENT as u64)
    );
}

#[test]
fn decryptor_rejects_ciphertext_over_the_plaintext_limit() {
    let mut decryptor = Decryptor::new(&key().key, &sha256_hex(&[]));
    decryptor.received = MAX_CIPHERTEXT;
    assert!(decryptor.push(b"x").is_err());
    assert!(decryptor.buffer.is_empty());
}

#[test]
fn ciphertext_length_does_not_overflow_for_untrusted_sizes() {
    assert_eq!(ciphertext_len(u64::MAX), u64::MAX);
}
