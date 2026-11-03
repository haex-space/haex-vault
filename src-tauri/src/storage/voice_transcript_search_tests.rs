//! Spec 066 T127: transcripts are searched with the shared folding, a page at a time, in a stable
//! order that a cursor resumes strictly after.

use ::uuid::Uuid;

use super::*;
use crate::passwords::test_support::open_test_vault;
use crate::storage::query;
use crate::storage::voice_transcript::{append, EntryKind, NewEntry, Speaker};

fn said(thread_id: Uuid, text: &str) -> NewEntry {
    NewEntry {
        thread_id,
        spoken_at: 1_000,
        speaker: Speaker::User,
        kind: EntryKind::Utterance,
        text: text.to_owned(),
        spoken_until: None,
        outcome: None,
        ref_id: None,
    }
}

fn page(
    db: &haex_crdt::Database,
    query: &str,
    cursor: Option<&str>,
    limit: Option<u32>,
) -> SearchPage {
    let request = SearchRequest::new(query, cursor, limit).expect("request");
    query::read(db, |r| search(r, &request)).expect("search")
}

#[test]
fn case_and_accents_do_not_matter() {
    let (_dir, db) = open_test_vault();
    let thread = Uuid::new_v4();
    db.write(|tx| {
        append(tx, said(thread, "Wie wird das Wetter in Köln?"), 1)?;
        append(tx, said(thread, "Erinnere mich an den Zahnarzt"), 2)?;
        Ok(())
    })
    .expect("append");

    let hits = page(&db, "KOLN", None, None).entries;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].thread_id, thread);
    assert_eq!(hits[0].seq, 1);
    assert_eq!(hits[0].snippet, "Wie wird das Wetter in Köln?");
    assert!(page(&db, "  ", None, None).entries.is_empty());
    assert!(page(&db, "Urlaub", None, None).entries.is_empty());
}

#[test]
fn a_snippet_keeps_forty_characters_around_the_match() {
    let before = "a".repeat(60);
    let after = "b".repeat(60);
    let text = format!("{before} Müller {after}");
    let snippet = snippet(&text, &fold("muller").0).expect("match");
    assert_eq!(
        snippet,
        format!("…{} Müller {}…", "a".repeat(39), "b".repeat(39))
    );
    // A decomposed "ü" folds to one character less; the snippet still keeps the original text.
    let decomposed = format!("{} Mu\u{308}ller", "a".repeat(45));
    assert_eq!(
        snippet_of(&decomposed, "MÜLLER").as_deref(),
        Some(format!("…{} Mu\u{308}ller", "a".repeat(39)).as_str())
    );
}

fn snippet_of(text: &str, query: &str) -> Option<String> {
    snippet(text, &fold(query).0)
}

#[test]
fn pages_follow_thread_seq_and_id_and_resume_after_the_cursor() {
    let (_dir, db) = open_test_vault();
    let mut threads = [Uuid::new_v4(), Uuid::new_v4()];
    threads.sort_by_key(|id| id.to_string());
    db.write(|tx| {
        for (n, thread) in threads.iter().enumerate() {
            for i in 0..3 {
                append(tx, said(*thread, &format!("Budget {n}.{i}")), 1)?;
            }
            append(tx, said(*thread, "etwas anderes"), 1)?;
        }
        Ok(())
    })
    .expect("append");

    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let result = page(&db, "budget", cursor.as_deref(), Some(2));
        assert!(result.entries.len() <= 2, "the limit holds");
        seen.extend(result.entries.into_iter().map(|h| (h.thread_id, h.seq)));
        match result.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    let expected: Vec<(Uuid, i64)> = threads
        .iter()
        .flat_map(|thread| (1..=3).map(move |seq| (*thread, seq)))
        .collect();
    assert_eq!(seen, expected, "every hit once, in (thread, seq) order");
}

#[test]
fn the_last_page_has_no_cursor() {
    let (_dir, db) = open_test_vault();
    let thread = Uuid::new_v4();
    db.write(|tx| {
        append(tx, said(thread, "Budget"), 1)?;
        append(tx, said(thread, "Budget"), 2)?;
        Ok(())
    })
    .expect("append");
    let full = page(&db, "budget", None, Some(2));
    assert_eq!(full.entries.len(), 2);
    assert_eq!(full.next_cursor, None);
}

#[test]
fn the_request_checks_limit_and_cursor() {
    assert_eq!(
        SearchRequest::new("x", None, None).expect("default").limit,
        DEFAULT_LIMIT
    );
    assert!(SearchRequest::new("x", None, Some(1)).is_ok());
    assert!(SearchRequest::new("x", None, Some(100)).is_ok());
    assert!(SearchRequest::new("x", None, Some(0)).is_err());
    assert!(SearchRequest::new("x", None, Some(101)).is_err());
    assert!(SearchRequest::new("x", Some("kaputt"), None).is_err());

    let cursor = Cursor {
        thread_id: Uuid::new_v4(),
        seq: 7,
        id: Uuid::new_v4(),
    };
    assert_eq!(Cursor::decode(&cursor.encode()), Some(cursor));
    assert_eq!(Cursor::decode(&format!("{}:x", cursor.encode())), None);
}
