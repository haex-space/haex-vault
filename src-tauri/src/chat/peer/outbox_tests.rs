//! The outbox's back-off, failures without an end, sending again and "Erneut senden" (spec 072
//! T026, research R6; spec 095 T028, T031, T033).

//! spaex: These integration tests currently share one vault fixture and exercise transitions of
//! the same persisted outbox state. Split the resend schedule and resend lifecycle cases into
//! `outbox_resend_tests.rs` when the next resend behavior is added; keep failure/back-off cases here.

use super::*;
use crate::chat::peer::ids;
use crate::chat::peer::ratchet::RESEND_AFTER_MS;
use crate::chat::peer::receive::{ensure_delivery, insert_message, MessageRow};
use crate::chat::peer::test_support::{two_vaults, ChatDevice};
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

const PEER: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";
const OTHER: &str = "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3";
const MESSAGE: &str = "0b5c3f0e-4f6c-5c55-9a55-4b0d6f8a1e01";

fn queued(db: &VaultDb, now: i64) -> Entry {
    db.write_blocking(move |tx| {
        ensure_delivery(tx, MESSAGE.parse().expect("uuid"), PEER)?;
        enqueue(
            tx,
            "e1",
            "t1",
            Some(MESSAGE),
            &Rumor {
                kind: 24120,
                content: "{}".into(),
            },
            &[PEER.to_owned()],
            &[PEER.to_owned()],
            now,
        )?;
        Ok(())
    })
    .expect("enqueue");
    due(db, now).into_iter().next().expect("due at once")
}

fn due(db: &VaultDb, now: i64) -> Vec<Entry> {
    db.read_blocking(move |r| Ok(super::due(r, now)?))
        .expect("due")
}

fn attempt(db: &VaultDb, entry: &Entry, attempt: Attempt, now: i64) {
    let entry = entry.clone();
    db.write_blocking(move |tx| Ok(record(tx, &entry, &attempt, now)?))
        .expect("record");
}

fn delivery(db: &VaultDb) -> (Option<i64>, Option<i64>, Option<String>) {
    let id = ids::delivery_id(MESSAGE.parse().expect("uuid"), PEER).to_string();
    db.read_blocking(move |r| {
        Ok(r.query_row(
            "SELECT sent_at, failed_at, failed_reason FROM chat_deliveries WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?
        .expect("row"))
    })
    .expect("read")
}

fn not_yet() -> Attempt {
    Attempt {
        error: Some("offline".into()),
        ..Attempt::default()
    }
}

/// An attempt that made wraps for the peer's devices, none of which a Nostr-Relay took.
fn unaccepted() -> Attempt {
    Attempt {
        unaccepted: vec![PEER.to_owned()],
        ..not_yet()
    }
}

fn next_at(db: &VaultDb) -> i64 {
    db.read_blocking(|r| {
        Ok(r.query_row(
            "SELECT next_at FROM chat_outbox_no_sync WHERE id = 'e1'",
            &[],
            |row| row.get::<_, i64>(0),
        )?
        .expect("row"))
    })
    .expect("read")
}

#[test]
fn the_waits_grow_to_fifteen_minutes() {
    let waits: Vec<i64> = (1..=7).map(backoff).collect();
    assert_eq!(
        waits,
        [5_000, 15_000, 60_000, 300_000, 900_000, 900_000, 900_000]
    );
}

#[test]
fn a_failed_attempt_waits_and_the_row_survives_a_restart() {
    let anna = ChatDevice::genesis(&[]);
    let entry = queued(&anna.db, 1_000);
    attempt(&anna.db, &entry, not_yet(), 1_000);
    assert!(due(&anna.db, 5_999).is_empty(), "waits 5 s");
    let again = due(&anna.db, 6_000);
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].attempts, 1);

    attempt(&anna.db, &again[0], not_yet(), 6_000);
    assert!(due(&anna.db, 20_999).is_empty(), "then 15 s");

    // A new handle on the same vault, as after closing and opening it again.
    let reopened = anna.member.device.vault_db();
    assert_eq!(
        due(&reopened, 21_000).len(),
        1,
        "the outbox is in the vault"
    );
    assert_eq!(
        reopened.read_blocking(|r| Ok(next_due(r)?)).expect("next"),
        Some(21_000)
    );
}

