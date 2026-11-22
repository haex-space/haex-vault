//! Resets at module level (spec 095 T029): who gets one, how often, and what the outbox does after
//! one was accepted.

use super::*;
use crate::chat::peer::outbox::{self, Rumor};
use crate::chat::peer::ratchet::store::PeerRow;
use crate::chat::peer::ratchet::{account, session, RATCHET_KIND, RESET_EVERY_MS};
use crate::chat::peer::test_support::{two_vaults, ChatDevice};
use crate::chat::peer::wrap;
use crate::storage::query::Query;

/// `from` opens a session with `to` from `to`'s keys at `now`.
fn open(from: &ChatDevice, to: &ChatDevice, now: i64) {
    let installation = to.me.installation;
    let keys = to
        .db
        .write_blocking(move |tx| {
            Ok(account::own_keys(&account::load_or_create(
                tx,
                installation,
                0,
            )?)?)
        })
        .expect("account");
    let (me, peer) = (from.me.clone(), to.me.device_hex());
    from.db
        .write_blocking(move |tx| {
            Ok(session::open_outbound(
                tx,
                &me,
                &peer,
                keys.identity,
                keys.start,
                now,
            )?)
        })
        .expect("open");
}

fn sessions(device: &ChatDevice, peer: &ChatDevice) -> usize {
    let (installation, peer) = (device.me.installation, peer.me.device_hex());
    device
        .db
        .read_blocking(move |r| Ok(store::sessions(r, installation, &peer)?))
        .expect("sessions")
        .len()
}

fn peer_row(device: &ChatDevice, peer: &ChatDevice) -> PeerRow {
    let (installation, peer) = (device.me.installation, peer.me.device_hex());
    device
        .db
        .read_blocking(move |r| Ok(store::peer(r, installation, &peer)?))
        .expect("peer")
}

fn undecryptable_at(device: &ChatDevice, from: &ChatDevice, now: i64) -> bool {
    let (me, from) = (device.me.clone(), from.me.keys.device_pubkey);
    device
        .db
        .write_blocking(move |tx| Ok(undecryptable(tx, &me, &from, now)?))
        .expect("undecryptable")
}

fn received_at(device: &ChatDevice, from: &ChatDevice, now: i64) -> bool {
    let (me, from) = (device.me.clone(), from.me.keys.device_pubkey);
    device
        .db
        .write_blocking(move |tx| Ok(received(tx, &me, &from, now)?))
        .expect("received")
}

#[tokio::test]
async fn an_undecryptable_event_from_a_known_device_drops_its_sessions_once_an_hour() {
    let (anna, ben) = two_vaults(&[]).await;
    open(&ben, &anna, 1_000);
    assert_eq!(sessions(&ben, &anna), 1);

    assert!(undecryptable_at(&ben, &anna, 10_000));
    assert_eq!(sessions(&ben, &anna), 0);
    let row = peer_row(&ben, &anna);
    assert_eq!(row.reset_sent_at, Some(10_000));
    assert_eq!(row.recovering_since, Some(10_000));
    assert!(!row.refetch_record);

    open(&ben, &anna, 20_000);
    assert!(
        !undecryptable_at(&ben, &anna, 10_000 + RESET_EVERY_MS - 1),
        "a second reset within the hour"
    );
    assert_eq!(sessions(&ben, &anna), 1, "no second drop within the hour");
    assert!(undecryptable_at(&ben, &anna, 10_000 + RESET_EVERY_MS));
}

#[tokio::test]
async fn a_device_on_no_stored_card_gets_no_reset_and_its_reset_is_not_accepted() {
    let anna = ChatDevice::genesis(&[]);
    let stranger = ChatDevice::genesis(&[]);
    open(&anna, &stranger, 1_000);

    assert!(!undecryptable_at(&anna, &stranger, 2_000));
    assert!(!received_at(&anna, &stranger, 2_000));
    assert_eq!(sessions(&anna, &stranger), 1);
    assert_eq!(peer_row(&anna, &stranger), PeerRow::default());
}

#[tokio::test]
async fn the_reset_not_sent_lets_the_next_undecryptable_event_send_one() {
    let (anna, ben) = two_vaults(&[]).await;
    assert!(undecryptable_at(&ben, &anna, 1_000));
    let (me, device) = (ben.me.clone(), anna.me.keys.device_pubkey);
    ben.db
        .write_blocking(move |tx| Ok(not_sent(tx, &me, &device)?))
        .expect("not sent");
    assert_eq!(peer_row(&ben, &anna).reset_sent_at, None);
    assert!(undecryptable_at(&ben, &anna, 2_000));
}

