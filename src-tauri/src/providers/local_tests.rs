//! Tests for the fixed local provider ids (spec 024).

use haex_crdt::rusqlite::params;

use super::*;
use crate::storage::query;
use crate::sync::test_support::open_vault;

/// A local chat row as installations before spec 024 created it.
fn insert_legacy_row(tx: &mut CrdtTransaction<'_>, id: Uuid, created_at: i64) -> Result<()> {
    insert_provider(
        tx,
        &Provider {
            id,
            kind: ProviderKind::Local,
            adapter: None,
            name: format!("legacy {created_at}"),
            base_url: None,
            credentials: None,
            created_at,
            capability: ProviderCapability::Chat,
        },
    )?;
    Ok(())
}

fn local_chat_ids(db: &haex_crdt::Database) -> Vec<Uuid> {
    query::read(db, |r| {
        provider_ids_by_kind_and_capability(r, ProviderKind::Local, ProviderCapability::Chat)
    })
    .expect("read ids")
}

#[test]
fn a_new_vault_gets_the_fixed_ids() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let chat = db.write(ensure_local_provider).expect("chat");
    let transcription = db
        .write(ensure_local_transcription_provider)
        .expect("transcription");

    assert_eq!(chat, local_provider_id());
    assert_eq!(transcription, local_transcription_provider_id());
    assert_ne!(chat, transcription);
    assert_eq!(db.write(ensure_local_provider).expect("again"), chat);
    assert_eq!(local_chat_ids(&db), vec![chat]);
}

#[test]
fn legacy_rows_move_to_the_fixed_id_with_their_references() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let (oldest, other) = (Uuid::new_v4(), Uuid::new_v4());
    let thread = Uuid::new_v4();
    db.write(|tx| {
        insert_legacy_row(tx, oldest, 10)?;
        insert_legacy_row(tx, other, 20)?;
        tx.execute(
            "INSERT INTO models (id, provider_id, name) VALUES ('m', ?1, 'Model')",
            params![other.to_string()],
        )?;
        tx.execute(
            "INSERT INTO chat_threads (id, title, last_provider_id, created_at, updated_at) \
             VALUES (?1, 't', ?2, 1, 1)",
            params![thread.to_string(), oldest.to_string()],
        )?;
        tx.execute(
            "INSERT INTO chat_messages (id, thread_id, role, content, provider_id, created_at) \
             VALUES (?1, ?2, 'user', 'hi', ?3, 1)",
            params![
                Uuid::new_v4().to_string(),
                thread.to_string(),
                other.to_string()
            ],
        )?;
        Ok(())
    })
    .expect("seed legacy rows");

    let id = db.write(ensure_local_provider).expect("move");

    assert_eq!(id, local_provider_id());
    assert_eq!(local_chat_ids(&db), vec![id]);
    let kept = query::read(&db, |r| get_provider(r, id))
        .expect("read")
        .expect("row");
    assert_eq!((kept.name.as_str(), kept.created_at), ("legacy 10", 10));
    let references: (String, String, String) = query::read(&db, |r| {
        r.query_row(
            "SELECT (SELECT provider_id FROM models), \
                    (SELECT last_provider_id FROM chat_threads), \
                    (SELECT provider_id FROM chat_messages)",
            &[],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map(|row| row.expect("one row"))
    })
    .expect("read references");
    let fixed = id.to_string();
    assert_eq!(references, (fixed.clone(), fixed.clone(), fixed));
    let markers: i64 = query::read(&db, |r| {
        r.query_row(
            "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'providers'",
            &[],
            |row| row.get(0),
        )
        .map(|n| n.unwrap_or(0))
    })
    .expect("count markers");
    assert_eq!(markers, 2, "both legacy rows leave a delete marker");
}

#[test]
fn an_older_legacy_row_wins_over_an_existing_fixed_row() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let legacy = Uuid::new_v4();
    db.write(|tx| {
        insert_legacy_row(tx, legacy, 10)?;
        insert_provider(
            tx,
            &Provider {
                id: local_provider_id(),
                kind: ProviderKind::Local,
                adapter: None,
                name: "new fixed value".to_string(),
                base_url: None,
                credentials: None,
                created_at: 20,
                capability: ProviderCapability::Chat,
            },
        )?;
        Ok(())
    })
    .expect("seed providers");

    db.write(ensure_local_provider).expect("move");

    let fixed = query::read(&db, |r| get_provider(r, local_provider_id()))
        .expect("read")
        .expect("fixed row");
    assert_eq!((fixed.name.as_str(), fixed.created_at), ("legacy 10", 10));
    assert_eq!(local_chat_ids(&db), vec![local_provider_id()]);
}

/// The ids are UUIDv5 over fixed texts; the expected values come from Python's `uuid` (spec 054 S3).
#[test]
fn the_local_provider_ids_have_fixed_values() {
    assert_eq!(
        local_provider_id().to_string(),
        "49e0c716-77d8-5fab-90aa-cfa350575950"
    );
    assert_eq!(
        local_transcription_provider_id().to_string(),
        "9a578e87-046a-5d03-af47-4b74fd3b91b6"
    );
}
