//! Nostr-Relays that drop, reorder and replay (spec 095 T025, quickstart scenario 6, SC-004,
//! SC-005, SC-009): one relay accepts every wrap and withholds it, the other hands wraps out in
//! reverse order and twice. Sixty messages with a restart of sender and recipient in between arrive
//! once and in order; _delivered_ shows only after the recipient's receipt; and a message every
//! relay withholds for days stays in the outbox, is encrypted and published again after 1 day,
//! 3 days and 7 days, and arrives once a relay delivers.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use haex_crdt::rusqlite::params;
use nostr::event::Kind;
use nostr::filter::Filter;
use nostr::message::MachineReadablePrefix;
use nostr_sdk::local_relay::{
    LocalRelay, LocalRelayBuilder, QueryPolicy, QueryPolicyResult, RateLimit,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

use super::messages::{DeliveryState, PeerMessage};
use super::test_support::{two_vaults, ChatDevice};
use crate::storage::query::Query;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// How long the waits for sixty messages may take.
const SIXTY: Duration = Duration::from_secs(90);

/// Refuses subscriptions to wraps while `withhold` is set; chat key records stay readable.
#[derive(Debug)]
struct Withhold(Arc<AtomicBool>);

impl QueryPolicy for Withhold {
    fn admit_query<'a>(
        &'a self,
        query: &'a mut Filter,
        _addr: &'a std::net::SocketAddr,
    ) -> Pin<Box<dyn Future<Output = QueryPolicyResult> + Send + 'a>> {
        let wraps = query
            .kinds
            .as_ref()
            .is_none_or(|kinds| kinds.contains(&Kind::GiftWrap));
        let refuse = wraps && self.0.load(Ordering::SeqCst);
        Box::pin(async move {
            if refuse {
                QueryPolicyResult::reject(MachineReadablePrefix::Restricted, "withheld")
            } else {
                QueryPolicyResult::Accept
            }
        })
    }
}

/// A Nostr-Relay in the process that accepts every wrap and serves none while `withhold` is set.
async fn withholding_relay(withhold: Arc<AtomicBool>) -> (LocalRelay, String) {
    let relay = LocalRelayBuilder::default()
        .rate_limit(RateLimit {
            max_reqs: 500,
            notes_per_minute: 1_000_000,
        })
        .messages_per_minute(1_000_000)
        .query_policy(Withhold(withhold))
        .build();
    relay.run().await.expect("test relay");
    let url = relay.url().await.to_string();
    (relay, url)
}

/// A Nostr-Relay in front of `upstream` that holds the events it sends until the end of the stored
/// ones or a pause of 300 ms, then hands them out in reverse order, twice.
async fn reversing_relay(upstream: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listen");
    let url = format!("ws://{}", listener.local_addr().expect("address"));
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(reverse(stream, upstream.clone()));
        }
    });
    url
}

async fn reverse(stream: TcpStream, upstream: String) {
    let Ok(client) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    let address = upstream
        .trim_start_matches("ws://")
        .trim_end_matches('/')
        .to_owned();
    let Ok(tcp) = TcpStream::connect(address).await else {
        return;
    };
    let Ok((relay, _)) = tokio_tungstenite::client_async(upstream.as_str(), tcp).await else {
        return;
    };
    let (mut to_client, mut from_client) = client.split();
    let (mut to_relay, mut from_relay) = relay.split();
    let up = async move {
        while let Some(Ok(message)) = from_client.next().await {
            if to_relay.send(message).await.is_err() {
                return;
            }
        }
    };
    let down = async move {
        let mut held: Vec<Message> = Vec::new();
        loop {
            match tokio::time::timeout(Duration::from_millis(300), from_relay.next()).await {
                Err(_) => {
                    if flush(&mut held, &mut to_client).await.is_err() {
                        return;
                    }
                }
                Ok(Some(Ok(message))) => {
                    let event = matches!(&message, Message::Text(text)
                        if text.as_str().starts_with("[\"EVENT\""));
                    if event {
                        held.push(message);
                        continue;
                    }
                    if flush(&mut held, &mut to_client).await.is_err()
                        || to_client.send(message).await.is_err()
                    {
                        return;
                    }
                }
                Ok(_) => return,
            }
        }
    };
    tokio::select! {
        () = up => {}
        () = down => {}
    }
}

async fn flush<S>(held: &mut Vec<Message>, to: &mut S) -> Result<(), S::Error>
where
    S: futures::Sink<Message> + Unpin,
{
    for _ in 0..2 {
        for message in held.iter().rev() {
            to.send(message.clone()).await?;
        }
    }
    held.clear();
    Ok(())
}

