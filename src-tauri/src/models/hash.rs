//! Full-file SHA-256 hashing (spec 005 Entscheidung 6).
//!
//! One reusable function backs every write path that must set
//! `models.file_sha256` (download, update, import) and the read path that
//! must verify it (`load_model`, before every normal runtime load). Always
//! run via `spawn_blocking` — hashing a multi-gigabyte GGUF is a
//! synchronous, CPU/IO-bound loop that must never hold the async executor.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::{HaexError, Result};

/// Read buffer size for the hashing loop. Large enough to amortise
/// syscall overhead on multi-gigabyte GGUF files without holding an
/// unreasonable amount of memory.
const READ_BUFFER_BYTES: usize = 1024 * 1024;

/// Computes the lowercase-hex SHA-256 digest of the file at `path`.
/// Synchronous — callers on the async runtime MUST wrap this in
/// `spawn_blocking`.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|e| HaexError::Io {
        reason: format!("open {} for hashing: {e}", path.display()),
    })?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; READ_BUFFER_BYTES];
    loop {
        let n = file.read(&mut buf).map_err(|e| HaexError::Io {
            reason: format!("read {} for hashing: {e}", path.display()),
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(to_hex(&hasher.finalize()))
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Lowercase-hex SHA-256 of an in-memory byte slice. Used by the HF
/// discovery boundary to derive the deterministic `hf-<digest>` model id
/// (data-model.md) — a separate concern from [`sha256_file`], which reads
/// an on-disk file.
pub(crate) fn sha256_bytes_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    to_hex(&hasher.finalize())
}

/// `true` when `s` is a well-formed SHA-256 hex digest: exactly 64
/// lowercase hex characters. Used to validate a stored/expected hash
/// before treating it as authoritative.
pub fn is_valid_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
#[path = "hash_tests.rs"]
mod tests;
