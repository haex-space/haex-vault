//! The encrypted file format `HXCF` v1 (spec 072 research R11, contracts/blossom.md): one random
//! AES-256-GCM key per file, the plaintext in segments of 1 MiB, each sealed with the nonce
//! `prefix ‖ u32 BE index` and the AAD `"HXCF1" ‖ last flag`. A missing last segment, a segment out
//! of order or a changed byte fails; so does a plaintext whose SHA-256 is not the one the message
//! named. An empty file is one empty last segment.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use sha2::{Digest, Sha256};

use crate::error::{HolziError, Result};
use crate::sync::keys::hex;

/// The first bytes of every encrypted file.
pub const MAGIC: &[u8; 4] = b"HXCF";
/// The only version this one writes and reads.
pub const VERSION: u8 = 1;
/// Plaintext bytes per segment.
pub const SEGMENT: usize = 1024 * 1024;
/// Magic, version, segment size and nonce prefix.
pub const HEADER_LEN: usize = 4 + 1 + 4 + 8;
/// The GCM tag behind every segment.
pub const TAG_LEN: usize = 16;
/// HaexVault's own cap of a file (FR-034); a lower limit of the server wins.
pub const MAX_PLAINTEXT: u64 = 100_000_000;
/// Largest valid HXCF object under the plaintext limit.
const MAX_CIPHERTEXT: u64 =
    HEADER_LEN as u64 + MAX_PLAINTEXT + TAG_LEN as u64 * MAX_PLAINTEXT.div_ceil(SEGMENT as u64);

/// The secret of one file for sealing it. Only the key travels, inside the encrypted message; the
/// nonce prefix is in the file's header.
#[derive(Clone, PartialEq, Eq)]
pub struct FileKey {
    pub key: [u8; 32],
    pub nonce_prefix: [u8; 8],
}

impl std::fmt::Debug for FileKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileKey(..)")
    }
}

impl FileKey {
    /// A fresh random key and nonce prefix.
    pub fn generate() -> Result<Self> {
        let mut key = [0; 32];
        let mut nonce_prefix = [0; 8];
        getrandom::fill(&mut key).map_err(|e| invalid(&e.to_string()))?;
        getrandom::fill(&mut nonce_prefix).map_err(|e| invalid(&e.to_string()))?;
        Ok(Self { key, nonce_prefix })
    }

    fn cipher(&self) -> Aes256Gcm {
        cipher(&self.key)
    }
}

fn cipher(key: &[u8; 32]) -> Aes256Gcm {
    Aes256Gcm::new(&(*key).into())
}

/// How many bytes the encrypted form of `plaintext_len` bytes has: the header and one tag per
/// segment, at least one segment.
pub fn ciphertext_len(plaintext_len: u64) -> u64 {
    let segment = SEGMENT as u64;
    let segments = plaintext_len.div_ceil(segment).max(1);
    (TAG_LEN as u64)
        .saturating_mul(segments)
        .saturating_add(HEADER_LEN as u64)
        .saturating_add(plaintext_len)
}

/// SHA-256 as lowercase hex, the form messages and Blossom use.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn nonce(prefix: &[u8; 8], index: u32) -> [u8; 12] {
    let mut nonce = [0; 12];
    nonce[..8].copy_from_slice(prefix);
    nonce[8..].copy_from_slice(&index.to_be_bytes());
    nonce
}

fn aad(last: bool) -> [u8; 6] {
    let mut aad = *b"HXCF1\0";
    aad[5] = u8::from(last);
    aad
}

/// Encrypts a whole file. `ponytail:` the plaintext and the result are held in memory (≤ 100 MB per
/// file, research R14); upgrade: seal segment by segment from a reader into the upload stream.
pub fn encrypt(file: &FileKey, plaintext: &[u8]) -> Result<Vec<u8>> {
    if plaintext.len() as u64 > MAX_PLAINTEXT {
        return Err(HolziError::InvalidInput {
            reason: "the file is larger than 100 MB".into(),
        });
    }
    let len = usize::try_from(ciphertext_len(plaintext.len() as u64)).map_err(|_| too_big())?;
    let mut out = Vec::with_capacity(len);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&(SEGMENT as u32).to_be_bytes());
    out.extend_from_slice(&file.nonce_prefix);
    let cipher = file.cipher();
    let segments: Vec<&[u8]> = if plaintext.is_empty() {
        vec![&[]]
    } else {
        plaintext.chunks(SEGMENT).collect()
    };
    let count = segments.len();
    for (index, segment) in segments.into_iter().enumerate() {
        let last = index + 1 == count;
        let index = u32::try_from(index).map_err(|_| too_big())?;
        let sealed = cipher
            .encrypt(
                &Nonce::from(nonce(&file.nonce_prefix, index)),
                Payload {
                    msg: segment,
                    aad: &aad(last),
                },
            )
            .map_err(|_| invalid("a segment could not be sealed"))?;
        out.extend_from_slice(&sealed);
    }
    Ok(out)
}

