//! Envelope header: fixed-size prefix that lets a reader identify a haex file
//! envelope, pick the right key epoch, and derive per-chunk nonces without
//! looking at any out-of-band metadata.
//!
//! The header carries only what is needed to decrypt the body: a magic to
//! distinguish envelopes from legacy plaintext, a version to guard format
//! evolution, the epoch the file was sealed under (indexes
//! `haex_mls_sync_keys` in Round B), and the random per-file nonce that
//! `chunk::chunk_nonce` combines with a chunk index. No path, no size, no
//! content-type — those live in the metadata sidecar (Round C) so the storage
//! provider learns as little as possible from the ciphertext alone.

use crate::file_sync::crypto::chunk::CryptoError;

/// `HXFE` — Haex File Envelope. Chosen to be ASCII-visible so hexdumps
/// distinguish envelopes from unrelated binary blobs at a glance, and unlikely
/// to collide with random data or with the leading bytes of common file
/// formats.
pub const MAGIC: [u8; 4] = *b"HXFE";

/// Current envelope format version. Bump this whenever the byte layout of the
/// header or the chunk-body serialisation changes; downgrades are rejected
/// (see `EnvelopeHeader::parse`).
pub const ENVELOPE_VERSION: u8 = 1;

/// Length of the per-file XChaCha20Poly1305 nonce (24 bytes = 192 bits). The
/// last 8 bytes double as a per-chunk counter surface — see
/// `chunk::chunk_nonce`.
pub const NONCE_SIZE: usize = 24;

/// Serialized header size in bytes: `magic(4) + version(1) + epoch(8) + nonce(24)`.
pub const HEADER_SIZE: usize = 4 + 1 + 8 + NONCE_SIZE;

/// Parsed / to-be-written envelope header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeHeader {
    /// Format version. Always [`ENVELOPE_VERSION`] for freshly written
    /// headers. Preserved on parse so downstream code can log which version a
    /// legacy object was sealed under (once we start supporting more than one).
    pub version: u8,
    /// MLS epoch that produced the key this file was sealed with. Looked up in
    /// `haex_mls_sync_keys WHERE space_id = ? AND epoch = ?` on decrypt (see
    /// Round B in the plan).
    pub epoch: u64,
    /// Random per-file nonce. XORed with the chunk index to derive each
    /// chunk's nonce.
    pub file_nonce: [u8; NONCE_SIZE],
}

impl EnvelopeHeader {
    /// Build a fresh header for `epoch` with the given random nonce.
    pub fn new(epoch: u64, file_nonce: [u8; NONCE_SIZE]) -> Self {
        Self {
            version: ENVELOPE_VERSION,
            epoch,
            file_nonce,
        }
    }

    /// Serialize into a caller-owned buffer. The first [`HEADER_SIZE`] bytes
    /// of `out` are overwritten; any trailing bytes are left untouched, which
    /// lets callers write the header at the start of a larger stream buffer
    /// without an allocation.
    pub fn write(&self, out: &mut [u8]) -> Result<(), CryptoError> {
        if out.len() < HEADER_SIZE {
            return Err(CryptoError::BufferTooSmall);
        }
        self.write_into(&mut out[..HEADER_SIZE]);
        Ok(())
    }

    /// Serialize into a freshly allocated fixed-size array.
    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut out = [0u8; HEADER_SIZE];
        self.write_into(&mut out);
        out
    }

    /// Precondition: `out.len() == HEADER_SIZE`. Only reachable via `write`
    /// (bounds-checked) or `to_bytes` (fixed-size array), so no runtime check
    /// is needed here.
    fn write_into(&self, out: &mut [u8]) {
        out[0..4].copy_from_slice(&MAGIC);
        out[4] = self.version;
        out[5..13].copy_from_slice(&self.epoch.to_le_bytes());
        out[13..HEADER_SIZE].copy_from_slice(&self.file_nonce);
    }

    /// Parse from a byte slice, consuming exactly [`HEADER_SIZE`] bytes.
    ///
    /// Unknown versions are rejected outright: the version field is the sole
    /// migration hook, so best-effort interpretation of a future layout would
    /// silently decrypt with the wrong assumptions. Callers that need to walk
    /// a mixed corpus (e.g. Round E's plaintext-to-envelope migration) should
    /// use [`is_envelope`] to distinguish before attempting a parse.
    pub fn parse(buf: &[u8]) -> Result<Self, CryptoError> {
        if buf.len() < HEADER_SIZE {
            return Err(CryptoError::HeaderTooShort);
        }
        if buf[0..4] != MAGIC {
            return Err(CryptoError::BadMagic);
        }
        let version = buf[4];
        if version != ENVELOPE_VERSION {
            return Err(CryptoError::UnsupportedVersion(version));
        }
        let mut epoch_bytes = [0u8; 8];
        epoch_bytes.copy_from_slice(&buf[5..13]);
        let epoch = u64::from_le_bytes(epoch_bytes);
        let mut file_nonce = [0u8; NONCE_SIZE];
        file_nonce.copy_from_slice(&buf[13..HEADER_SIZE]);
        Ok(Self {
            version,
            epoch,
            file_nonce,
        })
    }
}

/// Cheap check: does `buf` start with the envelope magic? Used by Round E to
/// classify legacy plaintext objects without paying the cost of a full parse.
pub fn is_envelope(buf: &[u8]) -> bool {
    buf.len() >= MAGIC.len() && buf[..MAGIC.len()] == MAGIC
}
