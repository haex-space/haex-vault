use std::path::Path;

use tokio::io::AsyncWriteExt;

use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::resume::PartialState;
use crate::peer_storage::streaming::{ChunkVerifier, pipe_recv_to_writer_verified};

/// Build a vector with `len` bytes whose contents are deterministic and
/// unique enough that different chunks hash differently.
fn pattern_bytes(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| seed.wrapping_add(((i as u64) & 0xff) as u8))
        .collect()
}

fn chunk_hash(buf: &[u8]) -> String {
    blake3::hash(buf).to_hex().to_string()
}

/// In-memory recv stream: writes `bytes` into one end of a duplex pipe and
/// returns the other (read) half. Drops the writer once everything is
/// flushed so EOF surfaces normally.
async fn duplex_with_bytes(bytes: Vec<u8>) -> tokio::io::DuplexStream {
    let (mut tx, rx) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        tx.write_all(&bytes).await.expect("duplex write");
        tx.flush().await.expect("duplex flush");
        drop(tx);
    });
    rx
}

#[tokio::test]
async fn pipe_recv_verifies_happy_path() {
    let chunk_size: u32 = 1024;
    let total_chunks = 4;
    // 3.5 chunks worth so the last chunk is short — exercises the
    // partial-tail branch.
    let payload_len = (chunk_size as usize) * 3 + 512;

    let payload = pattern_bytes(payload_len, 0x42);

    let hashes: Vec<String> = (0..total_chunks)
        .map(|i| {
            let start = i as usize * chunk_size as usize;
            let end = ((i + 1) as usize * chunk_size as usize).min(payload_len);
            chunk_hash(&payload[start..end])
        })
        .collect();

    let recv = duplex_with_bytes(payload.clone()).await;
    let mut sink: Vec<u8> = Vec::new();
    let mut completed = vec![false; total_chunks];
    let verifier = ChunkVerifier {
        expected_chunk_hashes: &hashes,
        chunk_size,
        start_chunk_index: 0,
        completed: &mut completed,
    };

    let bytes = pipe_recv_to_writer_verified(recv, &mut sink, payload_len as u64, verifier, None)
        .await
        .expect("verified pipe ok");

    assert_eq!(bytes, payload_len as u64);
    assert_eq!(sink, payload, "verified bytes written verbatim");
    assert!(completed.iter().all(|c| *c), "all chunks marked completed");
}

#[tokio::test]
async fn pipe_recv_rejects_bad_chunk() {
    let chunk_size: u32 = 1024;
    let total_chunks = 3;
    let payload_len = (chunk_size as usize) * total_chunks;
    let payload = pattern_bytes(payload_len, 0x11);

    // Real hash for chunk 0, garbage for chunk 1 (the one we want to flag),
    // real hash for chunk 2 (never reached).
    let mut hashes: Vec<String> = (0..total_chunks)
        .map(|i| {
            let start = i * chunk_size as usize;
            let end = (i + 1) * chunk_size as usize;
            chunk_hash(&payload[start..end])
        })
        .collect();
    hashes[1] = "0".repeat(64);

    let recv = duplex_with_bytes(payload.clone()).await;
    let mut sink: Vec<u8> = Vec::new();
    let mut completed = vec![false; total_chunks];
    let verifier = ChunkVerifier {
        expected_chunk_hashes: &hashes,
        chunk_size,
        start_chunk_index: 0,
        completed: &mut completed,
    };

    let err = pipe_recv_to_writer_verified(recv, &mut sink, payload_len as u64, verifier, None)
        .await
        .expect_err("bad chunk must fail");

    match err {
        PeerStorageError::ChunkHashMismatch { index, expected, actual } => {
            assert_eq!(index, 1);
            assert_eq!(expected, "0".repeat(64));
            assert_ne!(actual, expected);
        }
        other => panic!("expected ChunkHashMismatch, got {other:?}"),
    }

    // Chunk 0 should have been written + marked completed before chunk 1
    // failed; chunk 1 must NOT have been written.
    assert_eq!(sink.len(), chunk_size as usize);
    assert_eq!(&sink[..], &payload[..chunk_size as usize]);
    assert_eq!(completed, vec![true, false, false]);
}

#[tokio::test]
async fn pipe_recv_persists_sidecar_per_chunk() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("dest.bin");

    let chunk_size: u32 = 512;
    let total_chunks = 3;
    let payload_len = (chunk_size as usize) * total_chunks;
    let payload = pattern_bytes(payload_len, 0x77);
    let file_hash = blake3::hash(&payload).to_hex().to_string();
    let hashes: Vec<String> = (0..total_chunks)
        .map(|i| {
            let s = i * chunk_size as usize;
            let e = (i + 1) * chunk_size as usize;
            chunk_hash(&payload[s..e])
        })
        .collect();

    let recv = duplex_with_bytes(payload.clone()).await;
    let mut sink: Vec<u8> = Vec::new();
    let mut completed = vec![false; total_chunks];
    let verifier = ChunkVerifier {
        expected_chunk_hashes: &hashes,
        chunk_size,
        start_chunk_index: 0,
        completed: &mut completed,
    };

    let bytes = pipe_recv_to_writer_verified(
        recv,
        &mut sink,
        payload_len as u64,
        verifier,
        Some((target.as_path() as &Path, &file_hash)),
    )
    .await
    .expect("verified pipe ok");

    assert_eq!(bytes, payload_len as u64);
    assert!(completed.iter().all(|c| *c));

    // Sidecar exists and reflects the final state. Task 11 will clear it
    // after the caller renames the partial file into place; the helper
    // itself just keeps it fresh.
    let loaded = PartialState::load(&target)
        .await
        .expect("sidecar load")
        .expect("sidecar present after success");
    assert_eq!(loaded.file_hash, file_hash);
    assert_eq!(loaded.chunk_size, chunk_size);
    assert_eq!(loaded.completed, vec![true, true, true]);
}

#[tokio::test]
async fn pipe_recv_rejects_mismatched_chunk_count() {
    let chunk_size: u32 = 1024;
    let payload_len = chunk_size as usize * 2;
    let payload = pattern_bytes(payload_len, 0x01);
    // Hand in 1 hash for what should be 2 chunks — the helper should
    // bail before even touching the stream.
    let hashes = vec![chunk_hash(&payload[..chunk_size as usize])];

    let recv = duplex_with_bytes(payload.clone()).await;
    let mut sink: Vec<u8> = Vec::new();
    let mut completed = vec![false; 2];
    let verifier = ChunkVerifier {
        expected_chunk_hashes: &hashes,
        chunk_size,
        start_chunk_index: 0,
        completed: &mut completed,
    };

    let err = pipe_recv_to_writer_verified(recv, &mut sink, payload_len as u64, verifier, None)
        .await
        .expect_err("count mismatch must fail");

    assert!(
        matches!(err, PeerStorageError::ProtocolError { .. }),
        "unexpected error variant: {err:?}"
    );
    assert!(sink.is_empty(), "nothing written when contract is broken");
}