fn texts(messages: &[PeerMessage]) -> Vec<String> {
    messages.iter().filter_map(|m| m.text.clone()).collect()
}

fn state_of(messages: &[PeerMessage], text: &str) -> Option<DeliveryState> {
    messages
        .iter()
        .find(|m| m.text.as_deref() == Some(text))
        .and_then(|m| m.delivery)
}

/// The `resends` of `device`'s outbox row of a message; `None` when it has none.
fn resends(device: &ChatDevice) -> Option<i64> {
    device
        .db
        .read_blocking(|r| {
            r.query_row(
                "SELECT resends FROM chat_outbox_no_sync WHERE message_id IS NOT NULL",
                params![],
                |row| row.get::<_, i64>(0),
            )
        })
        .expect("outbox")
}

/// Moves `device`'s clock on by `ms`, wakes its outbox and waits until its message row was sent
/// again `count` times.
async fn resent_after(device: &ChatDevice, ms: i64, count: i64) {
    device.clock.advance(ms);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while resends(device) != Some(count)
        || device
            .db
            .read_blocking(|r| {
                r.query_row(
                    "SELECT attempts FROM chat_outbox_no_sync WHERE message_id IS NOT NULL",
                    params![],
                    |row| row.get::<_, i64>(0),
                )
            })
            .expect("outbox")
            .unwrap_or(0)
            == 0
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "not sent again a {count}. time: {:?}",
            resends(device)
        );
        device.session.kick.notify_one();
        tokio::task::yield_now().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn messages_through_relays_that_withhold_reorder_and_replay_arrive_once_and_in_order() {
    let withhold_first = Arc::new(AtomicBool::new(true));
    let withhold_second = Arc::new(AtomicBool::new(false));
    let (_first, first_url) = withholding_relay(Arc::clone(&withhold_first)).await;
    let (_second, second_url) = withholding_relay(Arc::clone(&withhold_second)).await;
    let reversing_url = reversing_relay(second_url).await;
    let (anna, ben) = two_vaults(&[first_url, reversing_url]).await;
    ben.start().await;
    anna.start().await;

    let all: Vec<String> = (1..=60).map(|n| format!("Nachricht {n}")).collect();
    for text in &all[..30] {
        anna.clock.advance(1_000);
        anna.send(&ben, text);
    }
    ben.wait_within(SIXTY, &anna, "the first thirty", |m| m.len() >= 30)
        .await;
    anna.stop();
    ben.stop();
    anna.start().await;
    ben.start().await;
    for text in &all[30..] {
        anna.clock.advance(1_000);
        anna.send(&ben, text);
    }
    let at_ben = ben
        .wait_within(SIXTY, &anna, "all sixty", |m| m.len() >= 60)
        .await;
    assert_eq!(texts(&at_ben), all, "each once and in order");
    anna.wait_within(SIXTY, &ben, "all delivered", |m| {
        m.iter()
            .all(|m| m.delivery == Some(DeliveryState::Delivered))
    })
    .await;

    // Now every relay withholds wraps from Ben.
    ben.stop();
    withhold_second.store(true, Ordering::SeqCst);
    anna.send(&ben, "Zurückgehalten");
    anna.wait_for(&ben, "accepted", |m| {
        state_of(m, "Zurückgehalten") == Some(DeliveryState::Sent)
    })
    .await;
    ben.start().await;
    resent_after(&anna, DAY_MS, 1).await;
    resent_after(&anna, 3 * DAY_MS, 2).await;
    resent_after(&anna, 7 * DAY_MS, 3).await;
    assert_eq!(
        state_of(&anna.messages_with(&ben), "Zurückgehalten"),
        Some(DeliveryState::Sent),
        "accepted, never failed, and not delivered without the receipt"
    );
    assert_eq!(ben.messages_with(&anna).len(), 60);

    withhold_first.store(false, Ordering::SeqCst);
    ben.session.wake.notify_one();
    let at_ben = ben
        .wait_for(&anna, "the withheld message", |m| m.len() == 61)
        .await;
    assert_eq!(
        texts(&at_ben).last().map(String::as_str),
        Some("Zurückgehalten")
    );
    anna.wait_for(&ben, "delivered", |m| {
        state_of(m, "Zurückgehalten") == Some(DeliveryState::Delivered)
    })
    .await;
    assert_eq!(resends(&anna), None, "the receipt ended the row");
    assert_eq!(ben.messages_with(&anna).len(), 61, "once, from four wraps");
}