/// One outbox row with a wrap for each of `devices`, `accepted` ones taken by a Nostr-Relay.
fn row_with_wraps(
    device: &ChatDevice,
    id: &str,
    pending: &[String],
    awaiting: &[String],
    devices: &[(&ChatDevice, bool)],
) {
    let entry_id = id.to_owned();
    let (pending, awaiting) = (pending.to_vec(), awaiting.to_vec());
    let wraps: Vec<(String, nostr::event::Event, bool)> = devices
        .iter()
        .map(|(to, accepted)| {
            let event = wrap::wrap(
                RATCHET_KIND,
                "{}",
                &device.me.keys.device_secret,
                &to.me.keys.device_pubkey,
                1_000,
            )
            .expect("wrap");
            (to.me.device_hex(), event, *accepted)
        })
        .collect();
    device
        .db
        .write_blocking(move |tx| {
            let rumor = Rumor {
                kind: 24120,
                content: "{}".to_owned(),
            };
            outbox::enqueue(
                tx,
                &entry_id,
                "t",
                Some("m"),
                &rumor,
                &pending,
                &awaiting,
                0,
            )?;
            for (to, event, accepted) in &wraps {
                outbox::store_sealed(tx, &entry_id, to, event)?;
                if *accepted {
                    outbox::mark_sealed_accepted(tx, &entry_id, to, 1_000)?;
                }
            }
            tx.execute(
                "UPDATE chat_outbox_no_sync SET next_at = 999999 WHERE id = ?1",
                haex_crdt::rusqlite::params![entry_id],
            )?;
            Ok(())
        })
        .expect("row");
}

/// (row id, pending, next_at, devices with a wrap) of every outbox row.
fn rows(device: &ChatDevice) -> Vec<(String, String, i64, Vec<String>)> {
    device
        .db
        .read_blocking(|r| {
            let rows = r.query_map(
                "SELECT id, pending, next_at FROM chat_outbox_no_sync ORDER BY id",
                &[],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )?;
            let mut out = Vec::new();
            for (id, pending, next_at) in rows {
                let devices = r.query_map(
                    "SELECT recipient_device FROM chat_outbox_sealed_no_sync \
                     WHERE outbox_id = ?1 ORDER BY recipient_device",
                    haex_crdt::rusqlite::params![id],
                    |row| row.get::<_, String>(0),
                )?;
                out.push((id, pending, next_at, devices));
            }
            Ok(out)
        })
        .expect("rows")
}

#[tokio::test]
async fn an_accepted_reset_drops_the_sessions_and_seals_again_what_awaits_the_vault() {
    let (anna, ben) = two_vaults(&[]).await;
    let phone = anna.link();
    open(&anna, &ben, 1_000);
    let (ben_vault, own) = (ben.me.vault_hex.clone(), anna.me.vault_hex.clone());
    // A message to Ben whose wrap was accepted, waiting for Ben's receipt.
    row_with_wraps(
        &anna,
        "a",
        &[],
        std::slice::from_ref(&ben_vault),
        &[(&ben, true)],
    );
    // A message still pending for the own phone; Ben's wrap is accepted, the phone's not yet.
    row_with_wraps(
        &anna,
        "b",
        std::slice::from_ref(&own),
        std::slice::from_ref(&ben_vault),
        &[(&ben, true), (&phone, false)],
    );
    // A receipt to Ben no Nostr-Relay took yet.
    row_with_wraps(
        &anna,
        "c",
        std::slice::from_ref(&ben_vault),
        &[],
        &[(&ben, false)],
    );

    assert!(received_at(&anna, &ben, 5_000));
    assert_eq!(sessions(&anna, &ben), 0);
    let row = peer_row(&anna, &ben);
    assert_eq!(row.reset_received_at, Some(5_000));
    assert_eq!(row.recovering_since, Some(5_000));
    assert!(row.refetch_record);

    let ben_vault_json = format!("[\"{ben_vault}\"]");
    assert_eq!(
        rows(&anna),
        vec![
            ("a".to_owned(), ben_vault_json.clone(), 5_000, vec![]),
            (
                "b".to_owned(),
                format!("[\"{own}\",\"{ben_vault}\"]"),
                5_000,
                vec![phone.me.device_hex()]
            ),
            ("c".to_owned(), ben_vault_json, 5_000, vec![]),
        ]
    );

    assert!(
        !received_at(&anna, &ben, 5_000 + RESET_EVERY_MS - 1),
        "a second reset within the hour is not accepted"
    );
}

#[tokio::test]
async fn recovery_ends_with_an_event_on_a_session_made_after_the_reset() {
    let (anna, ben) = two_vaults(&[]).await;
    assert!(undecryptable_at(&ben, &anna, 5_000));
    let (installation, device) = (ben.me.installation, anna.me.device_hex());
    let recover = |created_at: i64| {
        let device = device.clone();
        ben.db
            .write_blocking(move |tx| Ok(recovered(tx, installation, &device, created_at)?))
            .expect("recovered");
    };
    recover(4_999);
    assert_eq!(peer_row(&ben, &anna).recovering_since, Some(5_000));
    recover(5_000);
    assert_eq!(peer_row(&ben, &anna).recovering_since, None);
    assert_eq!(peer_row(&ben, &anna).reset_sent_at, Some(5_000));
}
