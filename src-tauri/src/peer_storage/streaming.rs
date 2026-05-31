//! Shared pipelining primitives for peer-storage file transfers.
//!
//! Every direction of a peer-storage transfer pairs a disk-IO half with a
//! network-IO half. A serial loop (`recv.read().await` then `file.write_all()
//! .await`, or vice versa) makes each chunk pay both syscalls back-to-back —
//! fine on slow links, but on a fast LAN it pegs per-stream throughput to
//! roughly `chunk_size / (disk_latency + net_latency)`.
//!
//! The two helpers in this module decouple the halves through a bounded
//! `mpsc` channel so disk and network can overlap. Pulling the same logic
//! out of each call site also guarantees both directions use the same
//! chunk size and channel depth, which used to drift independently.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// 1 MB chunks. Big enough to amortise per-syscall overhead on fast LAN
/// links, small enough that `CHUNK_SIZE * CHANNEL_DEPTH * TRANSFER_CONCURRENCY`
/// stays well under tens of MB of live buffer per direction.
pub const CHUNK_SIZE: usize = 1024 * 1024;

/// Number of chunks buffered between the disk and network halves of each
/// pipeline. With `CHUNK_SIZE = 1 MB` this gives roughly 8 MB of in-flight
/// buffer per active stream per direction.
pub const CHANNEL_DEPTH: usize = 8;

/// Files at or above this size fan out across [`MAX_PARALLEL_STREAMS_PER_FILE`]
/// iroh streams. Below this threshold a single stream is faster because the
/// stat probe + extra `open_stream` round-trips outweigh the throughput gain.
pub const MULTI_STREAM_THRESHOLD: u64 = 16 * 1024 * 1024;

/// Maximum number of iroh streams a single download splits into. The QUIC
/// connection allows 256 bidi streams, but we cap per-file so several files
/// can still transfer concurrently under the engine's `TRANSFER_CONCURRENCY`.
pub const MAX_PARALLEL_STREAMS_PER_FILE: usize = 4;

#[derive(Debug)]
pub enum PipelineError {
    /// Disk-side I/O failure (read, write, flush).
    Io(std::io::Error),
    /// Network-side failure or unexpected EOF.
    Stream(String),
    /// Aborted via a cancellation token.
    Cancelled,
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::Stream(s) => write!(f, "stream: {s}"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::error::Error for PipelineError {}

/// Options that only apply to the network → disk direction.
#[derive(Default)]
pub struct RecvOptions {
    pub on_progress: Option<Box<dyn Fn(u64, u64) + Send>>,
    pub cancel_token: Option<CancellationToken>,
    pub pause_flag: Option<Arc<AtomicBool>>,
    /// Compute SHA-256 of the bytes written. Only meaningful for full-file
    /// reads — a partial-range hash is not comparable to a manifest hash.
    pub compute_hash: bool,
}

#[derive(Debug, Default)]
pub struct RecvStats {
    pub bytes: u64,
    pub hash: Option<String>,
}

/// Options for the disk → network direction. Same shape as [`RecvOptions`]
/// minus the receive-only fields (no pause for uploads — the API surface
/// keeps mirroring the read path but pause is not wired through yet).
#[derive(Default)]
pub struct SendOptions {
    pub on_progress: Option<Box<dyn Fn(u64, u64) + Send>>,
    pub cancel_token: Option<CancellationToken>,
}

#[derive(Debug, Default)]
pub struct SendStats {
    pub bytes: u64,
}

/// Disk → network pipeline.
///
/// Spawns a reader task that pulls `size` bytes from `reader` in
/// `CHUNK_SIZE` slices and feeds them through a bounded `mpsc` to the
/// network writer on this task. Returns once `size` bytes have been
/// forwarded (or `reader` reached EOF early, which is surfaced as a partial
/// transfer the caller can detect via `send.finish()` semantics).
///
/// `reader` must already be positioned at the first byte to transfer
/// (e.g. by an earlier `seek`).
pub async fn pipe_reader_to_send<R>(
    send: &mut iroh::endpoint::SendStream,
    mut reader: R,
    size: u64,
    options: SendOptions,
) -> Result<SendStats, PipelineError>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<Result<Vec<u8>, std::io::Error>>(CHANNEL_DEPTH);

    let read_task = tokio::spawn(async move {
        let mut remaining = size;
        while remaining > 0 {
            let to_read = (remaining as usize).min(CHUNK_SIZE);
            let mut buf = vec![0u8; to_read];
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    buf.truncate(n);
                    if tx.send(Ok(buf)).await.is_err() {
                        return;
                    }
                    remaining -= n as u64;
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            }
        }
    });

