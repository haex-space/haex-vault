use std::sync::Arc;

use crate::peer_storage::error::PeerStorageError;

/// Generic worker pool with per-item bounded retry.
///
/// Spawns `concurrency` workers that pop `(start, end, attempt)` triples off
/// the shared `pending` queue, invoke `fetcher(start, end)`, and on Err either
/// re-queue with `attempt + 1` (while `attempt < max_retries`) or return the
/// error from that worker. Sibling workers keep draining the queue regardless
/// of whether one returned Err — the only thing that bubbles up is the first
/// permanent failure (after retries) encountered across all workers.
///
/// `on_retry` is invoked once per failed attempt that is about to be
/// re-queued, *before* the attempt is pushed back. The retry pool itself
/// doesn't know about per-attempt side effects (progress counters that need
/// rolling back, sidecar bytes from the failed attempt, etc.); the hook lets
/// the caller clean those up.
pub(crate) async fn run_bounded_retry_pool(
    pending: Arc<tokio::sync::Mutex<Vec<(u64, u64, u32)>>>,
    concurrency: usize,
    max_retries: u32,
    fetcher: Arc<
        dyn Fn(
                u64,
                u64,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), PeerStorageError>> + Send>,
            > + Send
            + Sync,
    >,
    on_retry: Option<
        Arc<
            dyn Fn(
                    (u64, u64, u32),
                    &PeerStorageError,
                )
                    -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
                + Send
                + Sync,
        >,
    >,
) -> Option<PeerStorageError> {
    let mut workers = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let pending = pending.clone();
        let fetcher = fetcher.clone();
        let on_retry = on_retry.clone();
        workers.push(tokio::spawn(async move {
            loop {
                let next = pending.lock().await.pop();
                let Some((start, end, attempt)) = next else {
                    break;
                };

                match fetcher(start, end).await {
                    Ok(()) => continue,
                    // A cancelled transfer is a deliberate abort, not a
                    // transient transport failure — return it immediately so
                    // the pool unwinds instead of burning through retries.
                    Err(PeerStorageError::Cancelled) => return Err(PeerStorageError::Cancelled),
                    Err(e) if attempt < max_retries => {
                        if let Some(hook) = on_retry.as_ref() {
                            hook((start, end, attempt), &e).await;
                        }
                        pending.lock().await.push((start, end, attempt + 1));
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok::<(), PeerStorageError>(())
        }));
    }

    let mut first_err: Option<PeerStorageError> = None;
    for handle in workers {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
            Err(join_err) => {
                if first_err.is_none() {
                    first_err = Some(PeerStorageError::ProtocolError {
                        reason: format!("worker join: {join_err}"),
                    });
                }
            }
        }
    }
    first_err
}
