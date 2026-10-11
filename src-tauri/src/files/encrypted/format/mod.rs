//! The format of encrypted folders, `HXEF` v1 (spec 048, FR-041, FR-042), as specified in
//! `docs/formats/encrypted-folder-v1.md`. Pure: bytes in, bytes out. No I/O, and nothing of
//! `files` but the name rule ([`check_name`](crate::files::local::edit::check_name)).
//!
//! - [`keys`]: the key derivations.
//! - [`head`]: a folder's head, which carries the wrapped folder key and the folder's name.
//! - [`content`]: content objects in 64 KiB blocks, sealed in a stream and opened by range.
//! - [`sidecar`]: one sidecar per version of an entry, with its parent, name and file facts.

pub mod content;
pub mod head;
pub mod keys;
pub mod sidecar;
#[cfg(any(test, feature = "platform-probe"))]
pub mod vectors;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use data_encoding::BASE32_NOPAD;

/// The first four bytes of every object.
pub const MAGIC: [u8; 4] = *b"HXEF";
/// The format version this code writes and the highest it reads.
pub const VERSION: u8 = 1;
/// Plaintext bytes per content block.
pub const CHUNK: usize = 65_536;
/// The type byte of a folder head.
pub const TYPE_HEAD: u8 = 0x00;
/// The type byte of a content object.
pub const TYPE_CONTENT: u8 = 0x01;
/// The type byte of a sidecar.
pub const TYPE_SIDECAR: u8 = 0x02;
/// The suffix of an encrypted folder's name in its parent.
pub const FOLDER_SUFFIX: &str = ".hxef";

/// XChaCha20-Poly1305 nonce length.
pub const NONCE_LEN: usize = 24;
/// Poly1305 tag length.
pub const TAG_LEN: usize = 16;

/// A folder, entry, sidecar or content id: 16 random bytes.
pub type Id = [u8; 16];

/// Length of an id in Base32 without padding.
const ID_TEXT_LEN: usize = 26;

/// Why bytes are not a valid `HXEF` object. The messages never hold names or contents (FR-027).
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("not an HXEF object of the expected type")]
    NotHxef,
    #[error("the object has a newer format version")]
    NewerFormat,
    #[error("the object is malformed")]
    Malformed,
    #[error("the object belongs to another folder, entry or content")]
    Mismatch,
    #[error("the object's length does not match its size")]
    Length,
    #[error("the object does not decrypt")]
    Decrypt,
    #[error("the range lies outside the file")]
    OutOfRange,
    #[error(transparent)]
    Name(#[from] crate::files::FilesError),
    /// Only the category: serde_json's message can quote a value, and that can be a name (FR-027).
    #[error("the sidecar is not valid JSON ({0:?})")]
    Json(serde_json::error::Category),
}

/// An id as it appears in object names and sidecars: Base32, lowercase, no padding.
pub fn encode_id(id: &Id) -> String {
    BASE32_NOPAD.encode(id).to_ascii_lowercase()
}

/// The id of [`encode_id`]'s spelling, and only of that spelling: 26 lowercase characters whose
/// unused trailing bits are zero.
pub fn decode_id(text: &str) -> Option<Id> {
    if !is_id_text(text) {
        return None;
    }
    BASE32_NOPAD
        .decode(text.to_ascii_uppercase().as_bytes())
        .ok()?
        .try_into()
        .ok()
}

/// Whether `name` (one segment, without the trailing `/`) has the form of an encrypted folder:
/// exactly 26 lowercase Base32 characters and `.hxef`. Ordinary entries may not take this form.
/// A name of this form whose characters are no valid id still counts, so it shows as a damaged
/// encrypted folder ([`folder_id`] gives `None`).
pub fn is_folder_prefix(name: &str) -> bool {
    name.strip_suffix(FOLDER_SUFFIX).is_some_and(is_id_text)
}

/// The folder id an encrypted folder's name carries.
pub fn folder_id(name: &str) -> Option<Id> {
    decode_id(name.strip_suffix(FOLDER_SUFFIX)?)
}

fn is_id_text(text: &str) -> bool {
    text.len() == ID_TEXT_LEN
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b))
}

/// `MAGIC | VERSION | type`, the start of every object and of its additional data.
fn prefix(object_type: u8) -> [u8; 6] {
    let [m0, m1, m2, m3] = MAGIC;
    [m0, m1, m2, m3, VERSION, object_type]
}

/// Checks the first six bytes of an object of `object_type`.
fn check_prefix(bytes: &[u8], object_type: u8) -> Result<(), FormatError> {
    let start = bytes.get(..6).ok_or(FormatError::Malformed)?;
    if start[..4] != MAGIC {
        return Err(FormatError::NotHxef);
    }
    if start[4] > VERSION {
        return Err(FormatError::NewerFormat);
    }
    if start[4] != VERSION || start[5] != object_type {
        return Err(FormatError::NotHxef);
    }
    Ok(())
}

fn cipher(key: &[u8; 32]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(Key::from_slice(key))
}

/// XChaCha20-Poly1305: `ciphertext | tag`.
fn seal(key: &[u8; 32], nonce: &[u8; NONCE_LEN], msg: &[u8], aad: &[u8]) -> Vec<u8> {
    cipher(key)
        .encrypt(XNonce::from_slice(nonce), Payload { msg, aad })
        // Fails only beyond 256 GiB of plaintext; everything sealed whole here is a key, a name or
        // a sidecar.
        .expect("XChaCha20-Poly1305 seals a message of this size")
}

fn open(
    key: &[u8; 32],
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, FormatError> {
    if nonce.len() != NONCE_LEN {
        return Err(FormatError::Malformed);
    }
    cipher(key)
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| FormatError::Decrypt)
}

/// A fixed-size piece of `bytes` at `at`.
fn array<const N: usize>(bytes: &[u8], at: usize) -> Result<[u8; N], FormatError> {
    bytes
        .get(at..at + N)
        .and_then(|piece| piece.try_into().ok())
        .ok_or(FormatError::Malformed)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