    let SendOptions {
        on_progress,
        cancel_token,
    } = options;

    let mut bytes_sent: u64 = 0;
    let mut net_err: Option<PipelineError> = None;
    while let Some(item) = rx.recv().await {
        if let Some(ref token) = cancel_token {
            if token.is_cancelled() {
                net_err = Some(PipelineError::Cancelled);
                break;
            }
        }
        match item {
            Ok(chunk) => {
                if let Err(e) = send.write_all(&chunk).await {
                    net_err = Some(PipelineError::Stream(format!("send write: {e}")));
                    break;
                }
                bytes_sent += chunk.len() as u64;
                if let Some(ref cb) = on_progress {
                    cb(bytes_sent, size);
                }
            }
            Err(e) => {
                let _ = read_task.await;
                return Err(PipelineError::Io(e));
            }
        }
    }
    let _ = read_task.await;

    if let Some(err) = net_err {
        return Err(err);
    }
    Ok(SendStats { bytes: bytes_sent })
}

/// Network → disk pipeline.
///
/// Reads `size` bytes from `recv` on this task and forwards them through a
/// bounded `mpsc` to a writer task that owns `writer`. Honours optional
/// cancel/pause flags between chunks and reports per-chunk progress.
///
/// `RecvStats.bytes` is the count actually written (and flushed) to the
/// writer; callers must check it against the announced `size` themselves.
pub async fn pipe_recv_to_writer<W>(
    recv: &mut iroh::endpoint::RecvStream,
    writer: W,
    size: u64,
    options: RecvOptions,
) -> Result<RecvStats, PipelineError>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(CHANNEL_DEPTH);
    let compute_hash = options.compute_hash;

    let writer_task: tokio::task::JoinHandle<Result<(u64, Option<String>), std::io::Error>> =
        tokio::spawn(async move {
            let mut writer = writer;
            let mut hasher = compute_hash.then(Sha256::new);
            let mut bytes_written: u64 = 0;
            while let Some(chunk) = rx.recv().await {
                writer.write_all(&chunk).await?;
                if let Some(h) = hasher.as_mut() {
                    h.update(&chunk);
                }
                bytes_written += chunk.len() as u64;
            }
            writer.flush().await?;
            Ok((bytes_written, hasher.map(|h| hex::encode(h.finalize()))))
        });

    let RecvOptions {
        on_progress,
        cancel_token,
        pause_flag,
        compute_hash: _,
    } = options;

    let mut bytes_received: u64 = 0;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut io_err: Option<PipelineError> = None;

    while bytes_received < size {
        if let Some(ref token) = cancel_token {
            if token.is_cancelled() {
                io_err = Some(PipelineError::Cancelled);
                break;
            }
        }
        if let Some(ref flag) = pause_flag {
            while flag.load(Ordering::Relaxed) {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if let Some(ref token) = cancel_token {
                    if token.is_cancelled() {
                        break;
                    }
                }
            }
            if let Some(ref token) = cancel_token {
                if token.is_cancelled() {
                    io_err = Some(PipelineError::Cancelled);
                    break;
                }
            }
        }

        match recv.read(&mut buf).await {
            Ok(Some(n)) => {
                let chunk = buf[..n].to_vec();
                if tx.send(chunk).await.is_err() {
                    // Writer task aborted — its error surfaces via the join below.
                    break;
                }
                bytes_received += n as u64;
                if let Some(ref cb) = on_progress {
                    cb(bytes_received, size);
                }
            }
            Ok(None) => {
                io_err = Some(PipelineError::Stream(format!(
                    "stream ended early: expected {size} bytes, received {bytes_received}"
                )));
                break;
            }
            Err(e) => {
                io_err = Some(PipelineError::Stream(format!("recv read: {e}")));
                break;
            }
        }
    }
    drop(tx);

    let join = writer_task.await.map_err(|e| {
        PipelineError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("writer task: {e}"),
        ))
    })?;
    let (bytes_written, hash) = join.map_err(PipelineError::Io)?;

    if let Some(err) = io_err {
        return Err(err);
    }

    Ok(RecvStats {
        bytes: bytes_written,
        hash,
    })
}
