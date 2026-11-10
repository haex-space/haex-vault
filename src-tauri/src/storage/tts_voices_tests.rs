//! Spec 066 T142 (storage part): own voices keep their recording once per content, read back by
//! id, and a deleted voice's recording is marked and removed after the grace period.

// The tests read the binaries table directly.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;

use super::query;
use super::tts_voices::{delete, insert, list, prune, recording, NewVoice, ORPHAN_GRACE_DAYS};
use crate::passwords::test_support::open_test_vault;

const DAY: i64 = 86_400_000;

fn voice<'a>(name: &'a str, wav: &'a [u8]) -> NewVoice<'a> {
    NewVoice {
        name,
        language: "de",
        wav,
        sample_rate: 16_000,
        consent_confirmed_at: 42,
    }
}

fn binaries(db: &haex_crdt::Database) -> Vec<(String, Option<String>)> {
    db.with_connection(|conn| {
        let mut statement =
            conn.prepare("SELECT hash, orphaned_at FROM haex_tts_voices_binaries ORDER BY hash")?;
        let rows = statement
            .query_map(params![], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .expect("binaries")
}

#[test]
fn a_voice_keeps_its_recording_and_consent() {
    let (_dir, db) = open_test_vault();
    let stored = db
        .write(|tx| insert(tx, voice("Anna", b"RIFF-anna"), 7))
        .expect("insert");
    assert_eq!(stored.engine, "pocket");
    assert_eq!(stored.consent_confirmed_at, 42);
    assert_eq!(stored.created_at, 7);
    assert_eq!(stored.reference_hash.len(), 64, "SHA-256 in hex");

    let listed = query::read(&db, |r| list(r)).expect("list");
    assert_eq!(listed, vec![stored.clone()]);
    let (language, wav) = query::read(&db, |r| recording(r, &stored.id.to_string()))
        .expect("read")
        .expect("found");
    assert_eq!(language, "de");
    assert_eq!(wav, b"RIFF-anna");
    assert_eq!(
        query::read(&db, |r| recording(r, "unknown")).expect("read"),
        None
    );
}

#[test]
fn two_voices_of_one_recording_share_it_until_both_are_gone() {
    let (_dir, db) = open_test_vault();
    let (a, b) = db
        .write(|tx| {
            Ok((
                insert(tx, voice("A", b"same"), 1)?,
                insert(tx, voice("B", b"same"), 2)?,
            ))
        })
        .expect("insert");
    assert_eq!(binaries(&db).len(), 1);

    assert!(db.write(|tx| delete(tx, a.id)).expect("delete"));
    assert_eq!(binaries(&db)[0].1, None, "B still uses the recording");
    assert!(db.write(|tx| delete(tx, b.id)).expect("delete"));
    assert!(binaries(&db)[0].1.is_some(), "now it is marked");
    assert!(!db.write(|tx| delete(tx, b.id)).expect("delete again"));
    assert!(query::read(&db, |r| list(r)).expect("list").is_empty());
}

#[test]
fn an_unused_recording_goes_after_the_grace_period() {
    let (_dir, db) = open_test_vault();
    let now = crate::passwords::clock::unix_millis(std::time::SystemTime::now());
    let gone = db
        .write(|tx| insert(tx, voice("Weg", b"old"), 1))
        .expect("insert");
    db.write(|tx| insert(tx, voice("Da", b"kept"), 1))
        .expect("insert");
    db.write(|tx| delete(tx, gone.id)).expect("delete");

    assert_eq!(db.write(|tx| prune(tx, now)).expect("prune"), 0);
    assert_eq!(binaries(&db).len(), 2, "still within the grace period");
    let later = now + (ORPHAN_GRACE_DAYS + 1) * DAY;
    assert_eq!(db.write(|tx| prune(tx, later)).expect("prune"), 1);
    assert_eq!(binaries(&db).len(), 1, "the used recording stays");
}
