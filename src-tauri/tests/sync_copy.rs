//! Integration test for spec 024 User Story 7 (T071): the copy of a vault
//! file. Real app instances with their running sync service, over a real
//! in-process Nostr relay for presence and requests and real iroh endpoints
//! on loopback for the sync.
//!
//! A copy of a main device's file enrolls itself as a main device and finds
//! its source through its presence, although the source was the only device
//! of the vault and publishes nothing itself (scenarios 1, 2 and 6, SC-002,
//! SC-018). A copy of a linked device's file sends a request and sends and
//! receives nothing until a main device admits it (scenarios 3 to 5, SC-018).
//!
//! Linux only, like the other sync tests (storage is redirected through
//! `XDG_DATA_HOME`).
#![cfg(target_os = "linux")]

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use haex_crdt::rusqlite::params;
use tokio::sync::Mutex;

use holzi_lib::storage::query::{self, Query};
use holzi_lib::sync::commands::{decide_admission_now, this_device, ThisDevice};
use holzi_lib::sync::{admission, device_list};

#[path = "common/sync_fixture.rs"]
mod sync_fixture;
#[path = "common/sync_helpers.rs"]
mod sync_helpers;

use sync_fixture::Instance;
use sync_helpers::{listed, runtime_of, until};

/// `XDG_DATA_HOME` is process-wide; tests that set it take turns.
static TURN: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Reads the current device's role from its persisted sync state.
fn place_of(device: &Instance) -> ThisDevice {
    query::read(&device.database(), |r| {
        this_device(r, &device.keys.device_pubkey)
    })
    .expect("read this device's place")
}

/// The open requests `device` holds for devices the list has not settled.
fn open_requests(device: &Instance) -> Vec<admission::OpenRequest> {
    query::read(&device.database(), |r| {
        let valid = device_list::valid_lists(&device_list::load_all(r)?, &device.vault);
        let settled = device_list::effective(&valid)
            .map(admission::settled)
            .unwrap_or_default();
        admission::load_open(r, &settled)
    })
    .expect("read requests")
}

/// Keeps writing `id` on `writer` until `reader` has it, through the real
/// sessions.
async fn reaches(writer: &Instance, reader: &Instance, id: &str, what: &str) {
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            writer.write_thread(id, id).await;
            if reader.thread_title(id).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out: {what}"));
}

/// A main device and a linked one that already sync with each other.
async fn main_and_linked(relay_url: &str) -> (Instance, Instance) {
    let main = sync_fixture::create_main("main", relay_url).await;
    let linked = sync_fixture::join(&main, relay_url).await;
    reaches(&main, &linked, "warm-up", "main and linked device sync").await;
    (main, linked)
}

/// Scenarios 1, 2 and 6, SC-002, SC-018: the copy of the only device's file
/// has keys of its own, lists itself as a main device, and is found by the
/// source through its presence; from then on both sync, and what the copy
/// writes carries its own node id.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_of_a_main_devices_file_becomes_a_main_device_and_syncs_with_its_source() {
    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let source = sync_fixture::create_main("main", &relay_url).await;
    source.write_thread("before-copy", "before-copy").await;

    let copy_up = Instant::now();
    let copy = sync_fixture::copy_of(&source, &relay_url).await;

    // Scenario 1: its own keys, the source's left alone in the file.
    assert_ne!(copy.keys.device_pubkey, source.keys.device_pubkey);
    assert_ne!(copy.keys.endpoint_id, source.keys.endpoint_id);
    let source_row: Option<Vec<u8>> = query::read(&copy.database(), |r| {
        r.query_row(
            "SELECT device_secret FROM device_keys_no_sync WHERE device_pubkey = ?1",
            params![source.keys.device_pubkey.as_slice()],
            |row| row.get(0),
        )
    })
    .expect("read the source's row");
    assert_eq!(
        source_row.as_deref(),
        Some(source.keys.device_secret.as_slice()),
        "the source's key row is unchanged in the copy"
    );
    // Scenario 2: a main device on a new list, without doing anything, and
    // holding what the file held.
    assert_eq!(place_of(&copy), ThisDevice::Main);
    assert_eq!(
        listed(&copy),
        vec![
            (source.keys.device_pubkey, true),
            (copy.keys.device_pubkey, true)
        ]
    );
    assert!(copy.thread_title("before-copy").is_some());

    // Scenario 6 and SC-002: the source had no peer and published nothing; the
    // copy's presence brings it a newer list, and the two connect, within 30
    // seconds of the copy coming up.
    until("the source to take over the copy's list", || {
        listed(&source)
            .iter()
            .any(|(device, _)| *device == copy.keys.device_pubkey)
            .then_some(())
    })
    .await;
    reaches(
        &copy,
        &source,
        "from-copy",
        "the copy's change reaches the source",
    )
    .await;
    let found_after = copy_up.elapsed();
    println!("sync_copy: the source found the copy and synced {found_after:?} after it came up");
    assert!(
        found_after <= Duration::from_secs(30),
        "SC-002: the source took {found_after:?} to find its copy"
    );
    reaches(
        &source,
        &copy,
        "from-source",
        "the source's change reaches the copy",
    )
    .await;
    assert_eq!(
        source.thread_origin("from-copy"),
        Some(copy.device_uuid()),
        "the copy writes under a node id of its own"
    );
    assert_ne!(copy.device_uuid(), source.device_uuid());

    tokio::join!(source.shutdown(), copy.shutdown());
}

