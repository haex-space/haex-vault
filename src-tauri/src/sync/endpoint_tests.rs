use std::time::Duration;

use haex_crdt::rusqlite::params;
use tokio::sync::Notify;

use super::*;
use crate::storage::query::{self, Query};
use crate::sync::test_support::Member;

/// How long a test waits for an expected event.
const EVENT_LIMIT: Duration = Duration::from_secs(20);

struct Node {
    node: SyncNode,
    applied: Arc<Notify>,
}

async fn start(member: &Member) -> Node {
    let applied = Arc::new(Notify::new());
    let signal = Arc::clone(&applied);
    let node = SyncNode::bind(
        Arc::clone(&member.device.replica),
        member.keys.clone(),
        member.vault,
        NodeConfig {
            relay_mode: RelayMode::Disabled,
            bind_addr: Some((std::net::Ipv4Addr::LOCALHOST, 0).into()),
            session: None,
        },
        Arc::new(move |_tables| signal.notify_one()),
    )
    .await
    .expect("bind");
    Node { node, applied }
}

fn write_thread(member: &Member, id: &str, title: &str) {
    member
        .device
        .db()
        .write(|tx| {
            tx.execute(
                "INSERT INTO chat_threads (id, title, created_at, updated_at) VALUES (?1, ?2, 1, 1) \
                 ON CONFLICT(id) DO UPDATE SET title = excluded.title",
                params![id, title],
            )?;
            Ok(())
        })
        .expect("write thread");
}