#[test]
fn an_accepted_vault_is_sent_and_the_row_waits_for_its_receipt() {
    let anna = ChatDevice::genesis(&[]);
    let entry = queued(&anna.db, 1_000);
    attempt(
        &anna.db,
        &entry,
        Attempt {
            accepted: vec![PEER.to_owned()],
            ..Attempt::default()
        },
        2_000,
    );
    assert_eq!(delivery(&anna.db).0, Some(2_000));
    let resend = due(&anna.db, 2_000 + resend_after(0)).remove(0);
    assert!(resend.is_resend());
    assert_eq!(resend.awaiting, [PEER]);
}

/// Spec 095 T028: no end, and a clear failure.
#[test]
fn after_24_hours_the_delivery_fails_but_is_still_tried_and_an_accepted_wrap_clears_it() {
    let anna = ChatDevice::genesis(&[]);
    let entry = queued(&anna.db, 1_000);
    attempt(&anna.db, &entry, unaccepted(), 1_000 + GIVE_UP_MS - 1);
    assert_eq!(delivery(&anna.db), (None, None, None), "not before 24 h");

    anna.db
        .write_blocking(|tx| {
            tx.execute(
                "UPDATE chat_outbox_no_sync SET attempts = 40 WHERE id = 'e1'",
                &[],
            )?;
            Ok(())
        })
        .expect("set the persisted retry count");
    let entry = due(&anna.db, i64::MAX - 1).remove(0);
    let later = 1_000 + GIVE_UP_MS;
    attempt(&anna.db, &entry, unaccepted(), later);
    let (sent, failed, reason) = delivery(&anna.db);
    assert_eq!(sent, None);
    assert_eq!(failed, Some(later));
    assert_eq!(reason.as_deref(), Some("noRelay"));
    assert_eq!(
        next_at(&anna.db),
        later + 900_000,
        "still tried, at the last back-off step"
    );

    let entry = due(&anna.db, later + 900_000).remove(0);
    attempt(&anna.db, &entry, unaccepted(), later + 900_000);
    assert_eq!(delivery(&anna.db).1, Some(later), "marked once");

    let entry = due(&anna.db, i64::MAX - 1).remove(0);
    attempt(
        &anna.db,
        &entry,
        Attempt {
            accepted: vec![PEER.to_owned()],
            ..Attempt::default()
        },
        later + 2_000_000,
    );
    assert_eq!(
        delivery(&anna.db),
        (Some(later + 2_000_000), None, None),
        "an accepted wrap clears the failure and sets sent_at"
    );
}

#[test]
fn a_vault_waiting_for_keys_or_sent_already_does_not_fail() {
    let anna = ChatDevice::genesis(&[]);
    let entry = queued(&anna.db, 1_000);
    attempt(&anna.db, &entry, not_yet(), 1_000 + GIVE_UP_MS);
    assert_eq!(delivery(&anna.db), (None, None, None), "waiting for keys");

    let id = ids::delivery_id(MESSAGE.parse().expect("uuid"), PEER).to_string();
    anna.db
        .write_blocking(move |tx| {
            tx.execute(
                "UPDATE chat_deliveries SET sent_at = 5 WHERE id = ?1",
                params![id],
            )?;
            Ok(())
        })
        .expect("sent");
    let entry = due(&anna.db, i64::MAX - 1).remove(0);
    attempt(&anna.db, &entry, unaccepted(), 2_000 + GIVE_UP_MS);
    assert_eq!(delivery(&anna.db), (Some(5), None, None), "sent before");
}

#[test]
fn partial_acceptance_clears_no_relay_but_keeps_the_vault_pending() {
    let anna = ChatDevice::genesis(&[]);
    let entry = queued(&anna.db, 1_000);
    let failed_at = 1_000 + GIVE_UP_MS;
    attempt(&anna.db, &entry, unaccepted(), failed_at);
    assert_eq!(delivery(&anna.db).2.as_deref(), Some("noRelay"));

    let entry = due(&anna.db, i64::MAX - 1).remove(0);
    let next = failed_at + backoff(entry.attempts);
    attempt(
        &anna.db,
        &entry,
        Attempt {
            partial_accepted: vec![PEER.to_owned()],
            ..Attempt::default()
        },
        next,
    );

    assert_eq!(delivery(&anna.db), (None, None, None));
    assert_eq!(due(&anna.db, i64::MAX - 1)[0].pending, [PEER.to_owned()]);
}