/// Scenarios 3 and 4, SC-018: the copy of a linked device's file asks, sends
/// and receives nothing while it waits, and after "Aufnehmen" everything it
/// wrote in the meantime reaches the others.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_of_a_linked_devices_file_waits_until_a_main_device_admits_it() {
    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let (main, linked) = main_and_linked(&relay_url).await;

    let copy = sync_fixture::copy_of(&linked, &relay_url).await;

    // Scenario 3: a linked copy does not enroll; it waits.
    assert_eq!(place_of(&copy), ThisDevice::AwaitingAdmission);
    assert_eq!(listed(&copy).len(), 2, "its list is still the source's");
    copy.write_thread("while-waiting", "while-waiting").await;
    main.write_thread("while-the-copy-waits", "while-the-copy-waits")
        .await;
    let request = until("the request to reach the main device", || {
        open_requests(&main)
            .into_iter()
            .find(|r| r.device == copy.keys.device_pubkey)
    })
    .await;
    assert!(!request.name.is_empty(), "the request carries a name");
    // The request went through the relay, so the copy is up and looking.
    // Nothing flows either way while it is not admitted.
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        main.thread_title("while-waiting").is_none(),
        "the copy sent a change before it was admitted"
    );
    assert!(
        copy.thread_title("while-the-copy-waits").is_none(),
        "the copy received a change before it was admitted"
    );

    // Scenario 4: "Aufnehmen".
    decide_admission_now(&runtime_of(&main).await, copy.keys.device_pubkey, true)
        .await
        .expect("the main device admits the copy");
    until(
        "the copy's change from the waiting time to reach the main device",
        || main.thread_title("while-waiting"),
    )
    .await;
    until("the main device's change to reach the copy", || {
        copy.thread_title("while-the-copy-waits")
    })
    .await;
    assert_eq!(
        main.thread_origin("while-waiting"),
        Some(copy.device_uuid()),
        "what it wrote while it waited carries its node id"
    );
    assert_eq!(place_of(&copy), ThisDevice::Linked);
    assert!(
        open_requests(&main).is_empty(),
        "an answered request is gone"
    );
    reaches(
        &copy,
        &linked,
        "after-admission",
        "the copy syncs with every device",
    )
    .await;

    tokio::join!(main.shutdown(), linked.shutdown(), copy.shutdown());
}

/// Scenario 4, "Ablehnen": the copy stays outside.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_refused_copy_stays_outside() {
    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let (main, linked) = main_and_linked(&relay_url).await;
    let copy = sync_fixture::copy_of(&linked, &relay_url).await;
    until("the request to reach the main device", || {
        open_requests(&main)
            .into_iter()
            .find(|r| r.device == copy.keys.device_pubkey)
    })
    .await;

    decide_admission_now(&runtime_of(&main).await, copy.keys.device_pubkey, false)
        .await
        .expect("the main device refuses the copy");

    assert!(
        open_requests(&main).is_empty(),
        "a refused request is not offered again"
    );
    assert_eq!(listed(&main).len(), 2, "nobody was admitted");
    main.write_thread("after-refusal", "after-refusal").await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(copy.thread_title("after-refusal").is_none());
    assert_eq!(place_of(&copy), ThisDevice::AwaitingAdmission);

    tokio::join!(main.shutdown(), linked.shutdown(), copy.shutdown());
}

/// Scenario 5: the request reached a linked device, which is not a main
/// device and cannot decide; when it later syncs with a main device, the
/// request is there too, and the main device can admit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_request_that_reached_a_linked_device_reaches_a_main_device_with_its_next_sync() {
    let _turn = TURN.lock().await;
    let main_relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let other_relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let main = sync_fixture::create_main("main", &main_relay.url().await.to_string()).await;
    // The linked device and its copy share a relay the main device does not
    // use: only the linked device hears the copy.
    let other_url = other_relay.url().await.to_string();
    let linked = sync_fixture::join(&main, &other_url).await;
    let copy = sync_fixture::copy_of(&linked, &other_url).await;

    until("the request to reach the linked device", || {
        open_requests(&linked)
            .into_iter()
            .find(|r| r.device == copy.keys.device_pubkey)
    })
    .await;
    assert!(
        open_requests(&main).is_empty(),
        "the main device heard nothing yet"
    );
    let refused =
        decide_admission_now(&runtime_of(&linked).await, copy.keys.device_pubkey, true).await;
    assert!(
        matches!(refused, Err(holzi_lib::error::HaexError::NotMainDevice)),
        "a linked device cannot decide"
    );

    // The next sync of the two.
    sync_fixture::pull_all(&linked, &main.database());
    let request = open_requests(&main)
        .into_iter()
        .find(|r| r.device == copy.keys.device_pubkey)
        .expect("the request reached the main device with the sync");
    assert!(!request.name.is_empty());
    decide_admission_now(&runtime_of(&main).await, copy.keys.device_pubkey, true)
        .await
        .expect("the main device admits the copy");
    assert!(listed(&main)
        .iter()
        .any(|(device, is_main)| *device == copy.keys.device_pubkey && !is_main));

    tokio::join!(main.shutdown(), linked.shutdown(), copy.shutdown());
}
