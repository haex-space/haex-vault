//! Notes to oneself, the "Ich" entry of the list (spec 072 FR-002b, T119): one conversation per
//! vault with an id every own device derives, notes stored without delivery or outbox rows.

use haex_crdt::rusqlite::params;
use uuid::Uuid;

use super::store::{self, ConversationKind, ListFilter};
use super::test_support::ChatDevice;
use super::{ids, messages, outbox, send};
use crate::storage::query::Query;

fn open(anna: &ChatDevice, now: i64) -> Uuid {
    let own = anna.me.vault_hex.clone();
    anna.db
        .write_blocking(move |tx| Ok(store::open_note(tx, &own, now)?))
        .expect("open notes")
}

fn note(anna: &ChatDevice, thread: Uuid, event: &str, text: &str) -> crate::error::Result<Uuid> {
    let (me, event, text) = (anna.me.clone(), event.to_owned(), text.to_owned());
    anna.db
        .write_blocking(move |tx| Ok(send::message(tx, &me, thread, &event, &text, 500)?))
}

fn count(anna: &ChatDevice, sql: &'static str, thread: Uuid) -> i64 {
    anna.db
        .read_blocking(move |r| {
            Ok(
                r.query_row(sql, params![thread.to_string()], |row| row.get(0))?
                    .unwrap_or_default(),
            )
        })
        .expect("count")
}

#[test]
fn the_notes_open_once_with_the_id_of_the_own_vault_and_only_the_own_vault_as_member() {
    let anna = ChatDevice::genesis(&[]);
    let first = open(&anna, 1);
    assert_eq!(open(&anna, 2), first);
    assert_eq!(first, ids::note_thread_id(&anna.me.vault_hex));
    assert_eq!(
        count(
            &anna,
            "SELECT COUNT(*) FROM chat_members WHERE thread_id = ?1",
            first
        ),
        1
    );
}

#[test]
fn a_note_is_stored_once_without_delivery_or_outbox_and_lists_as_notes() {
    let anna = ChatDevice::genesis(&[]);
    let thread = open(&anna, 1);
    let first = note(&anna, thread, "n1", "  Reifen wechseln  ").expect("note");
    assert_eq!(
        note(&anna, thread, "n1", "Reifen wechseln").expect("retry"),
        first,
        "the same idempotency key stores once"
    );
    assert!(note(&anna, thread, "n2", "   ").is_err(), "an empty note");

    let due = anna
        .db
        .read_blocking(|r| Ok(outbox::due(r, i64::MAX - 1)?))
        .expect("outbox");
    assert!(due.is_empty(), "a note never goes to a Nostr-Relay");
    assert_eq!(
        count(
            &anna,
            "SELECT COUNT(*) FROM chat_deliveries d JOIN chat_peer_messages m \
               ON m.id = d.message_id WHERE m.thread_id = ?1",
            thread
        ),
        0
    );

    let own = anna.me.vault_hex.clone();
    let shown = anna
        .db
        .read_blocking({
            let own = own.clone();
            move |r| Ok(messages::list(r, &own, thread, Default::default())?)
        })
        .expect("messages");
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].text.as_deref(), Some("Reifen wechseln"));
    assert!(shown[0].own);
    assert_eq!(shown[0].delivery, None, "no delivery state for a note");

    let list = anna
        .db
        .read_blocking(move |r| Ok(store::list(r, &own, ListFilter::default())?))
        .expect("list");
    let entry = list
        .iter()
        .find(|c| c.id == thread.to_string())
        .expect("listed");
    assert_eq!(entry.kind, ConversationKind::Note);
    assert_eq!(entry.unread, 0);
    assert_eq!(entry.last_text.as_deref(), Some("Reifen wechseln"));
    assert_eq!(entry.last_at, 500);
}

#[test]
fn deleting_the_notes_removes_them_with_their_notes() {
    let anna = ChatDevice::genesis(&[]);
    let thread = open(&anna, 1);
    note(&anna, thread, "n1", "weg damit").expect("note");
    let deleted = anna
        .db
        .write_blocking(move |tx| Ok(store::delete(tx, thread)?))
        .expect("delete");
    assert!(deleted);
    assert_eq!(
        count(
            &anna,
            "SELECT COUNT(*) FROM chat_peer_messages WHERE thread_id = ?1",
            thread
        ),
        0
    );
}