#[test]
fn a_parked_vault_stays_parked_when_another_vault_later_succeeds() {
    let anna = ChatDevice::genesis(&[]);
    queued(&anna.db, 1_000);
    let message = MESSAGE.parse().expect("uuid");
    anna.db
        .write_blocking(move |tx| {
            ensure_delivery(tx, message, OTHER)?;
            let pending = vec![PEER, OTHER];
            tx.execute(
                "UPDATE chat_outbox_no_sync SET pending = ?1, awaiting = ?2 WHERE id = 'e1'",
                params![
                    serde_json::to_string(&pending).expect("pending"),
                    serde_json::to_string(&pending).expect("awaiting")
                ],
            )?;
            Ok(())
        })
        .expect("second delivery");
    anna.db
        .write_blocking(|tx| Ok(park_vault(tx, PEER, 2_000)?))
        .expect("peer blocked this vault");
    let entry = due(&anna.db, 7_000).remove(0);
    attempt(
        &anna.db,
        &entry,
        Attempt {
            accepted: vec![OTHER.to_owned()],
            ..Attempt::default()
        },
        7_000,
    );
    let other_delivery = ids::delivery_id(MESSAGE.parse().expect("uuid"), OTHER).to_string();
    anna.db
        .write_blocking(move |tx| {
            tx.execute(
                "UPDATE chat_deliveries SET delivered_at = 7001 WHERE id = ?1",
                params![other_delivery],
            )?;
            Ok(())
        })
        .expect("other vault delivered");

    let automatic_resend = 7_000 + resend_after(0);
    assert_eq!(next_at(&anna.db), automatic_resend);
    let entry = due(&anna.db, automatic_resend).remove(0);
    let message = MESSAGE.parse().expect("uuid");
    let sender_vault = anna.me.vault_hex.clone();
    let sender_device = anna.me.device_hex();
    let inserted_by = sender_device.clone();
    anna.db
        .write_blocking(move |tx| {
            insert_message(
                tx,
                &MessageRow {
                    id: message,
                    thread_id: uuid::Uuid::nil(),
                    event_id: "e1",
                    sender_vault: &sender_vault,
                    sender_device: &sender_device,
                    text: "message",
                    sent_at: 1_000,
                    received_at: 1_000,
                    inserted_by: &inserted_by,
                },
            )?;
            Ok(())
        })
        .expect("message still exists");
    let me = anna.me.clone();
    assert!(anna
        .db
        .write_blocking(move |tx| Ok(start_resend(tx, &me, &entry)?))
        .expect("automatic resend")
        .is_none());
    assert_eq!(next_at(&anna.db), i64::MAX, "blocked vault stays parked");
    let blocked_resend = anna
        .db
        .write_blocking(|tx| Ok(resend(tx, MESSAGE, 8_000)))
        .expect_err("manual resend cannot clear a blocked failure");
    assert!(matches!(
        blocked_resend,
        crate::error::HaexError::InvalidInput { .. }
    ));
}

#[test]
fn an_attempt_snapshot_cannot_unpark_a_row_after_a_blocked_answer() {
    let anna = ChatDevice::genesis(&[]);
    let entry = queued(&anna.db, 1_000);
    anna.db
        .write_blocking(|tx| Ok(park_vault(tx, PEER, 2_000)?))
        .expect("block while the relay attempt is in flight");

    anna.db
        .write_blocking(move |tx| {
            record(
                tx,
                &entry,
                &Attempt {
                    accepted: vec![PEER.to_owned()],
                    ..Attempt::default()
                },
                3_000,
            )?;
            Ok(())
        })
        .expect("record stale attempt");

    assert_eq!(next_at(&anna.db), i64::MAX, "the row stays parked");
    assert!(due(&anna.db, i64::MAX - 1).is_empty());
    assert_eq!(delivery(&anna.db).2.as_deref(), Some("blocked"));
}

#[test]
fn no_nostr_relay_fails_at_once_but_keeps_trying_and_resend_makes_it_due() {
    let anna = ChatDevice::genesis(&[]);
    let entry = queued(&anna.db, 1_000);
    attempt(
        &anna.db,
        &entry,
        Attempt {
            failed: vec![(PEER.to_owned(), FailReason::NoRelay)],
            ..Attempt::default()
        },
        2_000,
    );
    let (_, failed, reason) = delivery(&anna.db);
    assert_eq!(failed, Some(2_000));
    assert_eq!(reason.as_deref(), Some("noRelay"));
    assert_eq!(next_at(&anna.db), 7_000, "not parked");
    assert_eq!(due(&anna.db, 7_000)[0].pending, [PEER.to_owned()]);

    let found = anna
        .db
        .write_blocking(|tx| Ok(resend(tx, MESSAGE, 3_000)?))
        .expect("resend");
    assert!(found);
    assert_eq!(delivery(&anna.db).1, None, "the failure is cleared");
    let again = due(&anna.db, 3_000);
    assert_eq!(again.len(), 1, "due at once");
    assert_eq!(again[0].attempts, 0);
}

