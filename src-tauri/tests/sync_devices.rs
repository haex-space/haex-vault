//! Integration test for spec 024 User Story 1 (T029): two real app
//! instances — the actual `SyncService`/`SyncNode`/presence/reconnect
//! wiring, not the lower-level fixtures the `sync::*` unit tests use —
//! connect on their own and exchange a change, over a real in-process
//! Nostr relay for presence discovery.
//!
//! Linux only: storage is redirected through `XDG_DATA_HOME` (see
//! `common/sync_fixture.rs`'s module docs on why this is safe to do
//! sequentially in one process).
#![cfg(target_os = "linux")]

use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

#[path = "common/sync_fixture.rs"]
mod sync_fixture;
#[path = "common/sync_helpers.rs"]
mod sync_helpers;

/// `XDG_DATA_HOME` is process-wide; tests that set it take turns (same
/// reasoning as `tests/vault_single_session.rs`).
static TURN: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Waits until `condition` holds, polling within a bound; no fixed sleep
/// decides the outcome. Rewrites `id`/`title` on `from` on every poll: a
/// single write can race the very first connection settling (iroh's own
/// address probing plus the Nostr relay handshake take a moment), which a
/// real, longer-lived session absorbs for free from later unrelated
/// commits — a test making only this one change has to redo it itself
/// instead of relying on one exactly-timed shot landing.
async fn eventually(
    from: &sync_fixture::Instance,
    id: &str,
    title: &str,
    condition: impl Fn() -> bool,
) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !condition() {
            from.write_thread(id, title).await;
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    })
    .await
    .expect("the condition holds within the bound");
}

/// US1 scenarios 1 and 2 (spec.md): two devices of the same vault find and
/// connect to each other without any manual action, and a change made on
/// one appears on the other.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_devices_connect_on_their_own_and_exchange_a_change() {
    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();

    let main = sync_fixture::create_main("main", &relay_url).await;
    let linked = sync_fixture::join(&main, &relay_url).await;

    eventually(&main, "t1", "from main", || {
        linked.thread_title("t1").as_deref() == Some("from main")
    })
    .await;

    eventually(&linked, "t2", "from linked", || {
        main.thread_title("t2").as_deref() == Some("from linked")
    })
    .await;

    tokio::join!(main.shutdown(), linked.shutdown());
}

/// US2 scenario 1 and SC-005 (spec.md): a device of another vault, running
/// the same sync service against the same Nostr relay, neither finds nor
/// receives anything of this vault, and this vault receives nothing of it.
/// (The refusals a stranger that does dial gets — foreign vault, not on the
/// list, bad signature — are `sync::handshake` unit tests, where the
/// handshake is exercised message by message.)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_device_of_another_vault_neither_gets_nor_gives_anything() {
    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();

    let main = sync_fixture::create_main("mine", &relay_url).await;
    let linked = sync_fixture::join(&main, &relay_url).await;
    let foreign = sync_fixture::create_main("theirs", &relay_url).await;
    let foreign_peer = sync_fixture::join(&foreign, &relay_url).await;
    foreign.write_thread("theirs", "not for you").await;

    // Both vaults complete a positive synchronization round before the
    // isolation assertions. This makes the negative checks observe a settled
    // exchange rather than only the initial state.
    eventually(&main, "secret", "mine only", || {
        linked.thread_title("secret").as_deref() == Some("mine only")
    })
    .await;
    eventually(&foreign, "theirs", "not for you", || {
        foreign_peer.thread_title("theirs").as_deref() == Some("not for you")
    })
    .await;
    assert_eq!(foreign.thread_title("secret"), None);
    assert_eq!(foreign_peer.thread_title("secret"), None);
    assert_eq!(main.thread_title("theirs"), None);
    assert_eq!(linked.thread_title("theirs"), None);

    tokio::join!(
        main.shutdown(),
        linked.shutdown(),
        foreign.shutdown(),
        foreign_peer.shutdown()
    );
}

/// US3 scenarios 1 to 3 (spec.md): three devices end with the same data, no
/// change is lost or doubled, and each change still names the device it
/// came from, not the one it came through.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_devices_converge_and_each_change_keeps_its_author() {
    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();

    let a = sync_fixture::create_main("main", &relay_url).await;
    let b = sync_fixture::join(&a, &relay_url).await;
    let c = sync_fixture::join(&a, &relay_url).await;
    let devices = [&a, &b, &c];
    let threads = ["from-a", "from-b", "from-c"];

    // Each device rewrites its own thread until all three have all threads
    // (the same settle race `eventually` documents).
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            for (device, id) in devices.iter().zip(threads) {
                device.write_thread(id, id).await;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
            let converged = devices
                .iter()
                .all(|d| threads.iter().all(|id| d.thread_title(id).is_some()));
            if converged {
                break;
            }
        }
    })
    .await
    .expect("all three devices hold all three threads");

    for device in devices {
        for (writer, id) in devices.iter().zip(threads) {
            assert_eq!(
                device.thread_origin(id),
                Some(writer.device_uuid()),
                "{id} names the device that wrote it"
            );
        }
    }

    tokio::join!(a.shutdown(), b.shutdown(), c.shutdown());
}