fn title(member: &Member, id: &str) -> Option<String> {
    query::read(member.device.db(), |r| {
        r.query_row(
            "SELECT title FROM chat_threads WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
    })
    .expect("read title")
}

/// Waits for `applied` until `done` holds, at most [`EVENT_LIMIT`].
async fn wait_for(applied: &Notify, done: impl Fn() -> bool) {
    tokio::time::timeout(EVENT_LIMIT, async {
        while !done() {
            applied.notified().await;
        }
    })
    .await
    .expect("the change arrives in time");
}

fn pair() -> (Member, Member) {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    (main, linked)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_devices_exchange_changes_in_both_directions() {
    let (main, linked) = pair();
    let a = start(&main).await;
    let b = start(&linked).await;
    b.node.connect(a.node.addr()).await.expect("connect");

    write_thread(&main, "t1", "from main");
    a.node.local_changed();
    wait_for(&b.applied, || {
        title(&linked, "t1").as_deref() == Some("from main")
    })
    .await;

    write_thread(&linked, "t2", "from linked");
    b.node.local_changed();
    wait_for(&a.applied, || {
        title(&main, "t2").as_deref() == Some("from linked")
    })
    .await;

    tokio::time::timeout(Duration::from_secs(5), async {
        a.node.shutdown().await;
        b.node.shutdown().await;
    })
    .await
    .expect("the nodes shut down within the session-end limits");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn changes_made_before_connecting_arrive_on_connect() {
    let (main, linked) = pair();
    write_thread(&main, "offline", "written while apart");
    let a = start(&main).await;
    let b = start(&linked).await;

    b.node.connect(a.node.addr()).await.expect("connect");

    wait_for(&b.applied, || title(&linked, "offline").is_some()).await;
    a.node.shutdown().await;
    b.node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_devices_dialing_each_other_keep_one_connection() {
    let (main, linked) = pair();
    let a = start(&main).await;
    let b = start(&linked).await;

    let (ab, ba) = tokio::join!(a.node.connect(b.node.addr()), b.node.connect(a.node.addr()));
    ab.expect("a dials");
    ba.expect("b dials");
    write_thread(&main, "t1", "x");
    a.node.local_changed();
    wait_for(&b.applied, || title(&linked, "t1").is_some()).await;
    write_thread(&linked, "t2", "y");
    b.node.local_changed();
    wait_for(&a.applied, || title(&main, "t2").is_some()).await;

    assert_eq!(a.node.connected(), vec![linked.keys.device_pubkey]);
    assert_eq!(b.node.connected(), vec![main.keys.device_pubkey]);
    a.node.shutdown().await;
    b.node.shutdown().await;
}

/// Forwards UDP between a dialer and `target`, keeping every datagram it
/// sees in both directions, like someone reading the network. Returns the
/// address to dial instead of `target` and the record.
async fn sniffing_proxy(
    target: std::net::SocketAddr,
) -> (std::net::SocketAddr, Arc<Mutex<Vec<u8>>>) {
    use tokio::net::UdpSocket;

    let record = Arc::new(Mutex::new(Vec::new()));
    let front = Arc::new(
        UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind"),
    );
    let back = Arc::new(
        UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind"),
    );
    back.connect(target).await.expect("connect upstream");
    let client = Arc::new(Mutex::new(None));
    let address = front.local_addr().expect("address");

    let (front_in, back_out, seen, from) = (
        Arc::clone(&front),
        Arc::clone(&back),
        Arc::clone(&record),
        Arc::clone(&client),
    );
    tokio::spawn(async move {
        let mut buffer = vec![0u8; 65536];
        while let Ok((n, source)) = front_in.recv_from(&mut buffer).await {
            *from.lock().expect("lock") = Some(source);
            seen.lock().expect("lock").extend_from_slice(&buffer[..n]);
            let _ = back_out.send(&buffer[..n]).await;
        }
    });
    let (front_out, back_in, seen, to) = (front, back, Arc::clone(&record), client);
    tokio::spawn(async move {
        let mut buffer = vec![0u8; 65536];
        while let Ok(n) = back_in.recv(&mut buffer).await {
            seen.lock().expect("lock").extend_from_slice(&buffer[..n]);
            let destination = *to.lock().expect("lock");
            if let Some(destination) = destination {
                let _ = front_out.send_to(&buffer[..n], destination).await;
            }
        }
    });
    (address, record)
}

/// SC-006, FR-002 (quickstart A9): what crosses the network between two own
/// devices shows no table or column name, no content, no device key and no
/// private key of the vault identity.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_traffic_between_two_devices_shows_nothing_in_the_clear() {
    let (main, linked) = pair();
    let a = start(&main).await;
    let b = start(&linked).await;
    let addr = a.node.addr();
    let target = addr.ip_addrs().next().copied().expect("a direct address");
    let (proxy, record) = sniffing_proxy(target).await;

    b.node
        .connect(EndpointAddr::from_parts(
            addr.id,
            [iroh::TransportAddr::Ip(proxy)],
        ))
        .await
        .expect("connect through the proxy");
    write_thread(&main, "t1", "a-distinctive-thread-title-4711");
    a.node.local_changed();
    wait_for(&b.applied, || {
        title(&linked, "t1").as_deref() == Some("a-distinctive-thread-title-4711")
    })
    .await;
    write_thread(&linked, "t2", "another-distinctive-title-0815");
    b.node.local_changed();
    wait_for(&a.applied, || {
        title(&main, "t2").as_deref() == Some("another-distinctive-title-0815")
    })
    .await;

    let vault_secret = query::read(main.device.db(), |r| crate::sync::keys::vault_secret(r))
        .expect("read")
        .expect("main holds the vault secret");
    let seen = record.lock().expect("lock").clone();
    assert!(
        seen.len() > 2000,
        "the proxy saw the exchange ({} bytes)",
        seen.len()
    );
    let contains = |needle: &[u8]| seen.windows(needle.len()).any(|w| w == needle);
    for (what, needle) in [
        ("a table name", b"chat_threads".as_slice()),
        ("a column name", b"updated_at".as_slice()),
        ("a title", b"distinctive".as_slice()),
        ("the protocol's own name", b"haex-sync".as_slice()),
        ("the main device's key", main.keys.device_pubkey.as_slice()),
        (
            "the linked device's key",
            linked.keys.device_pubkey.as_slice(),
        ),
        ("the vault's private key", vault_secret.as_slice()),
    ] {
        assert!(!contains(needle), "{what} crossed the network in the clear");
    }
    a.node.shutdown().await;
    b.node.shutdown().await;
}

/// R14, FR-030: a copy of a listed device that dials in from another
/// endpoint gets refused, and the device it copies is marked as duplicated.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_of_a_listed_device_marks_it_as_duplicated() {
    use crate::sync::problems::{self, Problem};

    let (main, linked) = pair();
    let a = start(&main).await;
    let changed = Arc::new(Notify::new());
    let signal = Arc::clone(&changed);
    a.node
        .on_devices_changed(Arc::new(move || signal.notify_one()));
    // The copy has the device key of `linked`, but an endpoint of its own.
    let elsewhere = crate::sync::keys::DeviceKeys::generate();
    let mut copy_keys = linked.keys.clone();
    copy_keys.endpoint_secret = elsewhere.endpoint_secret.clone();
    copy_keys.endpoint_id = elsewhere.endpoint_id;
    let copy = SyncNode::bind(
        Arc::clone(&linked.device.replica),
        copy_keys,
        linked.vault,
        NodeConfig {
            relay_mode: RelayMode::Disabled,
            bind_addr: Some((std::net::Ipv4Addr::LOCALHOST, 0).into()),
            session: None,
        },
        Arc::new(|_| {}),
    )
    .await
    .expect("bind the copy");

    copy.connect(a.node.addr()).await.expect("dial");

    tokio::time::timeout(EVENT_LIMIT, changed.notified())
        .await
        .expect("the problem is announced");
    let recorded = query::read(main.device.db(), |r| {
        problems::of(r, &linked.keys.device_pubkey)
    })
    .expect("read");
    assert_eq!(recorded, Some(Problem::Duplicate));
    assert!(a.node.connected().is_empty(), "no session with a duplicate");
    a.node.shutdown().await;
    copy.shutdown().await;
}

/// FR-027: a removed device that never pulls still loses its session once
/// this device knows of the removal. The stand-in speaks the handshake as the
/// listed device, then only reads the control stream: it never pulls, so only
/// the progress side of the session can end it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_removed_device_that_never_pulls_loses_its_session() {
    use crate::sync::wire::{expect_frame, read_frame, Message, FRAME_LIMIT};

    let (main, linked) = pair();
    let a = start(&main).await;
    let endpoint = Endpoint::builder(presets::Minimal)
        .secret_key(SecretKey::from_bytes(&linked.keys.endpoint_secret))
        .relay_mode(RelayMode::Disabled)
        .clear_ip_transports()
        .bind_addr((std::net::Ipv4Addr::LOCALHOST, 0))
        .expect("bind address")
        .bind()
        .await
        .expect("bind the stand-in");
    let connection = endpoint
        .connect(a.node.addr(), SYNC_ALPN)
        .await
        .expect("dial");
    let (mut send, mut recv) = connection.accept_bi().await.expect("handshake stream");
    handshake::dial(
        &mut send,
        &mut recv,
        &linked.device.replica,
        &linked.local(),
        *connection.remote_id().as_bytes(),
    )
    .await
    .expect("handshake");
    let first = expect_frame(&mut recv, FRAME_LIMIT).await.expect("a frame");
    assert!(
        matches!(first, Message::Progress { .. }),
        "the session runs"
    );
    assert_eq!(a.node.connected(), vec![linked.keys.device_pubkey]);

    crate::sync::removal::remove_device(
        &main.device.replica,
        &main.keys,
        &linked.keys.device_pubkey,
        5_000,
    )
    .expect("removed");
    a.node.local_changed();

    // The removal moved this device's progress, yet the removed device gets
    // no `Progress` with it: the control stream ends without another frame.
    let after = tokio::time::timeout(EVENT_LIMIT, async {
        let mut frames = Vec::new();
        while let Ok(Some(frame)) = read_frame(&mut recv, FRAME_LIMIT).await {
            frames.push(frame);
        }
        frames
    })
    .await
    .expect("the session ends");
    assert!(
        !after.iter().any(|m| matches!(m, Message::Progress { .. })),
        "no progress reaches a removed device"
    );
    let closed = connection.closed().await;
    let iroh::endpoint::ConnectionError::ApplicationClosed(close) = closed else {
        panic!("closed by the other side, got {closed:?}");
    };
    assert_eq!(close.error_code, ErrorCode::Rejected.as_u32().into());
    assert!(a.node.connected().is_empty());
    a.node.shutdown().await;
    endpoint.close().await;
}
