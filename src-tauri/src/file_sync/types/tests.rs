use super::*;

#[test]
fn chunked_hash_accepts_zero_byte_file() {
    // Zero-byte files have `chunk_hashes: Some(vec![])` because the streaming
    // hasher's tail-flush never fires when nothing was read. The manifest is
    // still authoritative (file_hash = BLAKE3 of empty input), so
    // `chunked_hash()` must return Some and not silently fall back to stat.
    let state = FileState {
        relative_path: "empty.bin".into(),
        size: 0,
        modified_at: 0,
        is_directory: false,
        hash: Some(blake3::hash(&[]).to_hex().to_string()),
        chunk_size: Some(1024 * 1024),
        chunk_hashes: Some(vec![]),
    };
    let chunks = state
        .chunked_hash()
        .expect("zero-byte file must produce ChunkedHash");
    assert_eq!(chunks.chunk_hashes.len(), 0);
    assert_eq!(chunks.chunk_size, 1024 * 1024);
}

#[test]
fn chunked_hash_rejects_non_empty_file_with_empty_chunks() {
    // A non-zero-size file with no chunk hashes is a malformed manifest.
    let state = FileState {
        relative_path: "weird.bin".into(),
        size: 10,
        modified_at: 0,
        is_directory: false,
        hash: Some("00".repeat(32)),
        chunk_size: Some(1024 * 1024),
        chunk_hashes: Some(vec![]),
    };
    assert!(
        state.chunked_hash().is_none(),
        "non-empty file with empty chunks must return None"
    );
}

#[test]
fn chunked_hash_rejects_directory() {
    // Directories never participate in chunk verification — even if all
    // chunk fields happen to be populated, the directory flag wins.
    let state = FileState {
        relative_path: "subdir".into(),
        size: 0,
        modified_at: 0,
        is_directory: true,
        hash: Some("00".repeat(32)),
        chunk_size: Some(1024 * 1024),
        chunk_hashes: Some(vec![]),
    };
    assert!(
        state.chunked_hash().is_none(),
        "directories must not produce ChunkedHash"
    );
}

#[test]
fn chunked_hash_rejects_zero_chunk_size() {
    // Defensive guard: `chunk_size == 0` is malformed; verifier rejects it too.
    let state = FileState {
        relative_path: "weird.bin".into(),
        size: 0,
        modified_at: 0,
        is_directory: false,
        hash: Some(blake3::hash(&[]).to_hex().to_string()),
        chunk_size: Some(0),
        chunk_hashes: Some(vec![]),
    };
    assert!(
        state.chunked_hash().is_none(),
        "chunk_size == 0 must return None"
    );
}