#[test]
fn a_permanent_failure_is_recorded_at_once() {
    let anna = ChatDevice::genesis(&[]);
    let entry = queued(&anna.db, 1_000);
    attempt(
        &anna.db,
        &entry,
        Attempt {
            failed: vec![(PEER.to_owned(), FailReason::NoDevices)],
            ..Attempt::default()
        },
        2_000,
    );
    let (_, failed, reason) = delivery(&anna.db);
    assert_eq!(failed, Some(2_000));
    assert_eq!(reason.as_deref(), Some("noDevices"));
    assert!(
        due(&anna.db, i64::MAX - 1).is_empty(),
        "parked, not retried"
    );
    assert!(
        anna.db
            .write_blocking(|tx| Ok(resend(tx, MESSAGE, 3_000)?))
            .expect("resend"),
        "the parked row is found again"
    );
    assert_eq!(due(&anna.db, 3_000)[0].pending, [PEER.to_owned()]);
    assert!(
        !anna
            .db
            .write_blocking(|tx| Ok(resend(tx, "unknown", 3_000)?))
            .expect("resend"),
        "a message sent from another device has no row here"
    );
}

#[test]
fn after_reconnecting_waiting_rows_are_due_at_once_but_parked_ones_stay() {
    let anna = ChatDevice::genesis(&[]);
    let entry = queued(&anna.db, 1_000);
    attempt(&anna.db, &entry, not_yet(), 1_000);
    anna.db
        .write_blocking(|tx| Ok(due_now(tx, 2_000)?))
        .expect("due now");
    let waiting = due(&anna.db, 2_000);
    assert_eq!(
        waiting.len(),
        1,
        "no 5 s wait after the connections came back"
    );

    attempt(
        &anna.db,
        &waiting[0],
        Attempt {
            failed: vec![(PEER.to_owned(), FailReason::NoDevices)],
            ..Attempt::default()
        },
        3_000,
    );
    anna.db
        .write_blocking(|tx| Ok(due_now(tx, 4_000)?))
        .expect("due now");
    assert!(
        due(&anna.db, 4_000).is_empty(),
        "a failed row waits for resend"
    );
}

/// Spec 095 T019, T031: a message's row stays after a Nostr-Relay took it until the receipt of
/// every awaited vault came, due again a day later; then it goes with its stored wraps.
#[test]
fn an_accepted_row_waits_for_the_receipt_and_goes_with_its_wraps() {
    let anna = ChatDevice::genesis(&[]);
    anna.db
        .write_blocking(|tx| {
            ensure_delivery(tx, MESSAGE.parse().expect("uuid"), PEER)?;
            enqueue(
                tx,
                "e1",
                "t1",
                Some(MESSAGE),
                &Rumor {
                    kind: 24120,
                    content: "{}".into(),
                },
                &[PEER.to_owned()],
                &[PEER.to_owned()],
                1_000,
            )?;
            Ok(())
        })
        .expect("enqueue");
    let entry = due(&anna.db, 1_000).remove(0);
    assert_eq!(entry.awaiting, [PEER.to_owned()]);

    let wrap = {
        let keys = crate::sync::keys::DeviceKeys::generate();
        crate::chat::peer::wrap::wrap(
            nostr::event::Kind::Custom(24122),
            "{}",
            &keys.device_secret,
            &anna.me.keys.device_pubkey,
            1_000,
        )
        .expect("wrap")
    };
    let stored = wrap.clone();
    anna.db
        .write_blocking(move |tx| Ok(store_sealed(tx, "e1", PEER, &stored)?))
        .expect("store");
    let sealed_now = |db: &VaultDb| {
        db.read_blocking(|r| Ok(sealed(r, "e1", PEER)?))
            .expect("sealed")
    };
    assert_eq!(
        sealed_now(&anna.db),
        Some(Sealed {
            wrap: wrap.clone(),
            accepted_at: None
        })
    );
    anna.db
        .write_blocking(|tx| Ok(mark_sealed_accepted(tx, "e1", PEER, 1_500)?))
        .expect("accepted");
    assert_eq!(sealed_now(&anna.db).expect("kept").accepted_at, Some(1_500));

    attempt(
        &anna.db,
        &entry,
        Attempt {
            accepted: vec![PEER.to_owned()],
            ..Attempt::default()
        },
        2_000,
    );
    assert_eq!(delivery(&anna.db).0, Some(2_000));
    assert!(due(&anna.db, 2_000 + RESEND_AFTER_MS[0] - 1).is_empty());
    anna.db
        .write_blocking(|tx| Ok(due_now(tx, 3_000)?))
        .expect("due now");
    assert!(
        due(&anna.db, 3_000).is_empty(),
        "opening the vault does not send again before the day is over"
    );
    assert!(
        due(&anna.db, 2_000 + RESEND_AFTER_MS[0])[0].is_resend(),
        "sent again after a day"
    );
    let accepted_at: Option<i64> = anna
        .db
        .read_blocking(|r| {
            Ok(r.query_row(
                "SELECT accepted_at FROM chat_outbox_no_sync WHERE id = 'e1'",
                &[],
                |row| row.get(0),
            )?
            .flatten())
        })
        .expect("row");
    assert_eq!(accepted_at, Some(2_000));

    anna.db
        .write_blocking(|tx| Ok(delivered(tx, MESSAGE, PEER)?))
        .expect("receipt");
    let rows: i64 = anna
        .db
        .read_blocking(|r| {
            Ok(r.query_row(
                "SELECT (SELECT COUNT(*) FROM chat_outbox_no_sync) \
                   + (SELECT COUNT(*) FROM chat_outbox_sealed_no_sync)",
                &[],
                |row| row.get(0),
            )?
            .unwrap_or_default())
        })
        .expect("count");
    assert_eq!(rows, 0, "the row and its wraps are gone");
}

