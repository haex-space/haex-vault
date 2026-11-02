//! Typed writes and reads for the `chat_threads` table.
//!
//! Threads are conversation containers. Individual messages live in the
//! sibling `chat_messages` module and reference `thread_id` here.
//!
//! Since spec 072 `chat_threads` also holds conversations with contacts (`kind` `direct` or
//! `group`, see `chat::peer::store`). Every reader and every change here is limited to
//! `kind = 'agent'`, so the agent's commands never see or change a contact conversation; new rows
//! get the column default `agent`.
//!
//! Since spec 066 a thread knows where it started (`origin`, migration `0050_voice_conversations`):
//! `NULL` for a typed conversation, `'voice'` for one started by speaking. It is set once when the
//! thread is created and never changed, so a typed reply in a spoken conversation keeps `voice`.

use ::uuid::Uuid;
use haex_crdt::rusqlite::{params, Result};
use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};

use crate::storage::chat_messages;
use crate::storage::query::Query;
use crate::storage::uuid;
use crate::storage::voice_transcript;

/// Where a conversation started (spec 066, data-model.md). A typed conversation has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThreadOrigin {
    /// Started by speaking to the assistant.
    Voice,
}

impl ThreadOrigin {
    /// The value stored in `chat_threads.origin`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Voice => "voice",
        }
    }

    /// Reads `chat_threads.origin`. A value this build does not know counts as typed, so a newer
    /// device's origin never makes a thread unreadable.
    pub fn from_column(value: Option<&str>) -> Option<Self> {
        match value? {
            "voice" => Some(Self::Voice),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatThread {
    pub id: Uuid,
    pub title: String,
    pub last_provider_id: Option<Uuid>,
    pub last_model_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// `None` for a typed conversation (spec 066).
    pub origin: Option<ThreadOrigin>,
}

/// Inserts a fresh chat thread.
pub fn insert_thread(tx: &mut CrdtTransaction<'_>, t: &ChatThread) -> haex_crdt::Result<usize> {
    tx.execute(
        "INSERT INTO chat_threads \
           (id, title, last_provider_id, last_model_id, created_at, updated_at, origin) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            t.id.to_string(),
            t.title,
            t.last_provider_id.map(|u| u.to_string()),
            t.last_model_id,
            t.created_at,
            t.updated_at,
            t.origin.map(ThreadOrigin::as_str),
        ],
    )
}

/// Removes a thread created solely for a generation that failed to start.
pub fn delete_thread(tx: &mut CrdtTransaction<'_>, id: Uuid) -> haex_crdt::Result<usize> {
    tx.execute(
        "DELETE FROM chat_threads WHERE id = ?1 AND kind = 'agent'",
        params![id.to_string()],
    )
}

/// Renames a thread without changing its opening time or recency ordering.
pub fn rename_title(
    tx: &mut CrdtTransaction<'_>,
    id: Uuid,
    title: &str,
) -> haex_crdt::Result<usize> {
    tx.execute(
        "UPDATE chat_threads SET title = ?1 WHERE id = ?2 AND kind = 'agent'",
        params![title, id.to_string()],
    )
}

/// Deletes a thread, its messages and its voice transcript; run it in one `write` so both go or neither.
///
/// The boolean is false when the thread did not exist. In that case no
/// message row is touched, even if a malformed database contains rows with
/// the same thread id.
pub fn delete_thread_and_messages(
    tx: &mut CrdtTransaction<'_>,
    id: Uuid,
) -> haex_crdt::Result<bool> {
    let exists: Option<i64> = tx.query_row(
        "SELECT 1 FROM chat_threads WHERE id = ?1 AND kind = 'agent'",
        params![id.to_string()],
        |row| row.get(0),
    )?;
    if exists.is_none() {
        return Ok(false);
    }

    // Spec 066: the transcript of a spoken conversation goes with it, each entry with its own
    // delete marker (no cascading foreign key).
    voice_transcript::delete_for_thread(tx, id)?;
    chat_messages::delete_for_thread(tx, id)?;
    let deleted = delete_thread(tx, id)?;
    if deleted != 1 {
        return Err(haex_crdt::rusqlite::Error::QueryReturnedNoRows.into());
    }
    Ok(true)
}

/// Updates a thread's title, last provider/model pointer and `updated_at`.
pub fn update_thread(
    tx: &mut CrdtTransaction<'_>,
    id: Uuid,
    title: &str,
    last_provider_id: Option<Uuid>,
    last_model_id: Option<&str>,
    updated_at: i64,
) -> haex_crdt::Result<usize> {
    tx.execute(
        "UPDATE chat_threads SET \
           title = ?1, last_provider_id = COALESCE(?2, last_provider_id), last_model_id = ?3, \
           updated_at = ?4 \
         WHERE id = ?5 AND kind = 'agent'",
        params![
            title,
            last_provider_id.map(|u| u.to_string()),
            last_model_id,
            updated_at,
            id.to_string(),
        ],
    )
}

/// Lists all threads, most-recently-updated first.
pub fn list_threads(q: &mut impl Query) -> haex_crdt::Result<Vec<ChatThread>> {
    q.query_map(
        "SELECT id, title, last_provider_id, last_model_id, created_at, updated_at, origin \
         FROM chat_threads WHERE kind = 'agent' ORDER BY updated_at DESC",
        &[],
        row_to_thread,
    )
}

/// Fetches a single thread by id.
pub fn get_thread(q: &mut impl Query, id: Uuid) -> haex_crdt::Result<Option<ChatThread>> {
    q.query_row(
        "SELECT id, title, last_provider_id, last_model_id, created_at, updated_at, origin \
         FROM chat_threads WHERE id = ?1 AND kind = 'agent'",
        params![id.to_string()],
        row_to_thread,
    )
}

fn row_to_thread(row: &haex_crdt::rusqlite::Row<'_>) -> Result<ChatThread> {
    let last_provider_str: Option<String> = row.get(2)?;
    Ok(ChatThread {
        id: uuid::from_row(row, 0)?,
        title: row.get(1)?,
        last_provider_id: uuid::optional(last_provider_str, 2)?,
        last_model_id: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        origin: ThreadOrigin::from_column(row.get::<_, Option<String>>(6)?.as_deref()),
    })
}
