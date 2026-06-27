//! Tests for the client/download retry-pool surface.
//!
//! Plan 004 added a `cancel_token` parameter to `run_bounded_retry_pool` so
//! cancellation between pops can no longer orphan a re-queued range. This
//! test pins that behaviour.
//!
//! Plan 006 broadens coverage: success path, retry-exhaustion, on_retry hook
//! invocation count, and concurrency invariants (serial / parallel).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::peer_storage::client::run_bounded_retry_pool;
use crate::peer_storage::error::PeerStorageError;

/// Helper: type-alias-ish boxed fetcher matching `run_bounded_retry_pool`'s
/// signature so individual tests stay focused on behaviour, not type ceremony.
type Fetcher = Arc<
    dyn Fn(
            u64,
            u64,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), PeerStorageError>> + Send>,
        > + Send
        + Sync,
>;

type OnRetry = Arc<
    dyn Fn(
            (u64, u64, u32),
            &PeerStorageError,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

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

#[tokio::test]
async fn retry_pool_returns_none_when_all_ranges_succeed() {
    // Three ranges, all succeed first try → pool drains, returns None, queue empty.
    let pending = Arc::new(tokio::sync::Mutex::new(vec![
        (0u64, 100u64, 0u32),
        (100, 200, 0),
        (200, 300, 0),
    ]));
    let fetcher: Fetcher = Arc::new(|_s, _e| Box::pin(async { Ok::<(), PeerStorageError>(()) }));

    let result = run_bounded_retry_pool(pending.clone(), 2, 3, fetcher, None, None).await;

    assert!(result.is_none(), "all successes ⇒ None, got {result:?}");
    assert!(
        pending.lock().await.is_empty(),
        "pool must fully drain the queue on success"
    );
}

#[tokio::test]
async fn retry_pool_exhausts_max_retries_then_returns_error() {
    // Single range that always fails. max_retries = 2 ⇒ attempt 0, 1, 2 → on
    // attempt 2 the `attempt < max_retries` arm is false (2 < 2 is false), so
    // the error returns. Total fetcher calls = max_retries + 1 = 3.
    let pending = Arc::new(tokio::sync::Mutex::new(vec![(0u64, 100u64, 0u32)]));
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_fetcher = calls.clone();

    let fetcher: Fetcher = Arc::new(move |_s, _e| {
        let calls = calls_for_fetcher.clone();
        Box::pin(async move {
            calls.fetch_add(1, Ordering::SeqCst);
            Err::<(), PeerStorageError>(PeerStorageError::ProtocolError {
                reason: "boom".to_string(),
            })
        })
    });

    let result = run_bounded_retry_pool(pending, 1, 2, fetcher, None, None).await;

    assert!(
        matches!(result, Some(PeerStorageError::ProtocolError { .. })),
        "exhausted retries ⇒ Some(ProtocolError), got {result:?}",
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "expected attempt 0+1+2 = 3 fetcher calls before giving up"
    );
}

#[tokio::test]
async fn retry_pool_invokes_on_retry_hook_once_per_attempted_retry() {
    // Single range, max_retries = 3 ⇒ attempts 0..=3 (4 total fetcher calls).
    // The `on_retry` hook fires on attempts 0, 1, 2 (the ones that get
    // re-queued), but NOT on attempt 3 (the final, error-returning one).
    // Expect: 4 fetcher calls, 3 on_retry invocations.
    let pending = Arc::new(tokio::sync::Mutex::new(vec![(0u64, 100u64, 0u32)]));
    let fetcher_calls = Arc::new(AtomicUsize::new(0));
    let fetcher_for_closure = fetcher_calls.clone();
    let retry_calls = Arc::new(AtomicUsize::new(0));
    let retry_for_closure = retry_calls.clone();

    let fetcher: Fetcher = Arc::new(move |_s, _e| {
        let fc = fetcher_for_closure.clone();
        Box::pin(async move {
            fc.fetch_add(1, Ordering::SeqCst);
            Err::<(), PeerStorageError>(PeerStorageError::ProtocolError {
                reason: "transient".to_string(),
            })
        })
    });

    let on_retry: OnRetry = Arc::new(move |_range, _err| {
        let rc = retry_for_closure.clone();
        Box::pin(async move {
            rc.fetch_add(1, Ordering::SeqCst);
        })
    });

    let result = run_bounded_retry_pool(pending, 1, 3, fetcher, Some(on_retry), None).await;

    assert!(
        matches!(result, Some(PeerStorageError::ProtocolError { .. })),
        "exhausted retries ⇒ ProtocolError, got {result:?}",
    );
    assert_eq!(
        fetcher_calls.load(Ordering::SeqCst),
        4,
        "expected attempts 0,1,2,3 = 4 fetcher calls"
    );
    assert_eq!(
        retry_calls.load(Ordering::SeqCst),
        3,
        "on_retry must fire once per re-queue (attempts 0,1,2) — not for the \
         terminal attempt that returns the error"
    );
}

#[tokio::test]
async fn retry_pool_with_concurrency_one_processes_serially() {
    // concurrency = 1 ⇒ at no point should more than one fetcher be in flight.
    // We track in-flight count via an atomic; the test asserts the observed
    // max == 1.
    let pending = Arc::new(tokio::sync::Mutex::new(
        (0..8u64).map(|i| (i * 100, (i + 1) * 100, 0u32)).collect(),
    ));
    let inflight = Arc::new(AtomicUsize::new(0));
    let max_inflight = Arc::new(AtomicUsize::new(0));
    let inflight_for_closure = inflight.clone();
    let max_for_closure = max_inflight.clone();

    let fetcher: Fetcher = Arc::new(move |_s, _e| {
        let inflight = inflight_for_closure.clone();
        let max = max_for_closure.clone();
        Box::pin(async move {
            let now = inflight.fetch_add(1, Ordering::SeqCst) + 1;
            max.fetch_max(now, Ordering::SeqCst);
            // Yield so a hypothetical second worker could observe overlap if
            // the bound weren't honoured.
            tokio::task::yield_now().await;
            inflight.fetch_sub(1, Ordering::SeqCst);
            Ok::<(), PeerStorageError>(())
        })
    });

    let result = run_bounded_retry_pool(pending, 1, 0, fetcher, None, None).await;

    assert!(result.is_none(), "all succeeded, got {result:?}");
    assert_eq!(
        max_inflight.load(Ordering::SeqCst),
        1,
        "concurrency = 1 must serialize fetcher calls"
    );
}

#[tokio::test]
async fn retry_pool_with_concurrency_n_processes_all_ranges() {
    // concurrency = 4 ⇒ the pool may run up to 4 in-flight, but every range
    // is still completed exactly once. The invariant we pin is: every range
    // is fetched, max in-flight never exceeds `concurrency`.
    let total_ranges = 12usize;
    let concurrency = 4usize;
    let pending = Arc::new(tokio::sync::Mutex::new(
        (0..total_ranges as u64)
            .map(|i| (i * 100, (i + 1) * 100, 0u32))
            .collect(),
    ));
    let inflight = Arc::new(AtomicUsize::new(0));
    let max_inflight = Arc::new(AtomicUsize::new(0));
    let total_calls = Arc::new(AtomicUsize::new(0));
    let inflight_for_closure = inflight.clone();
    let max_for_closure = max_inflight.clone();
    let total_for_closure = total_calls.clone();

    let fetcher: Fetcher = Arc::new(move |_s, _e| {
        let inflight = inflight_for_closure.clone();
        let max = max_for_closure.clone();
        let total = total_for_closure.clone();
        Box::pin(async move {
            let now = inflight.fetch_add(1, Ordering::SeqCst) + 1;
            max.fetch_max(now, Ordering::SeqCst);
            total.fetch_add(1, Ordering::SeqCst);
            // Yield so workers actually overlap; without this we may serialize
            // by coincidence of scheduling.
            tokio::task::yield_now().await;
            inflight.fetch_sub(1, Ordering::SeqCst);
            Ok::<(), PeerStorageError>(())
        })
    });

    let result = run_bounded_retry_pool(pending, concurrency, 0, fetcher, None, None).await;

    assert!(result.is_none(), "all succeeded, got {result:?}");
    assert_eq!(
        total_calls.load(Ordering::SeqCst),
        total_ranges,
        "every range must be fetched exactly once"
    );
    assert!(
        max_inflight.load(Ordering::SeqCst) <= concurrency,
        "observed max in-flight {} > concurrency {}",
        max_inflight.load(Ordering::SeqCst),
        concurrency,
    );
}
