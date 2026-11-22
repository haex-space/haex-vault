//! Keep-alives (spec 095 T035, quickstart scenario 2, SC-003, SC-010): with both vaults open and no
//! user message, only time passes. A copy of a device's key state that an attacker advances with
//! every wrap to the device keeps decrypting until the device sent a ratchet key made after the
//! copy and the peer answered it; from then on (`renewed_at` later than the copy) it decrypts
//! nothing. That happens within 48 hours; requests are answered once, and replies do not create a
//! keep-alive loop.

use std::collections::HashSet;
use std::time::Duration;

use nostr::event::Kind;
use nostr::filter::Filter;
use nostr_sdk::client::Client;
use vodozemac::olm::{Session, SessionPickle};

use crate::chat::peer::messages::{DeliveryState, PeerMessage};
use crate::chat::peer::ratchet::store::{self, SessionRow};
use crate::chat::peer::ratchet::wire::RatchetRumor;
use crate::chat::peer::test_support::{mock_relay, two_vaults, ChatDevice, LIMIT};
use crate::chat::peer::wrap;
use crate::sync::presence;

const HOUR_MS: i64 = 60 * 60 * 1000;
/// How often the run loop looks for a keep-alive that fell due (`KEEP_ALIVE_CHECK`).
const STEP_MS: i64 = HOUR_MS / 4;

fn delivered(messages: &[PeerMessage], text: &str) -> bool {
    messages
        .iter()
        .any(|m| m.text.as_deref() == Some(text) && m.delivery == Some(DeliveryState::Delivered))
}

/// The sessions of `device` with `peer`; the first is the one it sends on.
fn sessions(device: &ChatDevice, peer: &ChatDevice) -> Vec<SessionRow> {
    let (installation, peer) = (device.me.installation, peer.me.device_hex());
    device
        .db
        .read_blocking(move |r| Ok(store::sessions(r, installation, &peer)?))
        .expect("sessions")
}

fn last_received(device: &ChatDevice, peer: &ChatDevice) -> Option<i64> {
    sessions(device, peer)
        .iter()
        .filter_map(|row| row.last_received_at)
        .max()
}

