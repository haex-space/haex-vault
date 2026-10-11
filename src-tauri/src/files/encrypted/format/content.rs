//! Content objects, `P + "c/" + base32(oid)` (format "Inhaltsobjekt"):
//!
//! ```text
//! header:  MAGIC | VERSION | 0x01 | cid(16) | nonce_prefix(16)        = 38 bytes
//! block i: AEAD(DEK, nonce_prefix | u64le(i), block_i, aad = header | last_i)
//! ```
//!
//! Blocks hold [`CHUNK`] plaintext bytes, the last one 1 to [`CHUNK`]; only an empty file has an
//! empty (last) block. Every block is sealed exactly once under its `DEK`: [`ContentSealer`] only
//! moves forward, and a retry resends the same ciphertext or starts a new object (nonce reuse).

use std::ops::Range;

use chacha20poly1305::aead::AeadInPlace;
use chacha20poly1305::XNonce;
use zeroize::Zeroizing;

use super::{array, check_prefix, cipher, open, prefix, FormatError, Id, CHUNK, NONCE_LEN};
use super::{TAG_LEN, TYPE_CONTENT};
use crate::sync::keys::random_bytes;

/// Length of a content object's header.
pub const HEADER_LEN: usize = 38;
const CHUNK_U64: u64 = CHUNK as u64;
const BLOCK_LEN: u64 = CHUNK_U64 + TAG_LEN as u64;

/// Blocks of a file of `size` bytes: at least one.
pub fn block_count(size: u64) -> u64 {
    size.div_ceil(CHUNK_U64).max(1)
}

/// Length of the content object of a file of `size` bytes.
pub fn ciphertext_len(size: u64) -> u64 {
    HEADER_LEN as u64 + TAG_LEN as u64 * block_count(size) + size
}

/// Seals one file as a stream. Holds back one block until it knows whether another follows, so a
/// file of a multiple of [`CHUNK`] bytes ends with a full block marked last.
pub struct ContentSealer {
    dek: Zeroizing<[u8; 32]>,
    header: [u8; HEADER_LEN],
    next: u64,
    pending: Vec<u8>,
    started: bool,
}

impl ContentSealer {
    /// A new content object: fresh `DEK`, `cid` and `nonce_prefix`.
    pub fn fresh() -> Self {
        Self::with_parts(
            &random_bytes::<32>(),
            &random_bytes::<16>(),
            &random_bytes::<16>(),
        )
    }

    pub(super) fn with_parts(dek: &[u8; 32], cid: &Id, nonce_prefix: &[u8; 16]) -> Self {
        let mut header = [0u8; HEADER_LEN];
        header[..6].copy_from_slice(&prefix(TYPE_CONTENT));
        header[6..22].copy_from_slice(cid);
        header[22..].copy_from_slice(nonce_prefix);
        Self {
            dek: Zeroizing::new(*dek),
            header,
            next: 0,
            pending: Vec::with_capacity(CHUNK + TAG_LEN),
            started: false,
        }
    }

    /// The key the sidecar wraps.
    pub fn dek(&self) -> &[u8; 32] {
        &self.dek
    }

    /// The content id the sidecar names.
    pub fn cid(&self) -> Id {
        let mut cid = [0u8; 16];
        cid.copy_from_slice(&self.header[6..22]);
        cid
    }

    /// Takes the next plaintext bytes; appends to `out` what is ready (the header first).
    pub fn push(&mut self, mut data: &[u8], out: &mut Vec<u8>) {
        self.start(out);
        while !data.is_empty() {
            if self.pending.len() == CHUNK {
                self.seal_pending(false, out);
            }
            let take = (CHUNK - self.pending.len()).min(data.len());
            self.pending.extend_from_slice(&data[..take]);
            data = &data[take..];
        }
    }

    /// Seals the held-back block as the last one and appends it to `out`.
    pub fn finish(mut self, out: &mut Vec<u8>) {
        self.start(out);
        self.seal_pending(true, out);
    }

    fn start(&mut self, out: &mut Vec<u8>) {
        if !self.started {
            out.extend_from_slice(&self.header);
            self.started = true;
        }
    }

    fn seal_pending(&mut self, last: bool, out: &mut Vec<u8>) {
        let tag = cipher(&self.dek)
            .encrypt_in_place_detached(
                XNonce::from_slice(&block_nonce(&self.header, self.next)),
                &block_aad(&self.header, last),
                &mut self.pending,
            )
            // Fails only beyond 256 GiB; a block is at most CHUNK bytes.
            .expect("XChaCha20-Poly1305 seals a block");
        out.extend_from_slice(&self.pending);
        out.extend_from_slice(&tag);
        self.pending.clear();
        self.next += 1;
    }
}

