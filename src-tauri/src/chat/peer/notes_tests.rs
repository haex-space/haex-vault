//! Notes to oneself, the "Notiz an mich selber" entry of the list (spec 072 FR-002b, T119, T143,
//! T144): one conversation per vault with an id every own device derives, notes stored without
//! delivery rows. A note is sent like a message to the own other devices; one that no other device
//! is reached by stays in the vault.

use haex_crdt::rusqlite::params;
use uuid::Uuid;

use super::envelope::{self, Attachment, BlobRef, Body, FileAttachment, To};
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
    anna.db.write_blocking(move |tx| {
        Ok(send::message(
            tx,
            &me,
            thread,
            &event,
            &text,
            Vec::new(),
            500,
        )?)
    })
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
fn a_note_is_stored_once_without_delivery_and_lists_as_notes() {
    let anna = ChatDevice::genesis(&[]);
    let thread = open(&anna, 1);
    let own = anna.me.vault_hex.clone();
    let before_first_note = anna
        .db
        .read_blocking(move |r| Ok(store::list(r, &own, ListFilter::default())?))
        .expect("empty notes list");
    let entry = before_first_note
        .iter()
        .find(|conversation| conversation.id == thread.to_string())
        .expect("listed before the first note");
    assert_eq!(entry.last_at, 0, "an empty notes thread has no activity");

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
    assert!(
        due.is_empty(),
        "a note no other device is reached by is not published"
    );
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

const RELAY: &str = "wss://relay.example.org";

fn due(anna: &ChatDevice) -> Vec<outbox::Entry> {
    anna.db
        .read_blocking(|r| Ok(outbox::due(r, i64::MAX - 1)?))
        .expect("outbox")
}

fn attachment() -> Attachment {
    Attachment::File(FileAttachment {
        name: "reifen.pdf".into(),
        mime: "application/pdf".into(),
        size: 3,
        sha256: "ab".repeat(32),
        blossom: BlobRef {
            url: "https://blossom.example.org/cd".into(),
            sha256: "cd".repeat(32),
            key: "ef".repeat(32),
        },
        inline: false,
    })
}

#[test]
fn a_note_with_another_own_device_is_queued_for_the_own_vault_only() {
    let anna = ChatDevice::genesis(&[RELAY.to_owned()]);
    let _phone = anna.link();
    let thread = open(&anna, 1);
    let own = anna.me.vault_hex.clone();
    let me = anna.me.clone();
    let message = anna
        .db
        .write_blocking(move |tx| {
            Ok(send::message(
                tx,
                &me,
                thread,
                "n1",
                "Reifen wechseln",
                vec![attachment()],
                500,
            )?)
        })
        .expect("note with a file");

    let queued = due(&anna);
    assert_eq!(queued.len(), 1, "one outbox row");
    assert_eq!(queued[0].pending, vec![own.clone()], "only the own devices");
    assert!(queued[0].awaiting.is_empty(), "no receipt to wait for");
    let envelope: envelope::Envelope =
        serde_json::from_str(&queued[0].rumor.content).expect("the envelope");
    assert_eq!(envelope.to, To::Note(own.clone()));
    let Body::Message(body) = envelope.body;
    assert_eq!(body.text, "Reifen wechseln");
    assert_eq!(body.attachments, vec![attachment()]);
    assert_eq!(
        count(
            &anna,
            "SELECT COUNT(*) FROM chat_deliveries d JOIN chat_peer_messages m \
               ON m.id = d.message_id WHERE m.thread_id = ?1",
            thread
        ),
        0,
        "no delivery state"
    );
    assert_eq!(
        count(
            &anna,
            "SELECT COUNT(*) FROM chat_attachments a JOIN chat_peer_messages m \
               ON m.id = a.message_id WHERE m.thread_id = ?1",
            thread
        ),
        1,
        "the own copy of the row names the file"
    );
    assert_eq!(
        note(&anna, thread, "n1", "x").expect("retry"),
        message,
        "a retry answers the first note"
    );
    assert_eq!(due(&anna).len(), 1, "and queues nothing more");
}

#[test]
fn a_note_without_a_nostr_relay_is_stored_and_not_queued() {
    let anna = ChatDevice::genesis(&[]);
    let _phone = anna.link();
    let thread = open(&anna, 1);
    note(&anna, thread, "n1", "Reifen wechseln").expect("note");
    assert!(due(&anna).is_empty());
}

#[test]
fn a_long_note_falls_to_a_file_only_when_it_is_published() {
    let long = "ä".repeat(30_000);
    let published = ChatDevice::genesis(&[RELAY.to_owned()]);
    let _phone = published.link();
    let thread = open(&published, 1);
    let (me, text) = (published.me.clone(), long.clone());
    let fits = published
        .db
        .read_blocking(move |r| Ok(send::fits_inline(r, &me, thread, &text, 500)?))
        .expect("probe");
    assert!(!fits, "a published note of 60 KB does not fit one wrap");

    let alone = ChatDevice::genesis(&[]);
    let thread = open(&alone, 1);
    let (me, text) = (alone.me.clone(), long);
    let fits = alone
        .db
        .read_blocking(move |r| Ok(send::fits_inline(r, &me, thread, &text, 500)?))
        .expect("probe");
    assert!(fits, "a note nobody else receives has no wrap to fit");
}
