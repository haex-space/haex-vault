//! Client-side remote operations — outgoing requests to peer endpoints.

use crate::file_sync::hashing::ChunkedHash;
use crate::peer_storage::protocol::FileEntry;

mod download;
mod list;
mod read;
mod write;

#[allow(unused_imports)]
pub(crate) use download::{download_file_to_path, read_multipart_to_file, run_bounded_retry_pool};

/// Outcome of a streaming peer read into a local file.
///
/// `hash` is the manifest's BLAKE3 `file_hash` that the chunked verifier
/// confirmed against the bytes on disk. It is `None` only for paths that
/// don't produce a comparable full-file hash (zero-byte short-circuit on
/// the multi-stream entry-point with no manifest).
#[derive(Debug, Clone)]
pub struct StreamReadResult {
    pub bytes: u64,
    pub hash: Option<String>,
}

/// Result of a remote stat-probe — file metadata plus, for files, the
/// BLAKE3 chunked-hash manifest served from the peer's hash cache.
#[derive(Debug, Clone)]
pub struct RemoteStat {
    pub entry: FileEntry,
    /// `Some` for files, `None` for directories.
    pub chunks: Option<ChunkedHash>,
}
