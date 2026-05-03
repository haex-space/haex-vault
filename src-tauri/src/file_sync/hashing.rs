//! File hashing with an in-memory cache keyed by `(path, size, mtime)`.
//!
//! Sync diffs need an authoritative equality check that does not rely on
//! mtimes (the receiver's mtime always equals the write time, which never
//! matches the sender's). Hashing every file on every manifest scan would be
//! prohibitively expensive for multi-GB libraries, so we cache: as long as
//! `(absolute_path, size, mtime)` matches a previous scan, we reuse the
//! computed hash. A change in size or mtime invalidates the cache entry.
//!
//! The cache lives for the process lifetime — it is rebuilt on restart, but
//! the per-rule sync state DB ensures the first sync after restart is the
//! only slow one.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Mutex;

use lazy_static::lazy_static;
use sha2::{Digest, Sha256};

#[derive(Hash, PartialEq, Eq, Clone)]
struct CacheKey {
    path: String,
    size: u64,
    mtime: u64,
}

lazy_static! {
    static ref HASH_CACHE: Mutex<HashMap<CacheKey, String>> = Mutex::new(HashMap::new());
}

/// Hash buffer size — large enough to keep SHA-256 fed without excessive
/// syscall overhead, small enough to not bloat RAM with many parallel scans.
const HASH_BUF: usize = 256 * 1024;

/// Compute SHA-256 of a file. Streams in 256 KB chunks; lower-case hex output.
pub fn hash_file_sync(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; HASH_BUF];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Get the cached hash for a file, or compute and cache it.
///
/// `(absolute_path, size, mtime)` is the cache key — if any of these differ
/// from the cached entry, the file is treated as modified and re-hashed.
pub fn cached_hash(path: &Path, size: u64, mtime: u64) -> io::Result<String> {
    let key = CacheKey {
        path: path.to_string_lossy().to_string(),
        size,
        mtime,
    };

    if let Some(hash) = HASH_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .cloned()
    {
        return Ok(hash);
    }

    let hash = hash_file_sync(path)?;
    HASH_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, hash.clone());
    Ok(hash)
}
