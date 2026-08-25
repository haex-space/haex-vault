//! Chunk-level XChaCha20Poly1305 seal/open and the plaintext↔ciphertext size
//! arithmetic that lets the diff engine (Round D) reason about a remote
//! ciphertext file's plaintext size without downloading it.
//!
//! ## Chunk-nonce derivation
//!
//! Given the 24-byte `file_nonce` from the envelope header and a `chunk_index`
//! (0-based), the per-chunk nonce is:
//!
//! ```text
//! chunk_nonce = file_nonce
//! chunk_nonce[16..24] ^= chunk_index.to_le_bytes()
//! ```
//!
//! I.e. the first 16 bytes of the nonce are always the file's random prefix;
//! the last 8 bytes carry the chunk counter XORed into whatever random bytes
//! were there. That leaves 128 bits of per-file entropy plus a 64-bit counter,
//! keeping the accidental-collision bound well above the birthday floor even
//! at absurd file counts, and guaranteeing uniqueness across every chunk of a
//! single file.
//!
//! ## Size arithmetic
//!
//! Encrypted objects are `header + Σ chunks`, where every chunk contributes
//! its plaintext bytes plus a 16-byte AEAD tag. A file of zero bytes has zero
//! chunks and hence a header-only object (37 bytes); otherwise the last chunk
//! carries 1..=CHUNK_PLAINTEXT_SIZE plaintext bytes. This gives an exact
//! bijection between plaintext length and ciphertext length, verified by the
//! module tests over a wide range.
//!
//! The inverse function ([`plaintext_len`]) is load-bearing for the diff
//! engine: `CloudProvider::manifest()` needs to report plaintext sizes so that
//! `diff.rs::files_equal` (which falls back to `size + modified_at` for cloud
//! targets) does not turn every already-uploaded file into a silent
//! re-upload.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    Key, XChaCha20Poly1305, XNonce,
};

use crate::file_sync::crypto::envelope::{HEADER_SIZE, NONCE_SIZE};
use crate::file_sync::hashing::CHUNK_HASH_SIZE;

/// `Key::from_slice` and `XNonce::from_slice` in `chacha20poly1305 = "0.11"`
/// are marked deprecated in favour of `TryFrom`, but the replacement path
/// forces a fallible construction for values we've already sized correctly at
/// the type level. Localising the `allow` here keeps the fallout narrow.
#[allow(deprecated)]
fn xkey(key: &[u8; 32]) -> &Key {
    Key::from_slice(key)
}

#[allow(deprecated)]
fn xnonce(bytes: &[u8; NONCE_SIZE]) -> &XNonce {
    XNonce::from_slice(bytes)
}

/// Plaintext bytes per full chunk (1 MiB), aligned with
/// [`crate::file_sync::hashing::CHUNK_HASH_SIZE`] so encrypted-file BLAKE3
/// chunks can later line up with plaintext-file BLAKE3 chunks without a
/// separate hash pass.
pub const CHUNK_PLAINTEXT_SIZE: usize = CHUNK_HASH_SIZE as usize;

/// AEAD tag size for XChaCha20Poly1305 (Poly1305 output).
pub const TAG_SIZE: usize = 16;

/// Ciphertext bytes per full chunk: plaintext + tag.
pub const CHUNK_CIPHERTEXT_SIZE: usize = CHUNK_PLAINTEXT_SIZE + TAG_SIZE;

/// Errors surfaced by the envelope + chunk primitives. Kept flat and
/// non-`Copy` so callers can attach context strings without wrapping.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("header too short: need at least {HEADER_SIZE} bytes")]
    HeaderTooShort,
    #[error("bad magic: not a haex file envelope")]
    BadMagic,
    #[error("unsupported envelope version: {0}")]
    UnsupportedVersion(u8),
    #[error("plaintext chunk too large: {got} > max {}", CHUNK_PLAINTEXT_SIZE)]
    ChunkTooLarge { got: usize },
    #[error("ciphertext chunk too short: {got} <= tag size {}", TAG_SIZE)]
    CiphertextTooShort { got: usize },
    #[error("aead seal failed")]
    SealFailed,
    #[error("aead open failed (integrity check)")]
    OpenFailed,
    #[error("buffer too small for envelope header")]
    BufferTooSmall,
    #[error("malformed ciphertext length: {0}")]
    MalformedCiphertext(String),
}

