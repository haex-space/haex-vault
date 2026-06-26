//! Tests for the client/download retry-pool surface.
//!
//! Plan 004 added a `cancel_token` parameter to `run_bounded_retry_pool` so
//! cancellation between pops can no longer orphan a re-queued range. This
//! test pins that behaviour.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::peer_storage::client::run_bounded_retry_pool;
use crate::peer_storage::error::PeerStorageError;

#[tokio::test]
async fn retry_pool_returns_cancelled_when_token_cancelled_between_pops() {
    // Two ranges in the queue. The fetcher cancels the token after running
    // once; the next loop iteration on either worker must observe the
    // cancellation at the top of the loop and return `Cancelled` instead of
    // continuing to drain the queue.
    let pending = Arc::new(tokio::sync::Mutex::new(vec![
        (0u64, 100u64, 0u32),
        (100, 200, 0),
    ]));
    let cancel = CancellationToken::new();
    let cancel_for_fetcher = cancel.clone();

    let fetcher: Arc<
        dyn Fn(
                u64,
                u64,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), PeerStorageError>> + Send>,
            > + Send
            + Sync,
    > = Arc::new(move |_start, _end| {
        let cancel = cancel_for_fetcher.clone();
        Box::pin(async move {
            // First call: trip the token so the worker loop's pre-pop check
            // fires on the next iteration. Returning Ok keeps us out of the
            // `Err(Cancelled)` short-circuit inside `match fetcher(...)` —
            // the cancel must be surfaced by the top-of-loop check itself,
            // which is what the plan added.
            cancel.cancel();
            Ok::<(), PeerStorageError>(())
        })
            as std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), PeerStorageError>> + Send>,
            >
    });

    let result = run_bounded_retry_pool(pending.clone(), 2, 3, fetcher, None, Some(cancel)).await;

    assert!(
        matches!(result, Some(PeerStorageError::Cancelled)),
        "expected Some(Cancelled), got {result:?}",
    );
}
