//! Spec 066 T124: the transcript of a spoken conversation is numbered per thread, read in order,
//! keeps values of newer builds, records outcomes and goes with its thread.

// The upgrade case writes a row below the CRDT write path, as a newer device's sync would.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use uuid::Uuid;

use super::chat_threads::{self, ChatThread, ThreadOrigin};
use super::query;
use super::voice_transcript::{
    append, delete_for_thread, list_for_thread, set_outcome, EntryKind, NewEntry, Outcome, Speaker,
};
use crate::passwords::test_support::open_test_vault;

fn entry(thread_id: Uuid, speaker: Speaker, kind: EntryKind, text: &str) -> NewEntry {
    NewEntry {
        thread_id,
        spoken_at: 1_000,
        speaker,
        kind,
        text: text.to_owned(),
        spoken_until: None,
        outcome: None,
        ref_id: None,
    }
}

fn thread(id: Uuid, origin: Option<ThreadOrigin>) -> ChatThread {
    ChatThread {
        id,
        title: "Gespräch".to_owned(),
        last_provider_id: None,
        last_model_id: None,
        created_at: 1,
        updated_at: 1,
        origin,
    }
}

#[test]
fn entries_are_numbered_per_thread_and_read_in_order() {
    let (_dir, db) = open_test_vault();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    db.write(|tx| {
        append(
            tx,
            entry(
                a,
                Speaker::User,
                EntryKind::Utterance,
                "Wie wird das Wetter?",
            ),
            10,
        )?;
        append(
            tx,
            entry(b, Speaker::User, EntryKind::Utterance, "Anderes Gespräch"),
            11,
        )?;
        let answer = NewEntry {
            spoken_until: Some(12),
            ..entry(
                a,
                Speaker::Haex,
                EntryKind::Answer,
                "Morgen regnet es. Am Freitag scheint die Sonne.",
            )
        };
        append(tx, answer, 12)?;
        append(tx, entry(a, Speaker::User, EntryKind::Stop, "Stopp"), 13)?;
        Ok(())
    })
    .expect("append");

    let entries = query::read(&db, |r| list_for_thread(r, a)).expect("list");
    let seen: Vec<(i64, &str, &str)> = entries
        .iter()
        .map(|e| (e.seq, e.speaker.as_str(), e.kind.as_str()))
        .collect();
    assert_eq!(
        seen,
        [
            (1, "user", "utterance"),
            (2, "haex", "answer"),
            (3, "user", "stop")
        ]
    );
    assert_eq!(entries[1].spoken_until, Some(12));
    assert_eq!(entries[1].created_at, 12);
    let other = query::read(&db, |r| list_for_thread(r, b)).expect("list");
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].seq, 1);
}

#[test]
fn an_open_question_gets_its_outcome() {
    let (_dir, db) = open_test_vault();
    let t = Uuid::new_v4();
    let question = db
        .write(|tx| {
            append(
                tx,
                NewEntry {
                    outcome: Some(Outcome::Open),
                    ref_id: Some("request-1".to_owned()),
                    ..entry(
                        t,
                        Speaker::Haex,
                        EntryKind::ApprovalQuestion,
                        "Soll ich die Mail senden?",
                    )
                },
                5,
            )
        })
        .expect("append");
    let changed = db
        .write(|tx| set_outcome(tx, question.id, &Outcome::Approved))
        .expect("set outcome");
    assert_eq!(changed, 1);
    let stored = query::read(&db, |r| list_for_thread(r, t)).expect("list");
    assert_eq!(stored[0].outcome, Some(Outcome::Approved));
    assert_eq!(stored[0].ref_id.as_deref(), Some("request-1"));
}

#[test]
fn equal_seq_values_from_two_devices_read_the_same_everywhere() {
    let (_dir, db) = open_test_vault();
    let t = Uuid::new_v4();
    let first = db
        .write(|tx| append(tx, entry(t, Speaker::User, EntryKind::Utterance, "hier"), 1))
        .expect("append");
    // A second device numbered its entry 1 as well and the sync brought it here; a newer build
    // also used a speaker and a kind this build does not know.
    let other_id = Uuid::new_v4();
    db.with_connection(|conn| {
        conn.execute(
            "INSERT INTO voice_transcript_entries \
               (id, thread_id, seq, spoken_at, speaker, kind, text, created_at) \
             VALUES (?1, ?2, 1, 2, 'participant:jonas', 'summary', 'dort', 2)",
            params![other_id.to_string(), t.to_string()],
        )?;
        Ok(())
    })
    .expect("remote row");

    let entries = query::read(&db, |r| list_for_thread(r, t)).expect("list");
    let mut expected = [first.id, other_id];
    expected.sort();
    assert_eq!(entries.iter().map(|e| e.id).collect::<Vec<_>>(), expected);
    let remote = entries.iter().find(|e| e.id == other_id).expect("remote");
    assert_eq!(
        remote.speaker,
        Speaker::Other("participant:jonas".to_owned())
    );
    assert_eq!(remote.kind.as_str(), "summary");
    // The next local entry follows both.
    let next = db
        .write(|tx| append(tx, entry(t, Speaker::Haex, EntryKind::Answer, "weiter"), 3))
        .expect("append");
    assert_eq!(next.seq, 2);
}

#[test]
fn deleting_a_thread_deletes_its_transcript() {
    let (_dir, db) = open_test_vault();
    let (spoken, kept) = (Uuid::new_v4(), Uuid::new_v4());
    db.write(|tx| {
        chat_threads::insert_thread(tx, &thread(spoken, Some(ThreadOrigin::Voice)))?;
        chat_threads::insert_thread(tx, &thread(kept, None))?;
        append(
            tx,
            entry(spoken, Speaker::User, EntryKind::Utterance, "Hallo"),
            1,
        )?;
        append(
            tx,
            entry(spoken, Speaker::Haex, EntryKind::Closing, "Bis bald"),
            2,
        )?;
        append(
            tx,
            entry(kept, Speaker::User, EntryKind::Utterance, "Bleibt"),
            3,
        )?;
        Ok(())
    })
    .expect("seed");

    let deleted = db
        .write(|tx| chat_threads::delete_thread_and_messages(tx, spoken))
        .expect("delete");
    assert!(deleted);
    assert!(query::read(&db, |r| list_for_thread(r, spoken))
        .expect("list")
        .is_empty());
    assert_eq!(
        query::read(&db, |r| list_for_thread(r, kept))
            .expect("list")
            .len(),
        1
    );
    assert_eq!(
        db.write(|tx| delete_for_thread(tx, kept))
            .expect("delete entries"),
        1
    );
}

#[test]
fn a_thread_keeps_where_it_started() {
    let (_dir, db) = open_test_vault();
    let (spoken, typed) = (Uuid::new_v4(), Uuid::new_v4());
    db.write(|tx| {
        chat_threads::insert_thread(tx, &thread(spoken, Some(ThreadOrigin::Voice)))?;
        chat_threads::insert_thread(tx, &thread(typed, None))?;
        Ok(())
    })
    .expect("seed");
    let read = |id| {
        query::read(&db, |r| chat_threads::get_thread(r, id))
            .expect("read")
            .expect("thread")
            .origin
    };
    assert_eq!(read(spoken), Some(ThreadOrigin::Voice));
    assert_eq!(read(typed), None);
    assert_eq!(ThreadOrigin::from_column(Some("meeting")), None);
}