/// Derive the per-chunk nonce. See module docs for the construction.
pub fn chunk_nonce(file_nonce: &[u8; NONCE_SIZE], chunk_index: u64) -> [u8; NONCE_SIZE] {
    let mut nonce = *file_nonce;
    let counter = chunk_index.to_le_bytes();
    for i in 0..8 {
        nonce[16 + i] ^= counter[i];
    }
    nonce
}

/// AEAD-seal a single chunk. `plaintext.len()` must be `<= CHUNK_PLAINTEXT_SIZE`;
/// a length of exactly zero is legal for callers that want to represent an
/// "empty final chunk" (the file-level size arithmetic never asks for one, but
/// the primitive stays honest).
pub fn seal_chunk(
    key: &[u8; 32],
    file_nonce: &[u8; NONCE_SIZE],
    chunk_index: u64,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if plaintext.len() > CHUNK_PLAINTEXT_SIZE {
        return Err(CryptoError::ChunkTooLarge {
            got: plaintext.len(),
        });
    }
    let cipher = XChaCha20Poly1305::new(xkey(key));
    let nonce_bytes = chunk_nonce(file_nonce, chunk_index);
    let nonce = xnonce(&nonce_bytes);
    cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| CryptoError::SealFailed)
}

/// AEAD-open a single chunk. Fails if the ciphertext is shorter than the tag
/// or if the tag check fails.
pub fn open_chunk(
    key: &[u8; 32],
    file_nonce: &[u8; NONCE_SIZE],
    chunk_index: u64,
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if ciphertext.len() < TAG_SIZE {
        return Err(CryptoError::CiphertextTooShort {
            got: ciphertext.len(),
        });
    }
    let cipher = XChaCha20Poly1305::new(xkey(key));
    let nonce_bytes = chunk_nonce(file_nonce, chunk_index);
    let nonce = xnonce(&nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::OpenFailed)
}

/// Number of chunks a plaintext of the given length is split into. Zero-length
/// plaintexts have zero chunks (see module docs).
pub fn num_chunks(plaintext_len: u64) -> u64 {
    plaintext_len.div_ceil(CHUNK_PLAINTEXT_SIZE as u64)
}

/// Ciphertext-object size for a plaintext of `plaintext_len` bytes. Includes
/// the envelope header and every per-chunk AEAD tag.
pub fn ciphertext_len(plaintext_len: u64) -> u64 {
    let chunks = num_chunks(plaintext_len);
    HEADER_SIZE as u64 + plaintext_len + chunks * TAG_SIZE as u64
}

/// Exact inverse of [`ciphertext_len`]: given the on-disk ciphertext size,
/// recover the plaintext size. Returns [`CryptoError::MalformedCiphertext`] on
/// impossible sizes (shorter than a header, or a body remainder that would
/// imply a zero-plaintext final chunk).
///
/// The diff engine calls this when the storage provider reports only the
/// ciphertext size; a wrong answer here silently re-uploads every file on
/// every sync (see plan §"Warum der Diff nicht so bricht, wie man erwartet").
pub fn plaintext_len(ciphertext_len: u64) -> Result<u64, CryptoError> {
    if ciphertext_len < HEADER_SIZE as u64 {
        return Err(CryptoError::MalformedCiphertext(format!(
            "ciphertext {ciphertext_len} < header {HEADER_SIZE}"
        )));
    }
    let body = ciphertext_len - HEADER_SIZE as u64;
    if body == 0 {
        return Ok(0);
    }
    let ct_chunk = CHUNK_CIPHERTEXT_SIZE as u64;
    let full = body / ct_chunk;
    let rem = body % ct_chunk;
    if rem == 0 {
        Ok(full * CHUNK_PLAINTEXT_SIZE as u64)
    } else if rem > TAG_SIZE as u64 {
        Ok(full * CHUNK_PLAINTEXT_SIZE as u64 + (rem - TAG_SIZE as u64))
    } else {
        Err(CryptoError::MalformedCiphertext(format!(
            "body remainder {rem} <= tag size {TAG_SIZE}"
        )))
    }
}
