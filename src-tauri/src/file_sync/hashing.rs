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
//!
//! ## Mutex poisoning
//!
//! HASH_CACHE locks use `unwrap_or_else(|e| e.into_inner())` rather than the
//! `lock_or_fail` pattern used for HLC / SQL paths. Rationale:
//! - The cache holds derived data only (SHA-256 of file content). Recomputing
//!   on the next call is correct and cheap.
//! - A poison here means a previous panic happened while a hash was being
//!   inserted or read; the cache entry MAY be torn but the inserted value is
//!   either correct or absent (we never partially update an entry — `insert`
//!   replaces atomically once the lock is held). The worst-case is a missed
//!   cache hit on the next scan.
//! - No CRDT, no sync, no audit-log writes happen through this code path, so
//!   there is nothing for the user to "restart to fix" — a banner row here
//!   would be misleading. The cache continues to function correctly.

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

/// Insert a known hash into the cache without recomputing.
///
/// Use this on the receiver after a successful transfer: the sender already
/// announced the SHA-256 of the file content via the manifest, so re-reading
/// the freshly-written file just to compute the same hash is wasted I/O. By
/// priming the cache with the announced hash keyed on the on-disk
/// `(size, mtime_nanos)`, the next manifest scan returns it instantly and
/// the diff engine sees a hash match instead of falling back to size+mtime.
pub fn prime_cache(path: &Path, size: u64, mtime_nanos: u128, hash: String) {
    let key = CacheKey {
        path: path.to_string_lossy().to_string(),
        size,
        mtime_nanos,
    };
    HASH_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, hash);
}

/// Get the cached hash for a file, or compute and cache it.
///
/// `(absolute_path, size, mtime_nanos)` is the cache key — if any of these
/// differ from the cached entry, the file is treated as modified and
/// re-hashed. Pass the modification time as nanoseconds since UNIX_EPOCH
/// (e.g. `mtime.duration_since(UNIX_EPOCH)?.as_nanos()`).
pub fn cached_hash(path: &Path, size: u64, mtime_nanos: u128) -> io::Result<String> {
    let key = path.to_string_lossy().to_string();
    cached_hash_with_reader(key, size, mtime_nanos, || File::open(path))
}

/// Cache-aware streaming SHA-256 over an arbitrary byte source.
///
/// Used by the LocalProvider (file paths) and the Android Content URI scan
/// (FileUri-backed `std::fs::File` from `tauri_plugin_android_fs`). The cache
/// key is a caller-chosen string (e.g. absolute path, or `content://` URI)
/// plus `(size, mtime_nanos)` — the same invariants apply: same key + same
/// size + same nanos ⇒ unchanged file.
///
/// `open_reader` is only invoked on cache miss, so a cached scan never pays
/// the cost of opening the file (important on Android where every URI open
/// crosses the JNI boundary).
pub fn cached_hash_with_reader<R, F>(
    cache_key: String,
    size: u64,
    mtime_nanos: u128,
    open_reader: F,
) -> io::Result<String>
where
    R: Read,
    F: FnOnce() -> io::Result<R>,
{
    let key = CacheKey {
        path: cache_key,
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

    let mut reader = open_reader()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; HASH_BUF];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let hash = hex::encode(hasher.finalize());
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

    #[test]
    fn prime_cache_skips_recomputation() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"real-content").unwrap();
        // Seed the cache with a hash that does NOT match the file's actual
        // content. cached_hash() should return our planted value without
        // ever opening the file — proving prime_cache wires correctly.
        let planted = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        prime_cache(tmp.path(), 12, 7777, planted.to_string());
        let got = cached_hash(tmp.path(), 12, 7777).unwrap();
        assert_eq!(got, planted);
    }
}
