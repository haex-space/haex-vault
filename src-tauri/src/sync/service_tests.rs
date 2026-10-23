use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use tauri::test::{mock_builder, mock_context, noop_assets};

use super::*;
use crate::sync::test_support::Member;

/// A Tauri app on the mock runtime, to emit events into.
fn mock_app() -> AppHandle<tauri::test::MockRuntime> {
    mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock app")
        .handle()
        .clone()
}

/// Sync deps for `member`, bound to loopback with no relays at all.
fn deps(member: &Member) -> SyncDeps<tauri::test::MockRuntime> {
    SyncDeps {
        replica: Arc::clone(&member.device.replica),
        keys: member.keys.clone(),
        vault: member.vault,
        relay_mode: RelayMode::Disabled,
        nostr_relays: Vec::new(),
        bind_addr: Some((Ipv4Addr::LOCALHOST, 0).into()),
        app: mock_app(),
        peers: None,
        files: None,
    }
}

#[tokio::test]
async fn a_closing_gate_before_start_refuses() {
    let gate = VaultGate::with_runtime(tokio::runtime::Handle::current());
    gate.request_close();
    let member = Member::genesis();
    assert!(SyncService::start(&gate, deps(&member)).is_err());
}

#[tokio::test]
async fn a_closing_gate_ends_the_bound_service_within_budget() {
    let member = Member::genesis();
    let gate = VaultGate::with_runtime(tokio::runtime::Handle::current());
    gate.begin_session().expect("session");
    SyncService::start(&gate, deps(&member)).expect("start");

    gate.request_close();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !gate.is_idle() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the service ends once the close starts");
}

#[tokio::test(start_paused = true)]
async fn reconnect_passes_keep_the_least_gap() {
    let mut pace = ReconnectPace::default();
    let start = tokio::time::Instant::now();
    pace.ready().await;
    assert_eq!(
        start.elapsed(),
        Duration::ZERO,
        "the first pass runs at once"
    );
    pace.ready().await;
    assert_eq!(
        start.elapsed(),
        RECONNECT_MIN_GAP,
        "the next waits out the gap"
    );
    tokio::time::sleep(RECONNECT_MIN_GAP * 3).await;
    let later = tokio::time::Instant::now();
    pace.ready().await;
    assert_eq!(
        later.elapsed(),
        Duration::ZERO,
        "after a quiet while it runs at once"
    );
}

/// A peer that ends every session at once wakes reconnect again after each pass: in ten seconds
/// that is six passes, not the hundreds an unpaced loop made.
#[tokio::test(start_paused = true)]
async fn a_peer_ending_every_session_is_dialed_once_per_gap() {
    let wake = Notify::new();
    let mut pace = ReconnectPace::default();
    let start = tokio::time::Instant::now();
    let mut passes = 0;
    wake.notify_one();
    while start.elapsed() < Duration::from_secs(10) {
        wake.notified().await;
        pace.ready().await;
        passes += 1;
        // The session this pass opened ends at once, and the end wakes reconnect.
        wake.notify_one();
    }
    assert_eq!(passes, 6);
}