/// Opens the blocks of one content object, as the sidecar describes it.
pub struct ContentOpener {
    dek: Zeroizing<[u8; 32]>,
    header: [u8; HEADER_LEN],
    size: u64,
}

impl ContentOpener {
    /// Checks the object's `header` (its first [`HEADER_LEN`] bytes, read once per opened file)
    /// against the sidecar's `cid`; `size` is the sidecar's plaintext size.
    pub fn new(header: &[u8], cid: &Id, dek: &[u8; 32], size: u64) -> Result<Self, FormatError> {
        check_prefix(header, TYPE_CONTENT)?;
        let header: [u8; HEADER_LEN] = array(header, 0)?;
        if header[6..22] != cid[..] {
            return Err(FormatError::Mismatch);
        }
        Ok(Self {
            dek: Zeroizing::new(*dek),
            header,
            size,
        })
    }

    /// Whether the object's length fits the size.
    pub fn check_len(&self, object_len: u64) -> Result<(), FormatError> {
        if object_len == ciphertext_len(self.size) {
            Ok(())
        } else {
            Err(FormatError::Length)
        }
    }

    pub fn block_count(&self) -> u64 {
        block_count(self.size)
    }

    /// Where block `index` lies in the object.
    pub fn block_span(&self, index: u64) -> Result<Range<u64>, FormatError> {
        let len = self.plain_len(index)? + TAG_LEN as u64;
        let start = HEADER_LEN as u64 + index * BLOCK_LEN;
        Ok(start..start + len)
    }

    /// The plaintext of block `index` from its bytes in the object.
    pub fn open_block(&self, index: u64, block: &[u8]) -> Result<Vec<u8>, FormatError> {
        if block.len() as u64 != self.plain_len(index)? + TAG_LEN as u64 {
            return Err(FormatError::Length);
        }
        let last = index + 1 == self.block_count();
        open(
            &self.dek,
            &block_nonce(&self.header, index),
            block,
            &block_aad(&self.header, last),
        )
    }

    /// The object bytes that hold plaintext `[start, end)`: blocks `⌊start/CHUNK⌋` to
    /// `⌊(end−1)/CHUNK⌋`.
    pub fn ciphertext_range(&self, start: u64, end: u64) -> Result<Range<u64>, FormatError> {
        if start >= end || end > self.size {
            return Err(FormatError::OutOfRange);
        }
        let first = self.block_span(start / CHUNK_U64)?;
        let last = self.block_span((end - 1) / CHUNK_U64)?;
        Ok(first.start..last.end)
    }

    /// Plaintext `[start, end)` from the object bytes of [`Self::ciphertext_range`].
    pub fn open_range(&self, start: u64, end: u64, bytes: &[u8]) -> Result<Vec<u8>, FormatError> {
        let range = self.ciphertext_range(start, end)?;
        if bytes.len() as u64 != range.end - range.start {
            return Err(FormatError::Length);
        }
        let first = start / CHUNK_U64;
        let mut plain = Vec::new();
        for index in first..=(end - 1) / CHUNK_U64 {
            let span = self.block_span(index)?;
            let at = usize::try_from(span.start - range.start).map_err(|_| FormatError::Length)?;
            let len = usize::try_from(span.end - span.start).map_err(|_| FormatError::Length)?;
            plain.extend(self.open_block(index, &bytes[at..at + len])?);
        }
        let skip = usize::try_from(start - first * CHUNK_U64).map_err(|_| FormatError::Length)?;
        let want = usize::try_from(end - start).map_err(|_| FormatError::Length)?;
        Ok(plain[skip..skip + want].to_vec())
    }

    fn plain_len(&self, index: u64) -> Result<u64, FormatError> {
        let count = self.block_count();
        if index >= count {
            return Err(FormatError::OutOfRange);
        }
        Ok(if index + 1 < count {
            CHUNK_U64
        } else {
            self.size - (count - 1) * CHUNK_U64
        })
    }
}

/// `nonce_prefix | u64le(index)`.
fn block_nonce(header: &[u8; HEADER_LEN], index: u64) -> [u8; NONCE_LEN] {
    let mut nonce = [0u8; NONCE_LEN];
    nonce[..16].copy_from_slice(&header[22..]);
    nonce[16..].copy_from_slice(&index.to_le_bytes());
    nonce
}

/// `header | last`.
fn block_aad(header: &[u8; HEADER_LEN], last: bool) -> [u8; HEADER_LEN + 1] {
    let mut aad = [0u8; HEADER_LEN + 1];
    aad[..HEADER_LEN].copy_from_slice(header);
    aad[HEADER_LEN] = u8::from(last);
    aad
}

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;