/// Waits until `device` received something from `peer` after `before`.
async fn wait_received(device: &ChatDevice, peer: &ChatDevice, before: Option<i64>) {
    let deadline = tokio::time::Instant::now() + LIMIT;
    while last_received(device, peer) == before {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for a keep-alive"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// An attacker's copy of a device's sessions, advanced with every wrap to the device it sees.
struct Copy {
    sessions: Vec<Session>,
}

impl Copy {
    fn of(rows: &[SessionRow]) -> Self {
        Self {
            sessions: rows
                .iter()
                .map(|row| {
                    let pickle: SessionPickle =
                        serde_json::from_str(row.session.as_str()).expect("pickle");
                    Session::from_pickle(pickle)
                })
                .collect(),
        }
    }

    /// Whether one of the copied sessions decrypts `rumor`'s message (and moves on with it).
    fn decrypts(&mut self, rumor: &RatchetRumor) -> bool {
        let message = rumor.message().expect("an Olm message");
        self.sessions
            .iter_mut()
            .any(|session| session.decrypt(&message).is_ok())
    }
}

/// The ratchet rumors to `device` the relay holds and `seen` does not name yet.
async fn new_rumors(
    client: &Client,
    device: &ChatDevice,
    seen: &mut HashSet<nostr::event::EventId>,
) -> Vec<RatchetRumor> {
    let to_device = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(presence::public_key(&device.me.keys.device_pubkey).expect("key"));
    let events = client
        .fetch_events(to_device)
        .timeout(Duration::from_secs(5))
        .await
        .expect("fetch");
    let mut rumors = Vec::new();
    for event in events {
        if !seen.insert(event.id) {
            continue;
        }
        let opened = wrap::unwrap(&event, &device.me.keys.device_secret).expect("the wrap opens");
        rumors.push(RatchetRumor::decode(&opened.content).expect("a ratchet rumor"));
    }
    rumors
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn keep_alives_end_a_copied_key_state_within_48_hours_and_come_once_a_day() {
    let (_relay, url) = mock_relay().await;
    let (anna, ben) = two_vaults(std::slice::from_ref(&url)).await;
    anna.start().await;
    ben.start().await;
    anna.send(&ben, "hallo");
    ben.wait_for(&anna, "hallo", |m| m.iter().any(|m| !m.own))
        .await;
    anna.wait_for(&ben, "delivered", |m| delivered(m, "hallo"))
        .await;
    ben.send(&anna, "zurück");
    anna.wait_for(&ben, "zurück", |m| m.iter().any(|m| !m.own))
        .await;
    ben.wait_for(&anna, "delivered", |m| delivered(m, "zurück"))
        .await;

    // A keep-alive gets an immediate bounded response even though Ben sent recently and is not
    // due for his own proactive request. This prevents a fresh ratchet key waiting at the daily
    // gate; the distinct reply kind does not ask Anna to answer again.
    let (anna_check, ben_check) = (anna.delivery().await, ben.delivery().await);
    let (at_ben, at_anna) = (last_received(&ben, &anna), last_received(&anna, &ben));
    let ben_sent_before = sessions(&ben, &anna)[0]
        .last_sent_at
        .expect("Ben just sent");
    ben.clock.advance(1);
    anna.clock
        .advance(crate::chat::peer::ratchet::KEEP_ALIVE_MS);
    assert_eq!(
        anna_check.send_keep_alives().await.expect("Anna's request"),
        1
    );
    wait_received(&ben, &anna, at_ben).await;
    wait_received(&anna, &ben, at_anna).await;
    assert!(
        sessions(&ben, &anna)[0]
            .last_sent_at
            .expect("Ben's response")
            > ben_sent_before,
        "Ben answers before his daily keep-alive is due"
    );
    ben.clock
        .advance(crate::chat::peer::ratchet::KEEP_ALIVE_MS - 1);

    let archive = Client::builder().build();
    archive.add_relay(url.as_str()).await.expect("relay");
    archive.connect().and_wait(Duration::from_secs(5)).await;
    let mut seen = HashSet::new();
    new_rumors(&archive, &ben, &mut seen).await;

    // The copy of Ben's key state, taken now.
    let copied_at = ben.clock.now();
    let mut copy = Copy::of(&sessions(&ben, &anna));
    assert!(!copy.sessions.is_empty());

    // Time passes with both vaults open; the run loop's check is driven step by step.
    let (mut from_anna, mut from_ben) = (0, 0);
    let mut renewed_after = None;
    let (mut copy_decrypted, mut after_renewal) = (0, 0);
    let mut passed = 0;
    while passed < 72 * HOUR_MS {
        anna.clock.advance(STEP_MS);
        ben.clock.advance(STEP_MS);
        passed += STEP_MS;
        let (at_ben, at_anna) = (last_received(&ben, &anna), last_received(&anna, &ben));
        let anna_sent = anna_check.send_keep_alives().await.expect("Anna's check");
        let ben_sent = ben_check.send_keep_alives().await.expect("Ben's check");
        from_anna += anna_sent;
        from_ben += ben_sent;
        if ben_sent > 0 {
            wait_received(&anna, &ben, at_anna).await;
        }
        if anna_sent > 0 {
            wait_received(&ben, &anna, at_ben).await;
            for rumor in new_rumors(&archive, &ben, &mut seen).await {
                let decrypted = copy.decrypts(&rumor);
                if renewed_after.is_some() {
                    after_renewal += 1;
                    assert!(!decrypted, "the copy decrypts nothing after the renewal");
                } else if decrypted {
                    copy_decrypted += 1;
                }
            }
        }
        let renewed = sessions(&ben, &anna)
            .first()
            .and_then(|row| row.renewed_at)
            .is_some_and(|renewed_at| renewed_at > copied_at);
        if renewed && renewed_after.is_none() {
            renewed_after = Some(passed);
        }
    }
    archive.shutdown().await;

    let renewed_after = renewed_after.expect("Ben's session renewed");
    assert!(
        renewed_after <= 48 * HOUR_MS,
        "renewed after {} hours",
        renewed_after / HOUR_MS
    );
    assert!(
        copy_decrypted > 0,
        "before the renewal the copy still decrypts, so the test would see one claimed too early"
    );
    assert!(after_renewal > 0, "a keep-alive after the renewal");
    for (who, sent) in [("Anna", from_anna), ("Ben", from_ben)] {
        assert!(
            (2..=3).contains(&sent),
            "{who} sent {sent} keep-alives in 72 hours"
        );
    }

    // A closed vault sends none: Ben opens again after two days and sends one at the start, Anna
    // stays closed and sends nothing.
    drop((anna_check, ben_check));
    anna.stop();
    ben.stop();
    anna.clock.advance(48 * HOUR_MS);
    ben.clock.advance(48 * HOUR_MS);
    let anna_sent_before = sessions(&anna, &ben)[0].last_sent_at;
    let ben_sent_before = sessions(&ben, &anna)[0].last_sent_at;
    ben.start().await;
    let deadline = tokio::time::Instant::now() + LIMIT;
    while sessions(&ben, &anna)[0].last_sent_at == ben_sent_before {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for Ben's keep-alive at the start"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        sessions(&anna, &ben)[0].last_sent_at,
        anna_sent_before,
        "a closed vault sends no keep-alive"
    );
}
