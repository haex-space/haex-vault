//! File hashing with an in-memory cache keyed by `(path, size, mtime_nanos)`.
//!
//! Sync diffs need an authoritative equality check that does not rely on
//! mtimes (the receiver's mtime always equals the write time, which never
//! matches the sender's). Hashing every file on every manifest scan would be
//! prohibitively expensive for multi-GB libraries, so we cache: as long as
//! `(absolute_path, size, mtime_nanos)` matches a previous scan, we reuse the
//! computed BLAKE3 hashes (whole-file + per-chunk). A change in size or mtime
//! invalidates the cache entry.
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
//! This cache is shared across `file_sync` and `peer_storage` — peer_storage
//! stat-probe handlers call `cached_hash_chunked` to serve hashes to remote
//! readers without re-hashing per-request.
//!
//! ## Mutex poisoning
//!
//! HASH_CACHE locks use `unwrap_or_else(|e| e.into_inner())` rather than the
//! `lock_or_fail` pattern used for HLC / SQL paths. Rationale:
//! - The cache holds derived data only (BLAKE3 of file content). Recomputing
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
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Hash, PartialEq, Eq, Clone)]
struct CacheKey {
    path: String,
    size: u64,
    mtime_nanos: u128,
}

lazy_static! {
    static ref HASH_CACHE: Mutex<HashMap<CacheKey, ChunkedHash>> = Mutex::new(HashMap::new());
}

/// Chunk size for the resumable file-sync chunk hasher. 1 MiB, aligned to
/// `streaming::CHUNK_SIZE`.
pub const CHUNK_HASH_SIZE: u32 = 1024 * 1024;

/// Per-chunk + whole-file BLAKE3 hashes for resumable downloads.
///
/// Travels on the wire: served by the peer_storage stat-probe so file-browser
/// downloads can verify chunks without the receiver knowing the hashes in
/// advance, and carried in `FileState` so sync-rule manifests authoritatively
/// pin the expected hashes per chunk.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChunkedHash {
    /// BLAKE3 of the full file, lowercase hex.
    pub file_hash: String,
    /// Bytes per chunk (the last chunk may be shorter).
    pub chunk_size: u32,
    /// BLAKE3 of each chunk, lowercase hex, in order.
    pub chunk_hashes: Vec<String>,
}

/// Streaming chunked BLAKE3 over an arbitrary reader, shared by both the
/// path-based `hash_file_chunked` and the reader-based
/// `cached_hash_chunked_with_reader`.
fn chunk_hash_reader<R: Read>(mut reader: R) -> io::Result<ChunkedHash> {
    let mut file_hasher = blake3::Hasher::new();
    let mut chunk_hashes = Vec::new();
    let mut buf = vec![0u8; CHUNK_HASH_SIZE as usize];
    let mut filled = 0usize;

    loop {
        let n = reader.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
        if filled == buf.len() {
            file_hasher.update(&buf);
            chunk_hashes.push(blake3::hash(&buf).to_hex().to_string());
            filled = 0;
        }
    }
    if filled > 0 {
        file_hasher.update(&buf[..filled]);
        chunk_hashes.push(blake3::hash(&buf[..filled]).to_hex().to_string());
    }

    Ok(ChunkedHash {
        file_hash: blake3::Hasher::finalize(&file_hasher).to_hex().to_string(),
        chunk_size: CHUNK_HASH_SIZE,
        chunk_hashes,
    })
}

/// Compute BLAKE3 of a file in chunks of `CHUNK_HASH_SIZE`, returning both the
/// per-chunk hashes and the whole-file hash in a single streaming pass.
pub fn hash_file_chunked(path: &Path) -> io::Result<ChunkedHash> {
    chunk_hash_reader(File::open(path)?)
}

/// Seed the cache with a known `ChunkedHash` (e.g. from manifest after a
/// successful receive).
///
/// Use this on the receiver after a successful transfer: the sender already
/// announced the BLAKE3 file + chunk hashes via the manifest, so re-reading
/// the freshly-written file just to compute the same hash is wasted I/O. By
/// priming the cache with the announced hash keyed on the on-disk
/// `(size, mtime_nanos)`, the next manifest scan returns it instantly and
/// the diff engine sees a hash match instead of falling back to size+mtime.
pub fn prime_cache_chunked(path: &Path, size: u64, mtime_nanos: u128, chunks: ChunkedHash) {
    let key = CacheKey {
        path: path.to_string_lossy().to_string(),
        size,
        mtime_nanos,
    };
    HASH_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, chunks);
}