/// Decrypts an encrypted file as its bytes come in: [`Self::push`] answers the plaintext of every
/// segment that is complete and not the last one; [`Self::finish`] opens the last one and checks
/// the plaintext against the SHA-256 the message named.
pub struct Decryptor {
    cipher: Aes256Gcm,
    /// From the header.
    nonce_prefix: [u8; 8],
    buffer: Vec<u8>,
    header_read: bool,
    index: u32,
    hasher: Sha256,
    expected_sha256: String,
    received: u64,
}

impl Decryptor {
    pub fn new(key: &[u8; 32], expected_sha256: &str) -> Self {
        Self {
            cipher: cipher(key),
            nonce_prefix: [0; 8],
            buffer: Vec::new(),
            header_read: false,
            index: 0,
            hasher: Sha256::new(),
            expected_sha256: expected_sha256.to_ascii_lowercase(),
            received: 0,
        }
    }

    /// Takes the next bytes; answers the plaintext of the segments they completed.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        let received = self
            .received
            .checked_add(bytes.len() as u64)
            .filter(|&len| len <= MAX_CIPHERTEXT)
            .ok_or_else(too_big)?;
        self.received = received;
        self.buffer.extend_from_slice(bytes);
        if !self.header_read {
            if self.buffer.len() < HEADER_LEN {
                return Ok(Vec::new());
            }
            self.read_header()?;
        }
        let sealed = SEGMENT + TAG_LEN;
        let mut plain = Vec::new();
        // A full segment is the last one only when nothing follows it, so one stays buffered.
        let buffer = std::mem::take(&mut self.buffer);
        let mut start = 0;
        while buffer.len() - start > sealed {
            plain.extend(self.open(&buffer[start..start + sealed], false)?);
            start += sealed;
        }
        self.buffer = buffer[start..].to_vec();
        Ok(plain)
    }

    /// Opens the last segment and checks the whole plaintext; answers the last plaintext bytes.
    pub fn finish(mut self) -> Result<Vec<u8>> {
        if !self.header_read || self.buffer.len() < TAG_LEN {
            return Err(damaged());
        }
        let segment = std::mem::take(&mut self.buffer);
        let plain = self.open(&segment, true)?;
        if hex(&self.hasher.finalize()) != self.expected_sha256 {
            return Err(damaged());
        }
        Ok(plain)
    }

    fn read_header(&mut self) -> Result<()> {
        let header: Vec<u8> = self.buffer.drain(..HEADER_LEN).collect();
        let segment = u32::from_be_bytes([header[5], header[6], header[7], header[8]]);
        if &header[..4] != MAGIC || header[4] != VERSION || segment as usize != SEGMENT {
            return Err(damaged());
        }
        self.nonce_prefix.copy_from_slice(&header[9..]);
        self.header_read = true;
        Ok(())
    }

    fn open(&mut self, segment: &[u8], last: bool) -> Result<Vec<u8>> {
        let plain = self
            .cipher
            .decrypt(
                &Nonce::from(nonce(&self.nonce_prefix, self.index)),
                Payload {
                    msg: segment,
                    aad: &aad(last),
                },
            )
            .map_err(|_| damaged())?;
        self.index = self.index.checked_add(1).ok_or_else(damaged)?;
        self.hasher.update(&plain);
        Ok(plain)
    }
}

/// Decrypts a whole encrypted file and checks it against `expected_sha256`.
pub fn decrypt(key: &[u8; 32], ciphertext: &[u8], expected_sha256: &str) -> Result<Vec<u8>> {
    let mut decryptor = Decryptor::new(key, expected_sha256);
    let mut plain = decryptor.push(ciphertext)?;
    plain.extend(decryptor.finish()?);
    Ok(plain)
}

fn damaged() -> HolziError {
    invalid("the file is damaged or not the one the message named")
}

fn too_big() -> HolziError {
    invalid("the file is too large")
}

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: format!("chat file: {reason}"),
    }
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod tests;
