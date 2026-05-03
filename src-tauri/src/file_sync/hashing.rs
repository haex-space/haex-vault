//! File hashing with an in-memory cache keyed by `(path, size, mtime_nanos)`.
//!
//! Sync diffs need an authoritative equality check that does not rely on
//! mtimes (the receiver's mtime always equals the write time, which never
//! matches the sender's). Hashing every file on every manifest scan would be
//! prohibitively expensive for multi-GB libraries, so we cache: as long as
//! `(absolute_path, size, mtime_nanos)` matches a previous scan, we reuse the
//! computed hash. A change in size or mtime invalidates the cache entry.
//!
//! The mtime component uses nanosecond resolution: a same-size rewrite
//! within the same wall-clock second on filesystems with sub-second mtime
//! still invalidates the cache. On filesystems that only expose
//! second-resolution mtime, same-size same-second edits remain
//! indistinguishable — but that's a filesystem limitation, not a cache one.
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
    mtime_nanos: u128,
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
/// `(absolute_path, size, mtime_nanos)` is the cache key — if any of these
/// differ from the cached entry, the file is treated as modified and
/// re-hashed. Pass the modification time as nanoseconds since UNIX_EPOCH
/// (e.g. `mtime.duration_since(UNIX_EPOCH)?.as_nanos()`).
pub fn cached_hash(path: &Path, size: u64, mtime_nanos: u128) -> io::Result<String> {
    let key = CacheKey {
        path: path.to_string_lossy().to_string(),
        size,
        mtime_nanos,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn same_size_same_second_different_nanos_invalidates() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"original-content!!").unwrap();
        let h1 = cached_hash(tmp.path(), 18, 1_700_000_000_000_000_000).unwrap();

        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(tmp.path())
            .unwrap();
        f.write_all(b"changed-content!!!").unwrap();
        f.sync_all().unwrap();
        drop(f);

        // Same size, same wall-clock second, different nanos — cache must miss.
        let h2 = cached_hash(tmp.path(), 18, 1_700_000_000_500_000_000).unwrap();
        assert_ne!(h1, h2, "same-size rewrite within same second must rehash when nanos differ");
    }

    #[test]
    fn identical_key_returns_cached() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"abc").unwrap();
        let h1 = cached_hash(tmp.path(), 3, 42).unwrap();
        // Even after the file is modified, identical (size, mtime) reuses the cached hash.
        std::fs::write(tmp.path(), b"xyz").unwrap();
        let h2 = cached_hash(tmp.path(), 3, 42).unwrap();
        assert_eq!(h1, h2);
    }
}