/// Cache-aware BLAKE3 chunked hash for a file on disk.
///
/// `(absolute_path, size, mtime_nanos)` is the cache key — if any of these
/// differ from the cached entry, the file is treated as modified and
/// re-hashed. Pass the modification time as nanoseconds since UNIX_EPOCH
/// (e.g. `mtime.duration_since(UNIX_EPOCH)?.as_nanos()`).
pub fn cached_hash_chunked(path: &Path, size: u64, mtime_nanos: u128) -> io::Result<ChunkedHash> {
    let key = path.to_string_lossy().to_string();
    cached_hash_chunked_with_reader(key, size, mtime_nanos, || File::open(path))
}

/// Cache-aware BLAKE3 chunked hash over an arbitrary byte source.
///
/// Used by the LocalProvider (file paths) and the Android Content URI scan
/// (FsUri-backed `std::fs::File` from `tauri_plugin_android_fs`). The cache
/// key is a caller-chosen string (e.g. absolute path, or `content://` URI)
/// plus `(size, mtime_nanos)` — the same invariants apply: same key + same
/// size + same nanos ⇒ unchanged file.
///
/// `open_reader` is only invoked on cache miss, so a cached scan never pays
/// the cost of opening the file (important on Android where every URI open
/// crosses the JNI boundary).
pub fn cached_hash_chunked_with_reader<R, F>(
    cache_key: String,
    size: u64,
    mtime_nanos: u128,
    open_reader: F,
) -> io::Result<ChunkedHash>
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

    let hash = chunk_hash_reader(open_reader()?)?;
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
        let h1 = cached_hash_chunked(tmp.path(), 18, 1_700_000_000_000_000_000).unwrap();

        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(tmp.path())
            .unwrap();
        f.write_all(b"changed-content!!!").unwrap();
        f.sync_all().unwrap();
        drop(f);

        // Same size, same wall-clock second, different nanos — cache must miss.
        let h2 = cached_hash_chunked(tmp.path(), 18, 1_700_000_000_500_000_000).unwrap();
        assert_ne!(
            h1.file_hash, h2.file_hash,
            "same-size rewrite within same second must rehash when nanos differ"
        );
    }

    #[test]
    fn identical_key_returns_cached() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"abc").unwrap();
        let h1 = cached_hash_chunked(tmp.path(), 3, 42).unwrap();
        // Even after the file is modified, identical (size, mtime) reuses the cached hash.
        std::fs::write(tmp.path(), b"xyz").unwrap();
        let h2 = cached_hash_chunked(tmp.path(), 3, 42).unwrap();
        assert_eq!(h1.file_hash, h2.file_hash);
    }

    #[test]
    fn prime_cache_skips_recomputation() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"real-content").unwrap();
        // Seed the cache with a hash that does NOT match the file's actual
        // content. cached_hash_chunked() should return our planted struct
        // without ever opening the file — proving prime_cache_chunked wires
        // correctly.
        let planted = ChunkedHash {
            file_hash: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
                .to_string(),
            chunk_size: CHUNK_HASH_SIZE,
            chunk_hashes: vec![
                "cafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe".to_string(),
            ],
        };
        prime_cache_chunked(tmp.path(), 12, 7777, planted.clone());
        let got = cached_hash_chunked(tmp.path(), 12, 7777).unwrap();
        assert_eq!(got.file_hash, planted.file_hash);
        assert_eq!(got.chunk_size, planted.chunk_size);
        assert_eq!(got.chunk_hashes, planted.chunk_hashes);
    }

    #[test]
    fn chunk_hashes_compose_correctly() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let data: Vec<u8> = (0..(3 * 1024 * 1024 + 17))
            .map(|i| (i % 251) as u8)
            .collect();
        std::fs::write(tmp.path(), &data).unwrap();

        let result = hash_file_chunked(tmp.path()).unwrap();

        assert_eq!(result.chunk_size, CHUNK_HASH_SIZE);
        assert_eq!(result.chunk_hashes.len(), 4, "3MiB + 17B → 4 chunks");

        // First three chunks are exactly CHUNK_HASH_SIZE bytes
        for i in 0..3 {
            let start = i * CHUNK_HASH_SIZE as usize;
            let end = start + CHUNK_HASH_SIZE as usize;
            let expected = blake3::hash(&data[start..end]).to_hex().to_string();
            assert_eq!(result.chunk_hashes[i], expected, "chunk {i} hash");
        }

        // Last chunk is the 17-byte tail
        let tail_start = 3 * CHUNK_HASH_SIZE as usize;
        let tail_hash = blake3::hash(&data[tail_start..]).to_hex().to_string();
        assert_eq!(result.chunk_hashes[3], tail_hash, "tail chunk hash");

        // File-level hash matches a fresh blake3 over the whole content
        let expected_file = blake3::hash(&data).to_hex().to_string();
        assert_eq!(result.file_hash, expected_file);
    }
}
