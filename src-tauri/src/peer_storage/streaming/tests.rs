use std::path::Path;
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::resume::PartialState;
use crate::peer_storage::streaming::{pipe_recv_to_writer_verified, ChunkVerifier};

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
    let completed = Arc::new(Mutex::new(vec![false; total_chunks]));
    let verifier = ChunkVerifier {
        expected_chunk_hashes: &hashes,
        chunk_size,
        start_chunk_index: 0,
        completed: completed.clone(),
    };

    let bytes = pipe_recv_to_writer_verified(recv, &mut sink, payload_len as u64, verifier, None)
        .await
        .expect("verified pipe ok");

    assert_eq!(bytes, payload_len as u64);
    assert_eq!(sink, payload, "verified bytes written verbatim");
    assert!(
        completed.lock().await.iter().all(|c| *c),
        "all chunks marked completed"
    );
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
    let completed = Arc::new(Mutex::new(vec![false; total_chunks]));
    let verifier = ChunkVerifier {
        expected_chunk_hashes: &hashes,
        chunk_size,
        start_chunk_index: 0,
        completed: completed.clone(),
    };

    let err = pipe_recv_to_writer_verified(recv, &mut sink, payload_len as u64, verifier, None)
        .await
        .expect_err("bad chunk must fail");

    match err {
        PeerStorageError::ChunkHashMismatch {
            index,
            expected,
            actual,
        } => {
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
    assert_eq!(*completed.lock().await, vec![true, false, false]);
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
    let completed = Arc::new(Mutex::new(vec![false; total_chunks]));
    let verifier = ChunkVerifier {
        expected_chunk_hashes: &hashes,
        chunk_size,
        start_chunk_index: 0,
        completed: completed.clone(),
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
    assert!(completed.lock().await.iter().all(|c| *c));

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
    let completed = Arc::new(Mutex::new(vec![false; 2]));
    let verifier = ChunkVerifier {
        expected_chunk_hashes: &hashes,
        chunk_size,
        start_chunk_index: 0,
        completed: completed.clone(),
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

/// Two simulated workers operate on disjoint chunk ranges of the same
/// download, sharing one `Arc<Mutex<Vec<bool>>>` for the bitmap and writing
/// the sidecar against the same target path. After both finish, the
/// persisted sidecar must reflect ALL bits set by ALL workers — neither
/// worker's bit may overwrite the other's. This is the regression scenario
/// for the I1+I4 race fix: before the change, each worker cloned the
/// bitmap into a local Vec, called `state.save(target)` from that local
/// snapshot, and the second writer would overwrite the first writer's bits
/// in the persisted sidecar.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pipe_recv_concurrent_writers_dont_lose_bits() {
    // A single interleaving rarely trips the lost-update, and a current-thread
    // runtime can't race the saves at all. Repeat the two-worker scenario on a
    // multi-thread runtime so a racing snapshot+save pair reliably surfaces a
    // regression of the under-lock-save fix.
    for _ in 0..64 {
        concurrent_writers_round().await;
    }
}

async fn concurrent_writers_round() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("concurrent.bin");

    let chunk_size: u32 = 256;
    let total_chunks: usize = 8;
    let payload_len = chunk_size as usize * total_chunks;
    let payload = pattern_bytes(payload_len, 0xAA);
    let file_hash = blake3::hash(&payload).to_hex().to_string();

    let hashes: Vec<String> = (0..total_chunks)
        .map(|i| {
            let s = i * chunk_size as usize;
            let e = (i + 1) * chunk_size as usize;
            chunk_hash(&payload[s..e])
        })
        .collect();

    // Worker A drives the first half (chunks 0..4); worker B drives the
    // second half (chunks 4..8). Both share the same completion bitmap.
    let half_chunks = total_chunks / 2;
    let half_bytes = half_chunks * chunk_size as usize;

    let payload_a = payload[..half_bytes].to_vec();
    let payload_b = payload[half_bytes..].to_vec();
    let hashes_a: Vec<String> = hashes[..half_chunks].to_vec();
    let hashes_b: Vec<String> = hashes[half_chunks..].to_vec();

    let completed = Arc::new(Mutex::new(vec![false; total_chunks]));
    let target_a = target.clone();
    let target_b = target.clone();
    let completed_a = completed.clone();
    let completed_b = completed.clone();
    let file_hash_a = file_hash.clone();
    let file_hash_b = file_hash.clone();

    let task_a = tokio::spawn(async move {
        let recv = duplex_with_bytes(payload_a).await;
        let mut sink: Vec<u8> = Vec::new();
        let verifier = ChunkVerifier {
            expected_chunk_hashes: &hashes_a,
            chunk_size,
            start_chunk_index: 0,
            completed: completed_a,
        };
        pipe_recv_to_writer_verified(
            recv,
            &mut sink,
            half_bytes as u64,
            verifier,
            Some((target_a.as_path(), &file_hash_a)),
        )
        .await
        .expect("worker A ok");
    });

    let task_b = tokio::spawn(async move {
        let recv = duplex_with_bytes(payload_b).await;
        let mut sink: Vec<u8> = Vec::new();
        let verifier = ChunkVerifier {
            expected_chunk_hashes: &hashes_b,
            chunk_size,
            start_chunk_index: half_chunks,
            completed: completed_b,
        };
        pipe_recv_to_writer_verified(
            recv,
            &mut sink,
            half_bytes as u64,
            verifier,
            Some((target_b.as_path(), &file_hash_b)),
        )
        .await
        .expect("worker B ok");
    });

    task_a.await.unwrap();
    task_b.await.unwrap();

    // Shared bitmap reflects every bit set by either worker.
    assert!(
        completed.lock().await.iter().all(|c| *c),
        "shared bitmap missing bits after concurrent verifiers",
    );

    // Persisted sidecar matches the shared bitmap — no torn write, no
    // lost-update from racing snapshot+save pairs.
    let loaded = PartialState::load(&target)
        .await
        .expect("sidecar load")
        .expect("sidecar present");
    assert_eq!(loaded.file_hash, file_hash);
    assert_eq!(loaded.chunk_size, chunk_size);
    assert_eq!(
        loaded.completed,
        vec![true; total_chunks],
        "persisted sidecar lost bits to a racing writer",
    );
}
