//! Queuing messages and receipts (spec 072 US2, FR-012, FR-022, FR-024).

use super::*;
use crate::chat::peer::test_support::ChatDevice;

const BEN: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";

fn direct(anna: &ChatDevice) -> Uuid {
    let own = anna.me.vault_hex.clone();
    anna.db
        .write_blocking(move |tx| Ok(store::open_direct(tx, &own, BEN, 1)?))
        .expect("open")
}

fn send(anna: &ChatDevice, thread: Uuid, event: &str, text: &str) -> Result<Uuid> {
    let (me, event, text) = (anna.me.clone(), event.to_owned(), text.to_owned());
    anna.db
        .write_blocking(move |tx| Ok(message(tx, &me, thread, &event, &text, 100)?))
}

fn outbox(anna: &ChatDevice) -> Vec<outbox::Entry> {
    anna.db
        .read_blocking(|r| Ok(outbox::due(r, i64::MAX - 1)?))
        .expect("outbox")
}

#[test]
fn a_message_becomes_its_row_a_delivery_and_an_outbox_row_once() {
    let anna = ChatDevice::genesis(&[]);
    let thread = direct(&anna);
    let first = send(&anna, thread, "e1", "  Kommst du?  ").expect("send");
    let retry = send(&anna, thread, "e1", "Kommst du?").expect("retry");
    assert_eq!(first, retry, "the same idempotency key sends once");

    let rows = outbox(&anna);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].message_id.as_deref(),
        Some(first.to_string().as_str())
    );
    assert_eq!(rows[0].pending, [BEN.to_owned(), anna.me.vault_hex.clone()]);
    let rumor: envelope::Envelope = serde_json::from_str(&rows[0].rumor.content).expect("rumor");
    assert_eq!(rumor.to, To::Direct(BEN.into()));
    assert_eq!(rumor.from, anna.me.vault_hex);
    assert!(
        rumor.card.starts_with("holzi:card:"),
        "R5: the own card rides along"
    );

    let list = anna
        .db
        .read_blocking({
            let own = anna.me.vault_hex.clone();
            move |r| {
                Ok(crate::chat::peer::messages::list(
                    r,
                    &own,
                    thread,
                    Default::default(),
                )?)
            }
        })
        .expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].text.as_deref(), Some("Kommst du?"));
    assert!(list[0].own);
    assert_eq!(
        list[0].delivery,
        Some(crate::chat::peer::messages::DeliveryState::Waiting)
    );
}

#[test]
fn an_empty_or_too_long_text_or_an_agent_thread_is_refused() {
    let anna = ChatDevice::genesis(&[]);
    let thread = direct(&anna);
    assert!(send(&anna, thread, "e1", "   ").is_err());
    let long = "a".repeat(envelope::MAX_TEXT_CHARS + 1);
    let error = send(&anna, thread, "e2", &long).expect_err("too long");
    assert!(error.to_string().contains("chat.error.tooLarge"), "{error}");
    assert!(send(&anna, Uuid::new_v4(), "e3", "hallo").is_err());
    assert!(outbox(&anna).is_empty());
}

fn incoming(anna: &ChatDevice, thread: Uuid, event: &str, sort_at: i64) {
    let event = event.to_owned();
    anna.db
        .write_blocking(move |tx| {
            receive::insert_message(
                tx,
                &MessageRow {
                    id: ids::message_id(thread, BEN, &event),
                    thread_id: thread,
                    event_id: &event,
                    sender_vault: BEN,
                    sender_device: "dev",
                    text: "hallo",
                    sent_at: sort_at,
                    received_at: sort_at,
                    inserted_by: "d",
                },
            )?;
            Ok(())
        })
        .expect("incoming");
}

fn read(anna: &ChatDevice, thread: Uuid, upto: i64) -> bool {
    let me = anna.me.clone();
    anna.db
        .write_blocking(move |tx| Ok(read_up_to(tx, &me, thread, upto, 500)?))
        .expect("read")
}

#[test]
fn reading_queues_a_read_receipt_for_the_newly_read_messages_only_with_receipts_on() {
    let anna = ChatDevice::genesis(&[]);
    let thread = direct(&anna);
    incoming(&anna, thread, "e1", 10);
    incoming(&anna, thread, "e2", 20);
    assert!(read(&anna, thread, 10));
    assert!(!read(&anna, thread, 10), "nothing new");
    assert!(read(&anna, thread, 20));
    let receipts: Vec<Receipt> = outbox(&anna)
        .iter()
        .map(|row| serde_json::from_str(&row.rumor.content).expect("receipt"))
        .collect();
    let mut read_sets: Vec<Vec<String>> = receipts.iter().map(|r| r.read.clone()).collect();
    read_sets.sort();
    assert_eq!(read_sets, [vec!["e1".to_owned()], vec!["e2".to_owned()]]);
    assert!(receipts.iter().all(|r| r.thread == To::Direct(BEN.into())));

    incoming(&anna, thread, "e3", 30);
    anna.db
        .write_blocking(|tx| Ok(settings::set_read_receipts(tx, false)?))
        .expect("off");
    assert!(read(&anna, thread, 30));
    assert_eq!(outbox(&anna).len(), 2, "FR-024: off sends none");
}
