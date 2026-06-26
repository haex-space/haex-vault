use std::sync::Arc;

use iroh::{EndpointId, RelayUrl};

use crate::file_sync::hashing::ChunkedHash;
use crate::peer_storage::client::StreamReadResult;
use crate::peer_storage::endpoint::PeerEndpoint;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::streaming;

mod multipart;
mod retry_pool;
mod single_stream;

pub(crate) use multipart::read_multipart_to_file;
pub(crate) use retry_pool::run_bounded_retry_pool;

use single_stream::download_single_stream_with_resume;

/// Download a remote file to `output_path`, picking single-stream or
/// multi-stream automatically based on the announced size.
///
/// This is the single entry point both file_sync and the frontend
/// `peer_storage_remote_read` Tauri command should use. The dispatch logic
/// (stat probe → multi-stream above [`streaming::MULTI_STREAM_THRESHOLD`],
/// single-stream below) used to be open-coded in `peer_provider`, while
/// the frontend command path went straight to single-stream — so UI
/// downloads of a 200 MB file ran ~4× slower than the same file via sync.
///
/// On success, logs the transfer rate to stderr so throughput regressions
/// are visible without separate instrumentation.
pub(crate) async fn download_file_to_path(
    endpoint: Arc<tokio::sync::RwLock<PeerEndpoint>>,
    remote_id: EndpointId,
    relay_url: Option<RelayUrl>,
    path: String,
    output_path: std::path::PathBuf,
    expected_chunks: Option<ChunkedHash>,
    on_progress: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    pause_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    ucan_token: String,
) -> Result<StreamReadResult, PeerStorageError> {
    let started = std::time::Instant::now();

    // 1 RTT probe to learn the size. On LAN this is sub-millisecond; on
    // any WAN/relay link it is repaid many times over by the multi-stream
    // throughput gain it unlocks for large files.
    let stat = endpoint
        .read()
        .await
        .remote_stat(remote_id, relay_url.clone(), &path, &ucan_token)
        .await?;

    if stat.entry.is_dir {
        return Err(PeerStorageError::ProtocolError {
            reason: format!("remote path is a directory, not a file: {path}"),
        });
    }

    // Resolve which ChunkedHash governs verification.
    //
    // - Sync-rule flow: caller supplied `expected_chunks` from the manifest.
    //   The stat-probe also returns chunks (served from the sender's hash
    //   cache). In steady state these agree; if they disagree the file has
    //   changed on the sender between manifest scan and download — abort
    //   loudly so the engine can re-scan rather than persisting bytes that
    //   no longer match the announced hash.
    // - File-browser flow: caller passes `None`; we adopt the stat-probe's
    //   chunks as the verifier. The receiver therefore learns the hashes
    //   from the peer it is downloading from — TLS already covers transport
    //   integrity, and the stat-probe rides the same authenticated
    //   connection as the bytes themselves.
    //
    // `chunks_to_use` is currently only retained for the throughput log;
    // Task 7 plumbs it into per-chunk verification on the receive path.
    let chunks_to_use = match (expected_chunks, stat.chunks.clone()) {
        (Some(manifest), Some(stat_chunks)) => {
            if manifest != stat_chunks {
                return Err(PeerStorageError::ManifestHashMismatch {
                    manifest_file_hash: manifest.file_hash,
                    actual_file_hash: stat_chunks.file_hash,
                });
            }
            // Functionally either — the manifest is the source of truth for
            // the sync flow, so keep it.
            manifest
        }
        (manifest_opt, None) => {
            // The early `stat.entry.is_dir` check above already rejected
            // directories — reaching here means the server told us the
            // path is a file but returned no chunks. That violates the
            // server-side invariant Task 6 installs (the peer is required
            // to send `chunks: Some(_)` for any non-directory entry it
            // owns), so surface a ProtocolError rather than silently
            // accepting a download with no per-chunk verification.
            let manifest_detail = manifest_opt
                .as_ref()
                .map(|m| format!(" (manifest claimed file_hash {})", m.file_hash))
                .unwrap_or_default();
            return Err(PeerStorageError::ProtocolError {
                reason: format!(
                    "stat reported non-directory path with no chunks (server invariant violation): {path}{manifest_detail}"
                ),
            });
        }
        (None, Some(stat_chunks)) => stat_chunks,
    };
    let use_multi = stat.entry.size >= streaming::MULTI_STREAM_THRESHOLD;

    let result = if use_multi {
        read_multipart_to_file(
            endpoint,
            remote_id,
            relay_url,
            path,
            output_path,
            stat.entry.size,
            streaming::MAX_PARALLEL_STREAMS_PER_FILE,
            &chunks_to_use,
            on_progress,
            cancel_token,
            pause_flag,
            ucan_token,
        )
        .await?
    } else {
        download_single_stream_with_resume(
            endpoint.clone(),
            remote_id,
            relay_url.clone(),
            &path,
            &output_path,
            stat.entry.size,
            &chunks_to_use,
            on_progress.clone(),
            cancel_token.clone(),
            pause_flag.clone(),
            &ucan_token,
        )
        .await?
    };

    let elapsed = started.elapsed();
    let secs = elapsed.as_secs_f64();
    let mb = result.bytes as f64 / (1024.0 * 1024.0);
    let rate = if secs > 0.0 { mb / secs } else { 0.0 };
    let streams = if use_multi {
        streaming::MAX_PARALLEL_STREAMS_PER_FILE
    } else {
        1
    };
    eprintln!(
        "[PeerStorage] download {} bytes in {:.2}s = {:.2} MB/s ({} stream{})",
        result.bytes,
        secs,
        rate,
        streams,
        if streams == 1 { "" } else { "s" }
    );

    Ok(result)
}
