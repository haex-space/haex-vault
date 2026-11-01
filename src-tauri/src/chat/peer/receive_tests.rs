//! Storing verified rumors (spec 072 research R4, R6): one row per message however often it
//! arrives, the clock rule of `sort_at`, own copies with their delivery row, receipts set once.

use super::*;
use crate::chat::peer::envelope::{self, VERSION};
use crate::chat::peer::test_support::ChatDevice;
use crate::storage::query::Query;

const BEN: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";

fn store(anna: &ChatDevice, envelope: &Envelope, now: i64) -> Stored {
    let (me, envelope) = (anna.me.clone(), envelope.clone());
    anna.db
        .write_blocking(move |tx| Ok(message(tx, me.receiver(), "dev", &envelope, now)?))
        .expect("store")
        .expect("direct")
}

fn count(anna: &ChatDevice, sql: &'static str) -> i64 {
    anna.db
        .read_blocking(move |r| Ok(r.query_row(sql, &[], |row| row.get(0))?.unwrap_or(0)))
        .expect("count")
}

#[test]
fn a_message_arriving_twice_is_one_row_ordered_by_the_clock_rule() {
    let anna = ChatDevice::genesis(&[]);
    let at = 10_000_000;
    let from_ben = envelope::message(
        "Hallo",
        BEN,
        "card",
        To::Direct(anna.me.vault_hex.clone()),
        at,
    )
    .expect("message");
    let first = store(&anna, &from_ben, at + 1_000);
    assert!(first.new && first.incoming);
    assert_eq!(first.peer, BEN);
    let second = store(&anna, &from_ben, at + 2_000);
    assert!(!second.new);
    assert_eq!(second.message_id, first.message_id);
    assert_eq!(count(&anna, "SELECT COUNT(*) FROM chat_peer_messages"), 1);

    // A sender whose clock runs an hour ahead is placed at the arrival time plus 5 minutes.
    let ahead = envelope::message(
        "aus der Zukunft",
        BEN,
        "card",
        To::Direct(anna.me.vault_hex.clone()),
        at + 3_600_000,
    )
    .expect("message");
    store(&anna, &ahead, at);
    assert_eq!(
        count(&anna, "SELECT MAX(sort_at) FROM chat_peer_messages"),
        at + CLOCK_SLACK_MS
    );
}

#[test]
fn an_own_message_from_another_own_device_gets_its_delivery_row() {
    let anna = ChatDevice::genesis(&[]);
    let own = envelope::message(
        "an Ben",
        &anna.me.vault_hex,
        "card",
        To::Direct(BEN.into()),
        5,
    )
    .expect("message");
    let stored = store(&anna, &own, 6);
    assert!(stored.new);
    assert!(!stored.incoming, "no delivery receipt for the own copy");
    assert_eq!(
        stored.thread_id,
        ids::direct_thread_id(&anna.me.vault_hex, BEN)
    );
    assert_eq!(count(&anna, "SELECT COUNT(*) FROM chat_deliveries"), 1);
}

#[test]
fn a_receipt_sets_each_state_once() {
    let anna = ChatDevice::genesis(&[]);
    let own = envelope::message(
        "an Ben",
        &anna.me.vault_hex,
        "card",
        To::Direct(BEN.into()),
        5,
    )
    .expect("message");
    store(&anna, &own, 5);
    let receipt_of = |delivered: Vec<String>, read: Vec<String>| Receipt {
        v: VERSION,
        from: BEN.into(),
        card: "card".into(),
        thread: To::Direct(anna.me.vault_hex.clone()),
        delivered,
        read,
    };
    let apply = |receipt: Receipt, now: i64| {
        let own = anna.me.vault_hex.clone();
        anna.db
            .write_blocking(move |tx| Ok(super::receipt(tx, &own, &receipt, now)?))
            .expect("apply")
    };
    assert_eq!(apply(receipt_of(vec![own.id.clone()], vec![]), 7), 1);
    assert_eq!(
        apply(receipt_of(vec![own.id.clone()], vec![]), 8),
        0,
        "once"
    );
    assert_eq!(apply(receipt_of(vec![], vec![own.id.clone()]), 9), 1);
    assert_eq!(
        count(
            &anna,
            "SELECT delivered_at * 100 + read_at FROM chat_deliveries"
        ),
        709
    );
    assert_eq!(apply(receipt_of(vec!["unknown".into()], vec![]), 10), 0);
}