#[test]
fn the_resends_wait_one_day_three_days_then_seven() {
    let waits: Vec<i64> = (0..5).map(resend_after).collect();
    let day = 24 * 60 * 60 * 1000;
    assert_eq!(waits, [day, 3 * day, 7 * day, 7 * day, 7 * day]);
}

/// Spec 095 T031: before a resend, a vault whose receipt came through the sync leaves `awaiting`;
/// the rest drop their devices' wraps and are pending again.
#[tokio::test]
async fn a_resend_drops_the_awaiting_devices_wraps_and_ends_once_every_receipt_came() {
    let (anna, ben) = two_vaults(&[]).await;
    let phone = anna.link();
    let message = anna.send(&ben, "Hallo");
    let ben_vault = ben.me.vault_hex.clone();
    let row = |db: &VaultDb| {
        db.read_blocking(|r| Ok(super::due(r, i64::MAX - 1)?))
            .expect("due")
            .remove(0)
    };
    let entry = row(&anna.db);
    let wraps: Vec<(String, Event)> = [&ben, &phone]
        .iter()
        .map(|to| {
            let event = crate::chat::peer::wrap::wrap(
                nostr::event::Kind::Custom(24122),
                "{}",
                &anna.me.keys.device_secret,
                &to.me.keys.device_pubkey,
                1_000,
            )
            .expect("wrap");
            (to.me.device_hex(), event)
        })
        .collect();
    let id = entry.id.clone();
    anna.db
        .write_blocking(move |tx| {
            for (device, wrap) in &wraps {
                store_sealed(tx, &id, device, wrap)?;
                mark_sealed_accepted(tx, &id, device, 1_000)?;
            }
            tx.execute(
                "UPDATE chat_outbox_no_sync SET pending = '[]' WHERE id = ?1",
                params![id],
            )?;
            Ok(())
        })
        .expect("accepted");
    let entry = row(&anna.db);
    assert!(entry.is_resend());

    let me = anna.me.clone();
    let started = entry.clone();
    let resent = anna
        .db
        .write_blocking(move |tx| Ok(start_resend(tx, &me, &started)?))
        .expect("resend")
        .expect("still awaited");
    assert_eq!(resent.pending, std::slice::from_ref(&ben_vault));
    assert_eq!(resent.resends, 1);
    let (id, phone_hex, ben_hex) = (entry.id.clone(), phone.me.device_hex(), ben.me.device_hex());
    let kept = anna
        .db
        .read_blocking(move |r| Ok((sealed(r, &id, &phone_hex)?, sealed(r, &id, &ben_hex)?)))
        .expect("sealed");
    assert!(kept.0.is_some(), "the own phone's wrap stays");
    assert!(kept.1.is_none(), "Ben's wrap is made again");
    assert_eq!(row(&anna.db).resends, 1);

    // The receipt came to the phone and through the sync to here.
    let delivery = ids::delivery_id(message, &ben_vault).to_string();
    anna.db
        .write_blocking(move |tx| {
            tx.execute(
                "UPDATE chat_deliveries SET delivered_at = 9 WHERE id = ?1",
                params![delivery],
            )?;
            Ok(())
        })
        .expect("delivered");
    let me = anna.me.clone();
    let ended = anna
        .db
        .write_blocking(move |tx| Ok(start_resend(tx, &me, &entry)?))
        .expect("resend");
    assert_eq!(ended, None);
    assert!(anna
        .db
        .read_blocking(|r| Ok(super::due(r, i64::MAX - 1)?))
        .expect("due")
        .is_empty());
}

