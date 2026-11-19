//! Integration test for spec 024 User Story 6 (T066): a main device removes a
//! device; the removed one gets nothing new while the others keep syncing.
//!
//! Linux only, like the other sync tests (storage is redirected through
//! `XDG_DATA_HOME`).
#![cfg(target_os = "linux")]

use std::sync::LazyLock;
use std::time::Duration;

use tokio::sync::Mutex;

#[path = "common/sync_fixture.rs"]
mod sync_fixture;
#[path = "common/sync_helpers.rs"]
mod sync_helpers;

use sync_helpers::{listed, runtime_of, until};

/// `XDG_DATA_HOME` is process-wide; tests that set it take turns.
static TURN: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Who `device` has a session with, named by the devices of `all` (A, B, C), for a failure message.
async fn describe_peers(
    device: &sync_fixture::Instance,
    label: &str,
    all: [&sync_fixture::Instance; 3],
) -> String {
    let runtime = runtime_of(device).await;
    let names = ["A", "B", "C"];
    let peers: Vec<&str> = runtime
        .node
        .connected()
        .into_iter()
        .map(|key| {
            all.iter()
                .position(|d| d.keys.device_pubkey == key)
                .map_or("?", |i| names[i])
        })
        .collect();
    format!("{label} is connected to {peers:?}")
}

/// US6, SC-010 (spec.md): with three devices, the main device removes one. The
/// removed device gets nothing new any more, the other two keep syncing, and
/// both lists no longer name it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_removed_device_gets_nothing_new_while_the_others_keep_syncing() {
    use holzi_lib::sync::commands::{remove_device_now, this_device, ThisDevice};

    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let a = sync_fixture::create_main("main", &relay_url).await;
    let b = sync_fixture::join(&a, &relay_url).await;
    let c = sync_fixture::join(&a, &relay_url).await;

    // All three find each other and sync: the removal is of a working device.
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            a.write_thread("warm-up", "warm-up").await;
            if b.thread_title("warm-up").is_some() && c.thread_title("warm-up").is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    })
    .await
    .expect("all three devices sync before the removal");

    let runtime = runtime_of(&a).await;
    remove_device_now(&runtime, b.keys.device_pubkey)
        .await
        .expect("the main device removes the linked device");

    // What the main device writes now reaches the device that stays; the
    // removed one, which would have got it a moment ago, does not.
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            a.write_thread("after-removal", "after-removal").await;
            if c.thread_title("after-removal").is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    })
    .await
    .expect("the remaining device keeps syncing");
    if b.thread_title("after-removal").is_some() {
        panic!(
            "FR-027 broken: B got after-removal. {} | {} | {} | A lists {} | B lists {} | C lists {}",
            describe_peers(&a, "A", [&a, &b, &c]).await,
            describe_peers(&b, "B", [&a, &b, &c]).await,
            describe_peers(&c, "C", [&a, &b, &c]).await,
            listed(&a).len(),
            listed(&b).len(),
            listed(&c).len(),
        );
    }
    assert_eq!(listed(&a).len(), 2);
    let c_sees = until("the remaining device to learn the new list", || {
        let devices = listed(&c);
        (devices.len() == 2).then_some(devices)
    })
    .await;
    assert!(
        c_sees
            .iter()
            .all(|(device, _)| *device != b.keys.device_pubkey),
        "FR-034: the removed device is off the list of the others"
    );

    // The removed device learns of it the next time it reaches one that knows: its reconnect
    // dials the devices it still lists, and the refusal brings the new list along (FR-034). The
    // service does that on a 30 second tick; the test runs the same step instead of waiting.
    let removed_runtime = runtime_of(&b).await;
    let removed = tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            holzi_lib::sync::reconnect_missing(&removed_runtime.node, &removed_runtime.replica);
            let place = holzi_lib::storage::query::read(&b.database(), |r| {
                this_device(r, &b.keys.device_pubkey)
            })
            .expect("read place");
            if place == ThisDevice::Removed {
                break place;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        let rows = holzi_lib::storage::query::read(&b.database(), |r| {
            holzi_lib::sync::presence::load_all(r)
        })
        .expect("read presence");
        let names = [a.keys.device_pubkey, b.keys.device_pubkey, c.keys.device_pubkey];
        let known: Vec<String> = rows
            .iter()
            .map(|row| {
                let who = ["A", "B", "C"][names.iter().position(|k| *k == row.device_pubkey).unwrap_or(0)];
                format!("{who}: address {}", row.endpoint_addr.is_some())
            })
            .collect();
        panic!(
            "the removed device did not learn that it was removed: B lists {} devices; {}; B knows {known:?}",
            listed(&b).len(),
            "see above",
        )
    });
    assert_eq!(removed, ThisDevice::Removed);

    // The node of a runtime held here would outlive the drain.
    drop((runtime, removed_runtime));
    tokio::join!(a.shutdown(), b.shutdown(), c.shutdown());
}

/// US6 scenario 9 at the backend: a linked device cannot remove anything,
/// and no device removes itself.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_a_main_device_removes_and_never_itself() {
    use holzi_lib::sync::commands::remove_device_now;

    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let a = sync_fixture::create_main("main", &relay_url).await;
    let b = sync_fixture::join(&a, &relay_url).await;

    let main_runtime = runtime_of(&a).await;
    let itself = remove_device_now(&main_runtime, a.keys.device_pubkey).await;
    assert!(matches!(
        itself,
        Err(holzi_lib::error::HaexError::InvalidInput { .. })
    ));

    let linked_runtime = runtime_of(&b).await;
    let refused = remove_device_now(&linked_runtime, a.keys.device_pubkey).await;
    assert!(matches!(
        refused,
        Err(holzi_lib::error::HaexError::NotMainDevice)
    ));
    assert_eq!(listed(&a).len(), 2, "nothing was removed");

    // The node of a runtime held here would outlive the drain.
    drop((main_runtime, linked_runtime));
    tokio::join!(a.shutdown(), b.shutdown());
}
