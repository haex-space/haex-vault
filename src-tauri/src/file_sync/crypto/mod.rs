//! File-content envelope + chunk-level AEAD for Phase 4 (cloud file sync).
//!
//! **Scope:** primitives only — no I/O, no key resolution, no provider glue.
//! Higher rounds (see `docs/plans/2026-08-25-phase4-file-content-encryption.md`)
//! layer key lookup (Round B), sidecar/object-key mapping (Round C), and
//! provider wiring (Round D) on top of these primitives.
//!
//! ## Envelope layout on disk
//!
//! ```text
//! [ magic 4 ][ version 1 ][ epoch 8 LE ][ file_nonce 24 ][ chunk_0 ][ chunk_1 ] ...
//!  \-------- 37-byte header --------/    \-- ciphertext body (chunks) --/
//! ```
//!
//! Each chunk is a single XChaCha20Poly1305 sealed message: plaintext of up to
//! [`chunk::CHUNK_PLAINTEXT_SIZE`] bytes followed by a 16-byte AEAD tag. A file
//! of zero bytes has zero chunks (body is empty); all other files have at least
//! one chunk. Chunk size is aligned with `file_sync::hashing::CHUNK_HASH_SIZE`
//! (1 MiB) so that the encrypted-file BLAKE3 chunk boundaries can later be
//! aligned with the plaintext-file chunk boundaries without a rehash.
//!
//! ## Choice of AEAD (XChaCha20Poly1305, not Aes256Gcm)
//!
//! `src/crypto/mod.rs` uses `Aes256Gcm` with a 12-byte nonce for the
//! identity-sealed key-material path, where per-message nonces are generated
//! fresh each call. Here, file nonces are generated once per file and are
//! reused (with a per-chunk counter XORed in) across all chunks of that file.
//! With only 12 bytes of nonce space (of which the last 8 bytes are consumed
//! by the chunk counter), the birthday bound for accidental collision across
//! many files would be uncomfortably close. XChaCha20Poly1305's 24-byte nonce
//! keeps 128 bits of random per-file entropy even after the counter reservation,
//! which is why the plan calls for it explicitly.

pub mod chunk;
pub mod envelope;
pub mod key_resolver;

#[cfg(test)]
mod tests;

pub use chunk::{
    ciphertext_len, num_chunks, open_chunk, plaintext_len, seal_chunk, CryptoError,
    CHUNK_CIPHERTEXT_SIZE, CHUNK_PLAINTEXT_SIZE, TAG_SIZE,
};
pub use envelope::{EnvelopeHeader, ENVELOPE_VERSION, HEADER_SIZE, MAGIC, NONCE_SIZE};
pub use key_resolver::{resolve_key, resolve_latest, KeyError, KEY_LEN};