/// Spec 095 T034 with T031: a message whose conversation was deleted on another own device is not
/// sent again.
#[tokio::test]
async fn a_resend_of_a_deleted_message_ends_the_row() {
    let (anna, ben) = two_vaults(&[]).await;
    anna.send(&ben, "Hallo");
    let id = anna
        .db
        .read_blocking(|r| Ok(super::due(r, i64::MAX - 1)?))
        .expect("due")
        .remove(0)
        .id;
    let row_id = id.clone();
    anna.db
        .write_blocking(move |tx| {
            tx.execute(
                "UPDATE chat_outbox_no_sync SET pending = '[]' WHERE id = ?1",
                params![row_id],
            )?;
            tx.execute("DELETE FROM chat_peer_messages", &[])?;
            Ok(())
        })
        .expect("deleted elsewhere");
    let entry = anna
        .db
        .read_blocking(|r| Ok(super::due(r, i64::MAX - 1)?))
        .expect("due")
        .remove(0);
    let me = anna.me.clone();
    let ended = anna
        .db
        .write_blocking(move |tx| Ok(start_resend(tx, &me, &entry)?))
        .expect("resend");
    assert_eq!(ended, None);
}

/// Spec 099 FR-010, FR-008: an answer "blocked" stops sending to the vault and parks a row with
/// nothing else to do; its delivery fails with `blocked`. "accepted" makes that vault pending
/// again and due at once, and the failure goes; a row for the own devices keeps its own time.
#[test]
fn a_blocked_vault_parks_its_rows_and_an_accepted_one_resumes_them_at_once() {
    let anna = ChatDevice::genesis(&[]);
    anna.db
        .write_blocking(|tx| {
            ensure_delivery(tx, MESSAGE.parse().expect("uuid"), PEER)?;
            enqueue(
                tx,
                "e1",
                "t1",
                Some(MESSAGE),
                &Rumor {
                    kind: 24120,
                    content: "{}".into(),
                },
                &[PEER.to_owned(), OTHER.to_owned()],
                &[PEER.to_owned()],
                1_000,
            )?;
            Ok(())
        })
        .expect("enqueue");

    anna.db
        .write_blocking(|tx| Ok(park_vault(tx, PEER, 2_000)?))
        .expect("park");
    let parked = due(&anna.db, 1_000);
    assert_eq!(parked.len(), 1, "the own devices are still to do");
    assert_eq!(parked[0].pending, [OTHER.to_owned()]);
    assert_eq!(parked[0].awaiting, [PEER.to_owned()]);
    let (_, failed_at, reason) = delivery(&anna.db);
    assert_eq!(
        (failed_at, reason.as_deref()),
        (Some(2_000), Some("blocked"))
    );

    attempt(
        &anna.db,
        &parked[0],
        Attempt {
            accepted: vec![OTHER.to_owned()],
            ..Attempt::default()
        },
        3_000,
    );
    anna.db
        .write_blocking(|tx| Ok(park_vault(tx, PEER, 4_000)?))
        .expect("park again");
    assert_eq!(next_at(&anna.db), i64::MAX, "nothing else to do: parked");
    assert!(due(&anna.db, i64::MAX - 1).is_empty());

    let me = anna.me.clone();
    anna.db
        .write_blocking(move |tx| Ok(resume_vault(tx, &me, PEER, 5_000)?))
        .expect("resume");
    let resumed = due(&anna.db, 5_000);
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].pending, [PEER.to_owned()]);
    assert!(!resumed[0].is_resend(), "first-send path: {resumed:?}");
    let (_, failed_at, reason) = delivery(&anna.db);
    assert_eq!((failed_at, reason), (None, None));
}