/// US4 scenarios 1, 3 and 5 (spec.md), FR-033, FR-034: a connected device is
/// online with its role, and the device view is told when that changes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_connected_device_shows_as_online_with_its_role_and_the_view_is_told() {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tauri::{Listener, Manager};

    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let main = sync_fixture::create_main("main", &relay_url).await;
    let told = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&told);
    main.app.listen("sync-devices-changed", move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    // Read before `join` points `XDG_DATA_HOME` at the other device's directory.
    let installation = sync_fixture::read_installation_uuid(&main.app);
    let linked = sync_fixture::join(&main, &relay_url).await;

    // The linked device connects on its own; the list then shows it online.
    let devices = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            main.write_thread("poke", "poke").await;
            let connected: HashSet<[u8; 32]> = main
                .app
                .state::<Arc<holzi_lib::sync::registry::SyncRegistry>>()
                .get()
                .map(|runtime| runtime.node.connected().into_iter().collect())
                .unwrap_or_default();
            let devices = holzi_lib::storage::query::read(&main.database(), |r| {
                holzi_lib::sync::device_view::load(r, installation, &connected)
            })
            .expect("load devices");
            if devices.iter().any(|d| !d.is_current && d.online) {
                break devices;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("the linked device shows as online");

    let this = devices.iter().find(|d| d.is_current).expect("this device");
    let other = devices
        .iter()
        .find(|d| !d.is_current)
        .expect("the other device");
    assert_eq!(this.role, holzi_lib::sync::device_view::DeviceRole::Main);
    assert_eq!(other.role, holzi_lib::sync::device_view::DeviceRole::Linked);
    assert_eq!(
        other.device_pubkey,
        holzi_lib::sync::keys::hex(&linked.keys.device_pubkey)
    );
    assert!(other.last_seen.is_some(), "a connected device has a time");
    assert_eq!(other.problem, None);
    // Spec 059 FR-001, FR-002, FR-005: both rows show the version state of this build, the other
    // one as its handshake with this device named it.
    let own_state = holzi_lib::sync::device_view::DeviceSchema::from(
        holzi_lib::sync::handshake::local_schema(),
    );
    assert_eq!(this.schema, Some(own_state));
    assert_eq!(other.schema, Some(own_state));
    assert!(
        told.load(Ordering::SeqCst) > 0,
        "FR-034: the device view was told a device came online"
    );

    tokio::join!(main.shutdown(), linked.shutdown());
}

/// SC-001 and SC-002 (spec.md, T079): two devices are connected within 30
/// seconds of coming up, and once connected a change shows on the other device
/// within 5 seconds in 95 % of the cases. Measured over a real in-process
/// relay and real endpoints on loopback: the times printed are those of this
/// path (relay and sockets), not of a network between homes, so the bounds are
/// the specification's, not a promise about a slow link.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn devices_connect_within_thirty_seconds_and_changes_show_within_five() {
    const CONNECT_LIMIT: Duration = Duration::from_secs(30);
    const SHOW_LIMIT: Duration = Duration::from_secs(5);
    const CHANGES: usize = 20;

    let _turn = TURN.lock().await;
    let relay = nostr_sdk::local_relay::MockRelay::run()
        .await
        .expect("mock relay");
    let relay_url = relay.url().await.to_string();
    let main = sync_fixture::create_main("main", &relay_url).await;
    let up = Instant::now();
    let linked = sync_fixture::join(&main, &relay_url).await;

    // SC-002: from the moment the second device is up to the connection.
    let main_runtime = sync_helpers::runtime_of(&main).await;
    let linked_key = linked.keys.device_pubkey;
    let connected_after = tokio::time::timeout(CONNECT_LIMIT, async {
        loop {
            if main_runtime.node.connected().contains(&linked_key) {
                return up.elapsed();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("SC-002: the devices are connected within 30 seconds");

    // SC-001: a change on one device, the time until the other shows it, both ways.
    let mut times: Vec<Duration> = Vec::new();
    for n in 0..CHANGES {
        let (from, to) = if n % 2 == 0 {
            (&main, &linked)
        } else {
            (&linked, &main)
        };
        let id = format!("timed-{n}");
        let started = Instant::now();
        from.write_thread(&id, &id).await;
        let shown = tokio::time::timeout(Duration::from_secs(30), async {
            while to.thread_title(&id).is_none() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            started.elapsed()
        })
        .await
        .expect("a change shows on the other device at all");
        times.push(shown);
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    times.sort();
    let p95 = times[(CHANGES * 95).div_ceil(100) - 1];
    println!(
        "sync_devices: connected {connected_after:?} after coming up; {CHANGES} changes shown in \
         median {:?}, p95 {p95:?}, max {:?}",
        times[CHANGES / 2],
        times[CHANGES - 1]
    );
    assert!(
        p95 <= SHOW_LIMIT,
        "SC-001: the 95th percentile is {p95:?}, over {SHOW_LIMIT:?}: {times:?}"
    );

    // The node of a runtime held here would outlive the drain.
    drop(main_runtime);
    tokio::join!(main.shutdown(), linked.shutdown());
}
