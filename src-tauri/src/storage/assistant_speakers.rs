//! The people the voice assistant listens to (spec 066 FR-016a, T174): a row in
//! `haex_assistant_speakers` per person with their voice print (an embedding, little-endian `f32`,
//! never audio) and the model it was made with. The table syncs, so a person enrolled on one
//! device is recognized on every device of the vault; deleting removes them everywhere.

use ::uuid::Uuid;
use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use crate::storage::query::Query;
use crate::storage::uuid;

/// The longest name of a person.
pub const MAX_NAME_CHARS: usize = 80;

/// One enrolled person as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Speaker {
    pub id: Uuid,
    pub name: String,
    /// The embedding model the print was made with.
    pub model: String,
    pub embedding: Vec<f32>,
    /// How many utterances the print averages.
    pub utterances: i64,
    pub created_at: i64,
}

fn to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn from_bytes(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

/// Stores a new person.
pub fn insert(
    tx: &mut CrdtTransaction<'_>,
    name: &str,
    model: &str,
    embedding: &[f32],
    utterances: i64,
    now: i64,
) -> haex_crdt::Result<Speaker> {
    let stored = Speaker {
        id: Uuid::new_v4(),
        name: name.to_owned(),
        model: model.to_owned(),
        embedding: embedding.to_vec(),
        utterances,
        created_at: now,
    };
    tx.execute(
        "INSERT INTO haex_assistant_speakers (id, name, model, embedding, utterances, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            stored.id.to_string(),
            stored.name,
            stored.model,
            to_bytes(&stored.embedding),
            stored.utterances,
            stored.created_at,
        ],
    )?;
    Ok(stored)
}

/// Every enrolled person, by name.
pub fn list(q: &mut impl Query) -> haex_crdt::Result<Vec<Speaker>> {
    q.query_map(
        "SELECT id, name, model, embedding, utterances, created_at FROM haex_assistant_speakers \
         ORDER BY name COLLATE NOCASE, id",
        &[],
        |row| {
            Ok(Speaker {
                id: uuid::from_row(row, 0)?,
                name: row.get(1)?,
                model: row.get(2)?,
                embedding: from_bytes(&row.get::<_, Vec<u8>>(3)?),
                utterances: row.get(4)?,
                created_at: row.get(5)?,
            })
        },
    )
}

/// Renames the person `id`; returns whether they exist.
pub fn rename(tx: &mut CrdtTransaction<'_>, id: Uuid, name: &str) -> haex_crdt::Result<bool> {
    let changed = tx.execute(
        "UPDATE haex_assistant_speakers SET name = ?1 WHERE id = ?2",
        params![name, id.to_string()],
    )?;
    Ok(changed > 0)
}

/// Deletes the person `id` on every device; returns whether they existed.
pub fn delete(tx: &mut CrdtTransaction<'_>, id: Uuid) -> haex_crdt::Result<bool> {
    let changed = tx.execute(
        "DELETE FROM haex_assistant_speakers WHERE id = ?1",
        params![id.to_string()],
    )?;
    Ok(changed > 0)
}
