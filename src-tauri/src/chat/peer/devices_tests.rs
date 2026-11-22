//! Devices that come and go (spec 095 T036, T037, quickstart scenarios 3, 4 and 5, SC-003, SC-007,
//! SC-008): a device removed from its vault gets nothing more and its sessions go, a newly linked
//! device gets nothing from before its vault's contact knew it, and a copy of a vault file starts
//! with keys of its own and leaves the first installation's rows alone.

use std::collections::HashSet;
use std::time::Duration;

use haex_crdt::rusqlite::params;
use haex_crdt::rusqlite::types::Value;
use nostr::event::{Event, EventId, Kind};
use nostr::filter::Filter;
use nostr_sdk::client::Client;
use vodozemac::olm::{Session, SessionPickle};

use crate::chat::peer::messages::PeerMessage;
use crate::chat::peer::ratchet::store;
use crate::chat::peer::ratchet::wire::RatchetRumor;
use crate::chat::peer::test_support::{mock_relay, ChatDevice};
use crate::chat::peer::wrap;
use crate::storage::query::Query;
use crate::sync::presence;

fn has(messages: &[PeerMessage], text: &str) -> bool {
    messages.iter().any(|m| m.text.as_deref() == Some(text))
}

/// The wraps to `device` the relay at `url` holds.
async fn wraps_to(url: &str, device: &ChatDevice) -> Vec<Event> {
    let client = Client::builder().build();
    client.add_relay(url).await.expect("relay");
    client.connect().and_wait(Duration::from_secs(5)).await;
    let filter = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(presence::public_key(&device.me.keys.device_pubkey).expect("key"));
    let events = client
        .fetch_events(filter)
        .timeout(Duration::from_secs(5))
        .await
        .expect("fetch");
    client.shutdown().await;
    events.into_iter().collect()
}

/// Every wrap the relay at `url` holds.
async fn archive(url: &str) -> Vec<Event> {
    let client = Client::builder().build();
    client.add_relay(url).await.expect("relay");
    client.connect().and_wait(Duration::from_secs(5)).await;
    let events = client
        .fetch_events(Filter::new().kind(Kind::GiftWrap))
        .timeout(Duration::from_secs(5))
        .await
        .expect("fetch");
    client.shutdown().await;
    events.into_iter().collect()
}

/// The sessions of `device` with `peer`, as pickles.
fn session_pickles(device: &ChatDevice, peer: &ChatDevice) -> Vec<String> {
    let (installation, peer) = (device.me.installation, peer.me.device_hex());
    device
        .db
        .read_blocking(move |r| {
            Ok(store::sessions(r, installation, &peer)?
                .into_iter()
                .map(|row| row.session.to_string())
                .collect())
        })
        .expect("sessions")
}

