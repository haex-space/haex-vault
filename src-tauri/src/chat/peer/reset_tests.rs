//! Lost and older key state between two vaults (spec 095 T026, quickstart scenario 7, SC-011): a
//! device whose sessions were deleted, and a vault whose ratchet rows went back to an older copy on
//! the same installation. In both, the device that cannot decrypt sends a reset, both sides end
//! with one new session, and every message without a receipt arrives exactly once without a user
//! action. Who gets a reset and how often is tested in `ratchet/reset_tests.rs`; a wrap delivered
//! twice or past its expiration never reaches a session (`delivery_tests.rs`), so it sends none.

use haex_crdt::rusqlite::types::Value;
use haex_crdt::rusqlite::{params, ToSql};

use super::messages::{DeliveryState, PeerMessage};
use super::ratchet::store;
use super::test_support::{mock_relay, two_vaults, ChatDevice};
use crate::storage::query::Query;

/// The ratchet rows of a device's installation, as a copy of the vault file would hold them.
const RATCHET_TABLES: [&str; 3] = [
    "chat_ratchet_account_no_sync",
    "chat_ratchet_sessions_no_sync",
    "chat_ratchet_peers_no_sync",
];

fn texts(messages: &[PeerMessage]) -> Vec<&str> {
    messages.iter().filter_map(|m| m.text.as_deref()).collect()
}

fn delivered(messages: &[PeerMessage], text: &str) -> bool {
    messages
        .iter()
        .any(|m| m.text.as_deref() == Some(text) && m.delivery == Some(DeliveryState::Delivered))
}

fn sessions(device: &ChatDevice, peer: &ChatDevice) -> usize {
    let (installation, peer) = (device.me.installation, peer.me.device_hex());
    device
        .db
        .read_blocking(move |r| Ok(store::sessions(r, installation, &peer)?.len()))
        .expect("sessions")
}

fn peer_row(device: &ChatDevice, peer: &ChatDevice) -> store::PeerRow {
    let (installation, peer) = (device.me.installation, peer.me.device_hex());
    device
        .db
        .read_blocking(move |r| Ok(store::peer(r, installation, &peer)?))
        .expect("peer")
}

/// `from` sends `text` and waits until `to` has it and `from` sees it delivered.
async fn exchange(from: &ChatDevice, to: &ChatDevice, text: &str) {
    from.send(to, text);
    to.wait_for(from, text, |m| texts(m).contains(&text)).await;
    from.wait_for(to, "delivered", |m| delivered(m, text)).await;
}

/// Both ended up on one new session made after the reset `ben` sent to `anna`, and neither is
/// recovering any more.
fn recovered_on_one_session(anna: &ChatDevice, ben: &ChatDevice) {
    assert_eq!(sessions(anna, ben), 1);
    assert_eq!(sessions(ben, anna), 1);
    let (at_anna, at_ben) = (peer_row(anna, ben), peer_row(ben, anna));
    assert!(at_ben.reset_sent_at.is_some(), "Ben sent a reset");
    assert!(at_anna.reset_received_at.is_some(), "Anna accepted it");
    assert!(!at_anna.refetch_record, "Anna fetched Ben's record again");
    assert_eq!(at_anna.recovering_since, None);
    assert_eq!(at_ben.recovering_since, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_device_that_lost_its_sessions_asks_for_a_reset_and_gets_every_message_once() {
    let (_relay, url) = mock_relay().await;
    let (anna, ben) = two_vaults(std::slice::from_ref(&url)).await;
    ben.start().await;
    anna.start().await;
    exchange(&anna, &ben, "Vorher").await;

    let (installation, device) = (ben.me.installation, anna.me.device_hex());
    ben.db
        .write_blocking(move |tx| Ok(store::delete_sessions(tx, installation, &device)?))
        .expect("lost");
    assert_eq!(sessions(&ben, &anna), 0);

    exchange(&anna, &ben, "Nachher").await;
    exchange(&ben, &anna, "Angekommen").await;
    assert_eq!(
        texts(&ben.messages_with(&anna)),
        ["Vorher", "Nachher", "Angekommen"]
    );
    assert_eq!(
        texts(&anna.messages_with(&ben)),
        ["Vorher", "Nachher", "Angekommen"]
    );
    recovered_on_one_session(&anna, &ben);
}

/// The rows of [`RATCHET_TABLES`] of `device`'s installation.
fn ratchet_rows(device: &ChatDevice) -> Vec<(&'static str, Vec<Vec<Value>>)> {
    let installation = device.me.installation.to_string();
    device
        .db
        .read_blocking(move |r| {
            let mut copy = Vec::new();
            for table in RATCHET_TABLES {
                let rows = r.query_map(
                    &format!("SELECT * FROM {table} WHERE installation_uuid = ?1"),
                    params![installation],
                    |row| {
                        (0..row.as_ref().column_count())
                            .map(|index| row.get::<_, Value>(index))
                            .collect()
                    },
                )?;
                copy.push((table, rows));
            }
            Ok(copy)
        })
        .expect("copy")
}

/// Puts `copy` back as `device`'s ratchet rows.
fn put_back(device: &ChatDevice, copy: Vec<(&'static str, Vec<Vec<Value>>)>) {
    let installation = device.me.installation.to_string();
    device
        .db
        .write_blocking(move |tx| {
            for (table, rows) in &copy {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE installation_uuid = ?1"),
                    params![installation],
                )?;
                for row in rows {
                    let marks = vec!["?"; row.len()].join(", ");
                    let values: Vec<&dyn ToSql> = row.iter().map(|v| v as &dyn ToSql).collect();
                    tx.execute(
                        &format!("INSERT INTO {table} VALUES ({marks})"),
                        values.as_slice(),
                    )?;
                }
            }
            Ok(())
        })
        .expect("put back");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vault_put_back_to_an_older_copy_sends_and_receives_after_a_reset() {
    let (_relay, url) = mock_relay().await;
    let (anna, ben) = two_vaults(std::slice::from_ref(&url)).await;
    ben.start().await;
    anna.start().await;
    exchange(&anna, &ben, "Eins").await;
    let older = ratchet_rows(&ben);
    // Both move their ratchets on after the copy.
    exchange(&ben, &anna, "Zwei").await;
    exchange(&anna, &ben, "Drei").await;
    exchange(&ben, &anna, "Vier").await;

    ben.stop();
    put_back(&ben, older);
    ben.start().await;
    exchange(&anna, &ben, "Nach der Rücksicherung").await;
    exchange(&ben, &anna, "Auch hier").await;
    let all = [
        "Eins",
        "Zwei",
        "Drei",
        "Vier",
        "Nach der Rücksicherung",
        "Auch hier",
    ];
    assert_eq!(texts(&ben.messages_with(&anna)), all);
    assert_eq!(texts(&anna.messages_with(&ben)), all);
    recovered_on_one_session(&anna, &ben);
}
