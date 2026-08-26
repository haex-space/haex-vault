//! File-content envelope + chunk-level AEAD for Phase 4 (cloud file sync).
//!
//! **Scope:** the AEAD primitives ([`chunk`], [`envelope`]), shared-space
//! key resolution ([`key_resolver`], Round B), the metadata
//! sidecar/object-key mapping ([`sidecar`], [`object_key`], Round C), and
//! file-content sealing plus the `SyncProvider` decorator ([`content`],
//! [`provider`], Round D). The chunk/envelope primitives do no I/O;
//! `key_resolver` and `object_key` read and write via a live
//! [`DbConnection`](crate::database::DbConnection), and
//! `object_key::bootstrap_object_key_cache` additionally talks to a
//! [`StorageBackend`](crate::remote_storage::backend::StorageBackend) to
//! list/download sidecars. [`provider::EncryptingSyncProvider`] wraps an
//! existing `SyncProvider`; nothing in this module touches
//! `cloud_provider.rs` directly.
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
pub mod content;
pub mod envelope;
pub mod key_resolver;
pub mod object_key;
pub mod provider;
pub mod sidecar;
pub mod vault_key_derivation;

#[cfg(test)]
mod tests;

pub use chunk::{
    ciphertext_len, num_chunks, open_chunk, plaintext_len, seal_chunk, CryptoError,
    CHUNK_CIPHERTEXT_SIZE, CHUNK_PLAINTEXT_SIZE, TAG_SIZE,
};
pub use content::{open_bytes, open_stream, seal_bytes, seal_stream, StreamCryptoError};
pub use envelope::{EnvelopeHeader, ENVELOPE_VERSION, HEADER_SIZE, MAGIC, NONCE_SIZE};
pub use key_resolver::{clear_key_cache, resolve_key, resolve_latest, KeyError, KEY_LEN};
pub use object_key::{
    bootstrap_object_key_cache, generate_object_key, lookup_object_key, mark_object_deleted,
    set_object_key, sidecar_key_for, BootstrapReport, ObjectKeyError, SIDECAR_SUFFIX,
};
pub use provider::{EncryptingSyncProvider, FileKeySource, ProviderCryptoError};
pub use sidecar::{open_sidecar, seal_sidecar, SidecarError, SidecarPayload};
pub use vault_key_derivation::{derive_vault_file_key, VAULT_FILE_KEY_LEN};