/// What `device` keeps about `peer`: sessions, peer state, stored record.
fn kept_about(device: &ChatDevice, peer: &ChatDevice) -> (usize, store::PeerRow, bool) {
    let (installation, peer) = (device.me.installation, peer.me.device_hex());
    device
        .db
        .read_blocking(move |r| {
            Ok((
                store::sessions(r, installation, &peer)?.len(),
                store::peer(r, installation, &peer)?,
                store::record(r, &peer)?.is_some(),
            ))
        })
        .expect("read")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_removed_device_gets_nothing_more_and_its_sessions_go() {
    let (_relay, url) = mock_relay().await;
    let relays = std::slice::from_ref(&url);
    let anna = ChatDevice::genesis(relays);
    let ben = ChatDevice::genesis(relays);
    anna.learn(&ben).await;
    let phone = anna.link();
    ben.learn(&anna).await;
    for device in [&anna, &phone, &ben] {
        device.start().await;
    }
    ben.send(&anna, "eins");
    anna.wait_for(&ben, "eins", |m| has(m, "eins")).await;
    phone.wait_for(&ben, "eins", |m| has(m, "eins")).await;
    assert!(
        kept_about(&ben, &phone).0 > 0,
        "Ben has a session with the phone"
    );

    // The phone's state is copied, then the phone is removed and Ben gets the newer card.
    let copied = session_pickles(&phone, &ben);
    assert!(!copied.is_empty());
    phone.stop();
    anna.remove(&phone);
    ben.learn(&anna).await;
    let to_phone_before: HashSet<EventId> = wraps_to(&url, &phone)
        .await
        .into_iter()
        .map(|event| event.id)
        .collect();
    let all_before: HashSet<EventId> = archive(&url)
        .await
        .into_iter()
        .map(|event| event.id)
        .collect();
    ben.delivery()
        .await
        .catch_up_and_keep_record()
        .await
        .expect("catch-up");
    let (sessions, peer, record) = kept_about(&ben, &phone);
    assert_eq!(sessions, 0, "the sessions with the removed device are gone");
    assert_eq!(peer, store::PeerRow::default());
    assert!(!record, "its record is gone");
    assert!(kept_about(&ben, &anna).0 > 0, "Anna's device stays");

    ben.send(&anna, "zwei");
    ben.send(&anna, "drei");
    anna.wait_for(&ben, "zwei and drei", |m| has(m, "zwei") && has(m, "drei"))
        .await;
    let to_phone: Vec<Event> = wraps_to(&url, &phone)
        .await
        .into_iter()
        .filter(|event| !to_phone_before.contains(&event.id))
        .collect();
    assert!(to_phone.is_empty(), "no wrap to the removed device");

    // Even with Anna's device secret, the copied state decrypts none of the later messages.
    let later: Vec<Event> = archive(&url)
        .await
        .into_iter()
        .filter(|event| !all_before.contains(&event.id))
        .collect();
    let mut opened = 0;
    for event in later {
        let Ok(rumor) = wrap::unwrap(&event, &anna.me.keys.device_secret) else {
            continue;
        };
        let Ok(message) = RatchetRumor::decode(&rumor.content).and_then(|r| r.message()) else {
            continue;
        };
        opened += 1;
        for pickle in &copied {
            let pickle: SessionPickle = serde_json::from_str(pickle).expect("pickle");
            assert!(
                Session::from_pickle(pickle).decrypt(&message).is_err(),
                "the removed device's copy decrypts a later message"
            );
        }
    }
    assert!(opened >= 2, "the later messages to Anna were tried");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_device_gets_only_what_was_sent_after_the_contact_knew_it() {
    let (_relay, url) = mock_relay().await;
    let relays = std::slice::from_ref(&url);
    let anna = ChatDevice::genesis(relays);
    let ben = ChatDevice::genesis(relays);
    anna.learn(&ben).await;
    ben.learn(&anna).await;
    anna.start().await;
    ben.start().await;
    ben.send(&anna, "vorher");
    anna.wait_for(&ben, "vorher", |m| has(m, "vorher")).await;
    anna.send(&ben, "auch vorher");
    ben.wait_for(&anna, "auch vorher", |m| has(m, "auch vorher"))
        .await;

    // The tablet is linked: the own sync gives it the history, Ben still holds the older card.
    let tablet = anna.link();
    tablet.start().await;
    let before: Vec<Event> = archive(&url).await;
    assert!(!before.is_empty());
    for event in &before {
        assert!(
            wrap::unwrap(event, &tablet.me.keys.device_secret).is_err(),
            "no archived wrap is for the new device"
        );
    }

    ben.learn(&anna).await;
    ben.send(&anna, "nachher");
    tablet
        .wait_for(&ben, "nachher", |m| has(m, "nachher"))
        .await;
    anna.wait_for(&ben, "nachher", |m| has(m, "nachher")).await;
    assert!(
        kept_about(&tablet, &ben).0 > 0,
        "the later message reached the new device directly, on a session with Ben"
    );
}

/// The rows of `installation` in the ratchet tables of `device`'s database.
fn installation_rows(device: &ChatDevice, installation: uuid::Uuid) -> Vec<Vec<Vec<Value>>> {
    device
        .db
        .read_blocking(move |r| {
            let mut tables = Vec::new();
            for table in [
                "chat_ratchet_account_no_sync",
                "chat_ratchet_sessions_no_sync",
                "chat_ratchet_peers_no_sync",
            ] {
                let id = installation.to_string();
                tables.push(r.query_map(
                    &format!("SELECT * FROM {table} WHERE installation_uuid = ?1 ORDER BY 1, 2, 3"),
                    params![id],
                    |row| {
                        (0..row.as_ref().column_count())
                            .map(|index| row.get::<_, Value>(index))
                            .collect()
                    },
                )?);
            }
            Ok(tables)
        })
        .expect("rows")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copied_vault_file_makes_its_own_keys_and_leaves_the_first_installation_alone() {
    let (_relay, url) = mock_relay().await;
    let relays = std::slice::from_ref(&url);
    let anna = ChatDevice::genesis(relays);
    let ben = ChatDevice::genesis(relays);
    anna.learn(&ben).await;
    ben.learn(&anna).await;
    anna.start().await;
    ben.start().await;
    ben.send(&anna, "eins");
    anna.wait_for(&ben, "eins", |m| has(m, "eins")).await;
    anna.send(&ben, "zwei");
    ben.wait_for(&anna, "zwei", |m| has(m, "zwei")).await;

    let copy = anna.copy_vault_file();
    assert_ne!(copy.me.installation, anna.me.installation);
    assert_ne!(copy.me.keys.device_pubkey, anna.me.keys.device_pubkey);
    let first = anna.me.installation;
    let first_rows = installation_rows(&copy, first);
    assert!(
        !first_rows[1].is_empty(),
        "the copied file holds the first installation's sessions"
    );

    copy.start().await;
    let (own, published) = {
        let installation = copy.me.installation;
        copy.db
            .read_blocking(move |r| {
                Ok(store::account(r, installation)?
                    .map(|row| (true, row.record_published_at.is_some()))
                    .unwrap_or((false, false)))
            })
            .expect("account")
    };
    assert!(
        own && published,
        "the copy made and published its own record"
    );

    ben.learn(&anna).await;
    ben.send(&anna, "an beide");
    anna.wait_for(&ben, "an beide", |m| has(m, "an beide"))
        .await;
    copy.wait_for(&ben, "an beide", |m| has(m, "an beide"))
        .await;
    copy.send(&ben, "von der Kopie");
    ben.wait_for(&anna, "von der Kopie", |m| has(m, "von der Kopie"))
        .await;
    anna.send(&ben, "vom Original");
    ben.wait_for(&anna, "vom Original", |m| has(m, "vom Original"))
        .await;

    copy.stop();
    assert_eq!(
        installation_rows(&copy, first),
        first_rows,
        "the copy never changed the first installation's rows"
    );
}
