//! The outbox's back-off, giving up and "Erneut senden" (spec 072 T026, research R6).

use super::*;
use crate::chat::peer::receive::ensure_delivery;
use crate::chat::peer::test_support::ChatDevice;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

const PEER: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";
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
fn an_accepted_vault_is_sent_and_the_row_goes() {
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
    assert!(due(&anna.db, i64::MAX - 1).is_empty());
}

#[test]
fn after_24_hours_the_delivery_fails_and_resend_queues_it_again() {
    let anna = ChatDevice::genesis(&[]);
    let entry = queued(&anna.db, 1_000);
    attempt(&anna.db, &entry, not_yet(), 1_000 + GIVE_UP_MS);
    let (sent, failed, reason) = delivery(&anna.db);
    assert_eq!(sent, None);
    assert_eq!(failed, Some(1_000 + GIVE_UP_MS));
    assert_eq!(reason.as_deref(), Some("noRelay"));
    assert!(due(&anna.db, i64::MAX - 1).is_empty(), "parked");

    let found = anna
        .db
        .write_blocking(|tx| Ok(resend(tx, MESSAGE, 5_000_000_000)?))
        .expect("resend");
    assert!(found);
    assert_eq!(delivery(&anna.db).1, None, "the failure is cleared");
    let again = due(&anna.db, 5_000_000_000);
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].pending, [PEER.to_owned()]);
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

    attempt(&anna.db, &waiting[0], not_yet(), 1_000 + GIVE_UP_MS);
    anna.db
        .write_blocking(|tx| Ok(due_now(tx, 2_000 + GIVE_UP_MS)?))
        .expect("due now");
    assert!(
        due(&anna.db, 2_000 + GIVE_UP_MS).is_empty(),
        "a failed row waits for resend"
    );
}
