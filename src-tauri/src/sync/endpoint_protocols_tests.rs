//! A further protocol beside `holzi-sync/1` on the same endpoint (spec 057 T011).

use std::sync::Arc;
use std::time::Duration;

use haex_crdt::rusqlite::params;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::RelayMode;
use tokio::sync::Notify;

use super::*;
use crate::storage::query::{self, Query};
use crate::sync::endpoint::NodeConfig;
use crate::sync::test_support::Member;

const ECHO_ALPN: &[u8] = b"holzi-test-echo/1";
const LIMIT: Duration = Duration::from_secs(20);

/// Answers every bi-stream with what it read, on the node's tracker.
#[derive(Debug, Clone)]
struct Echo(NodeTasks);

impl ProtocolHandler for Echo {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        self.0.tracker.spawn(async move {
            if let Ok((mut send, mut recv)) = connection.accept_bi().await {
                if let Ok(bytes) = recv.read_to_end(64).await {
                    let _ = send.write_all(&bytes).await;
                    let _ = send.finish();
                }
                // Keeps the connection until the other side has read the answer.
                connection.closed().await;
            }
        });
        Ok(())
    }
}

async fn start(member: &Member, applied: Arc<Notify>) -> SyncNode {
    SyncNode::bind_with(
        Arc::clone(&member.device.replica),
        member.keys.clone(),
        member.vault,
        NodeConfig {
            relay_mode: RelayMode::Disabled,
            bind_addr: Some((std::net::Ipv4Addr::LOCALHOST, 0).into()),
            session: None,
        },
        Arc::new(move |_tables| applied.notify_one()),
        Protocols::default().with(ECHO_ALPN, Echo),
    )
    .await
    .expect("bind")
}

async fn echo(node: &SyncNode, to: &SyncNode, message: &[u8]) -> Vec<u8> {
    let connection = node
        .dial_alpn(to.addr(), ECHO_ALPN)
        .await
        .expect("the echo protocol accepts");
    let (mut send, mut recv) = connection.open_bi().await.expect("stream");
    send.write_all(message).await.expect("write");
    send.finish().expect("finish");
    let answer = tokio::time::timeout(LIMIT, recv.read_to_end(64))
        .await
        .expect("an answer in time")
        .expect("read");
    connection.close(0u32.into(), b"done");
    answer
}

fn title(member: &Member) -> Option<String> {
    query::read(member.device.db(), |r| {
        r.query_row(
            "SELECT title FROM chat_threads WHERE id = 't1'",
            params![],
            |row| row.get(0),
        )
    })
    .expect("read")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_further_protocol_answers_beside_sync() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    let applied = Arc::new(Notify::new());
    let a = start(&main, Arc::new(Notify::new())).await;
    let b = start(&linked, Arc::clone(&applied)).await;

    assert_eq!(echo(&b, &a, b"ping").await, b"ping");

    // holzi-sync/1 runs as before on the same endpoint.
    b.connect(a.addr()).await.expect("sync connects");
    main.device
        .db()
        .write(|tx| {
            tx.execute(
                "INSERT INTO chat_threads (id, title, created_at, updated_at) \
                 VALUES ('t1', 'beside echo', 1, 1)",
                params![],
            )?;
            Ok(())
        })
        .expect("write");
    a.local_changed();
    tokio::time::timeout(LIMIT, async {
        while title(&linked).is_none() {
            applied.notified().await;
        }
    })
    .await
    .expect("the change arrives");
    assert_eq!(echo(&a, &b, b"again").await, b"again");

    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_alpn_nobody_registered_is_refused() {
    let (one, two) = (Member::genesis(), Member::genesis());
    let a = start(&one, Arc::new(Notify::new())).await;
    let b = start(&two, Arc::new(Notify::new())).await;

    let refused = tokio::time::timeout(LIMIT, b.dial_alpn(a.addr(), b"holzi-nobody/1"))
        .await
        .expect("an answer in time");
    assert!(matches!(refused, Err(NodeError::Connect(_))), "{refused:?}");

    a.shutdown().await;
    b.shutdown().await;
}
