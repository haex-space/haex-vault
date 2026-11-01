//! Which new messages show a notification and what it says (spec 072 FR-023).

use super::*;
use crate::chat::peer::receive::{insert_message, MessageRow};
use crate::chat::peer::store;
use crate::chat::peer::test_support::ChatDevice;

struct Setup {
    anna: ChatDevice,
    thread: Uuid,
    stored: Stored,
}

/// Anna knows Ben ("Kontakt") and holds one message of his.
async fn setup() -> Setup {
    let anna = ChatDevice::genesis(&[]);
    let ben = ChatDevice::genesis(&[]);
    anna.learn(&ben).await;
    let own = anna.me.vault_hex.clone();
    let ben_hex = ben.me.vault_hex.clone();
    let peer = ben_hex.clone();
    let inserted_by = anna.me.device_uuid.clone();
    let message = Uuid::new_v4();
    let thread = anna
        .db
        .write_blocking(move |tx| {
            let thread = store::open_direct(tx, &own, &peer, 1)?;
            insert_message(
                tx,
                &MessageRow {
                    id: message,
                    thread_id: thread,
                    event_id: "e1",
                    sender_vault: &peer,
                    sender_device: "d",
                    text: "Kommst du heute?\nzweite Zeile",
                    sent_at: 10,
                    received_at: 10,
                    inserted_by: &inserted_by,
                },
            )?;
            Ok(thread)
        })
        .expect("thread");
    let stored = Stored {
        thread_id: thread,
        message_id: message,
        peer: ben_hex,
        new: true,
        incoming: true,
    };
    Setup {
        anna,
        thread,
        stored,
    }
}

fn notice_of(setup: &Setup, stored: &Stored) -> Option<Notice> {
    let device = Uuid::parse_str(&setup.anna.me.device_uuid).expect("device");
    let stored = stored.clone();
    setup
        .anna
        .db
        .read_blocking(move |r| Ok(notice(r, device, &stored, "de")?))
        .expect("notice")
}

#[tokio::test]
async fn a_new_message_shows_the_contact_and_the_first_line() {
    let setup = setup().await;
    let notice = notice_of(&setup, &setup.stored).expect("shown");
    assert_eq!(notice.thread_id, setup.thread.to_string());
    assert_eq!(notice.title, "Kontakt");
    assert_eq!(notice.body.as_deref(), Some("Kommst du heute?"));
}

#[tokio::test]
async fn known_own_or_muted_messages_and_switched_off_notifications_show_nothing() {
    let setup = setup().await;
    let known = Stored {
        new: false,
        ..setup.stored.clone()
    };
    assert_eq!(notice_of(&setup, &known), None, "already there");
    let own = Stored {
        incoming: false,
        ..setup.stored.clone()
    };
    assert_eq!(notice_of(&setup, &own), None, "from an own device");

    let thread = setup.thread;
    setup
        .anna
        .db
        .write_blocking(move |tx| Ok(store::set_muted(tx, thread, true)?))
        .expect("mute");
    assert_eq!(notice_of(&setup, &setup.stored), None, "muted");
    setup
        .anna
        .db
        .write_blocking(move |tx| Ok(store::set_muted(tx, thread, false)?))
        .expect("unmute");

    let device = Uuid::parse_str(&setup.anna.me.device_uuid).expect("device");
    setup
        .anna
        .db
        .write_blocking(move |tx| Ok(settings::set_notifications(tx, device, false)?))
        .expect("off");
    assert_eq!(notice_of(&setup, &setup.stored), None, "switched off");
}

#[tokio::test]
async fn a_request_shows_no_name_and_no_text() {
    let setup = setup().await;
    let thread = setup.thread.to_string();
    setup
        .anna
        .db
        .write_blocking(move |tx| {
            tx.execute(
                "UPDATE chat_threads SET state = 'request' WHERE id = ?1",
                params![thread],
            )?;
            Ok(())
        })
        .expect("request");
    let notice = notice_of(&setup, &setup.stored).expect("shown");
    assert_eq!(notice.title, "Neue Anfrage");
    assert_eq!(notice.body, None);
    assert!(!notice.title.contains("Kontakt"));
}

#[test]
fn an_open_conversation_in_a_focused_window_is_not_notified() {
    let registry = PeerRegistry::default();
    registry.set_viewing("view-1".into(), Some("t1".into()));
    registry.set_viewing("view-2".into(), Some("t2".into()));
    assert!(registry.is_viewing("t1"));
    registry.set_viewing("view-1".into(), None);
    assert!(!registry.is_viewing("t1"), "the window lost focus");
    assert!(registry.is_viewing("t2"));
}
