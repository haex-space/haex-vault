//! Shared SSH sessions (spec 050 research R3, R12, T067): calls reuse a session, a connection
//! holds at most two, an idle one closes after a minute, and new credentials never reuse an old
//! session.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use super::connect::connect;
use super::connect_tests::{access, password};
use super::pool::{Pool, IDLE, PER_CONNECTION};
use super::test_server::{self, Options};
use crate::remote_storage::test_support::resolver;

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[tokio::test]
async fn calls_share_sessions_up_to_two_and_new_credentials_get_their_own() {
    let server = test_server::start(Options::new(9)).await;
    let names = resolver(&[]);
    let pool = Pool::default();
    let opened = AtomicUsize::new(0);
    let target = access(
        &server.endpoint(),
        Some(&server.host_key),
        password("password"),
    );
    let open = || {
        opened.fetch_add(1, Ordering::SeqCst);
        connect(names.as_ref(), &target, soon())
    };
    let first = pool.session(&target, open).await.expect("first");
    drop(first);
    let again = pool.session(&target, open).await.expect("again");
    assert_eq!(
        opened.load(Ordering::SeqCst),
        1,
        "an idle session is reused"
    );
    let busy: Vec<_> = (0..4)
        .map(|_| async { pool.session(&target, open).await.expect("busy") })
        .collect();
    let held = futures::future::join_all(busy).await;
    assert_eq!(
        pool.open_sessions(),
        PER_CONNECTION,
        "at most two per connection"
    );
    drop((again, held));

    let mut other = target.clone();
    other.credentials = password("neu");
    let refused = pool
        .session(&other, || connect(names.as_ref(), &other, soon()))
        .await;
    assert!(
        refused.is_err(),
        "new credentials open their own session (and fail here)"
    );
}

#[tokio::test]
async fn an_idle_session_closes_after_a_minute() {
    let server = test_server::start(Options::new(10)).await;
    let names = resolver(&[]);
    let pool = Arc::new(Pool::default());
    let target = access(
        &server.endpoint(),
        Some(&server.host_key),
        password("password"),
    );
    drop(
        pool.session(&target, || connect(names.as_ref(), &target, soon()))
            .await
            .expect("session"),
    );
    assert_eq!(pool.open_sessions(), 1);
    tokio::time::pause();
    tokio::time::advance(IDLE + Duration::from_secs(2)).await;
    assert_eq!(pool.open_sessions(), 0);
}
