//! The one list of conversations and the rows of direct conversations (spec 072 US1, data-model.md).
//!
//! `chat_threads` holds every conversation; `kind` tells agent, direct and group apart (FR-001).
//! Agent rows keep their own storage in [`crate::storage::chat_threads`]; this module adds the list
//! over all kinds and the rows of contact conversations. Writes check before they insert, because
//! the synced tables carry no UNIQUE constraint (ids from [`super::ids`]).

use haex_crdt::rusqlite::{params, Row};
use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::ids;
use crate::contacts::card::received;
use crate::error::{HolziError, Result};
use crate::storage::chat_threads as agent_threads;
use crate::storage::query::Query;
use crate::sync::presence::decode_hex;

/// What a conversation is (FR-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ChatConversationKind"
)]
#[serde(rename_all = "camelCase")]
pub enum ConversationKind {
    /// A conversation with a model or a delegate (specs 003, 007, 068).
    Agent,
    /// The user and one contact.
    Direct,
    /// The user and two or more contacts, with a name.
    Group,
}

impl ConversationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Direct => "direct",
            Self::Group => "group",
        }
    }

    fn parse(raw: &str) -> haex_crdt::rusqlite::Result<Self> {
        match raw {
            "agent" => Ok(Self::Agent),
            "direct" => Ok(Self::Direct),
            "group" => Ok(Self::Group),
            other => Err(invalid_text("chat_threads.kind", other)),
        }
    }
}

/// Where a conversation stands (data-model.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ChatConversationState"
)]
#[serde(rename_all = "camelCase")]
pub enum ConversationState {
    Active,
    /// Started by a vault without a confirmed contact, or a group invitation (FR-040).
    Request,
    /// A group the user left or was removed from: readable, no composer (FR-055).
    Closed,
}

impl ConversationState {
    fn parse(raw: &str) -> haex_crdt::rusqlite::Result<Self> {
        match raw {
            "active" => Ok(Self::Active),
            "request" => Ok(Self::Request),
            "closed" => Ok(Self::Closed),
            other => Err(invalid_text("chat_threads.state", other)),
        }
    }
}

/// One entry of the conversation list (FR-001, contracts/ipc.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ChatConversation"
)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub kind: ConversationKind,
    pub state: ConversationState,
    /// Agent: the thread title; group: its name; direct: empty (the window shows the contact).
    pub title: String,
    /// Direct: the other vault identity, hex.
    pub peer_vault: Option<String>,
    /// Direct: the address-book contact holding the other vault's card, if any.
    pub peer_contact_id: Option<String>,
    /// Direct: the name on the other vault's stored card, shown when there is no contact.
    pub peer_card_name: Option<String>,
    /// Direct: the stored card names no Nostr-Relay, so messages go over the own ones (US2
    /// scenario 8, shown as a hint).
    pub peer_without_relays: bool,
    pub muted: bool,
    /// Messages of others after `last_read_at` (FR-001, FR-022).
    #[ts(type = "number")]
    pub unread: i64,
    /// The start of the last contact message, if any.
    pub last_text: Option<String>,
    /// Newest activity: the last message or the thread's own `updated_at`.
    #[ts(type = "number")]
    pub last_at: i64,
}

/// Filter of the list (FR-002).
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFilter {
    pub kind: Option<ConversationKind>,
    /// `true`: only requests; otherwise requests are left out (they have their own area).
    #[serde(default)]
    pub requests: bool,
}

/// How long the preview of the last message may be, in characters.
const PREVIEW_CHARS: usize = 120;

struct Row0 {
    id: String,
    kind: ConversationKind,
    state: ConversationState,
    title: String,
    peer_vault: Option<String>,
    muted: bool,
    unread: i64,
    last_text: Option<String>,
    last_at: i64,
}

/// Every conversation matching `filter`, newest activity first. `own_vault` is the vault identity
/// (hex) whose own messages never count as unread.
pub fn list(q: &mut impl Query, own_vault: &str, filter: ListFilter) -> Result<Vec<Conversation>> {
    let state_clause = if filter.requests {
        "t.state = 'request'"
    } else {
        "t.state <> 'request'"
    };
    let kind = filter.kind.map(ConversationKind::as_str);
    let sql = format!(
        "SELECT t.id, t.kind, t.state, t.title, t.peer_vault, t.muted, \
           (SELECT COUNT(*) FROM chat_peer_messages m \
              WHERE m.thread_id = t.id AND m.sort_at > COALESCE(t.last_read_at, 0) \
                AND m.sender_vault <> ?1), \
           (SELECT m.text FROM chat_peer_messages m WHERE m.thread_id = t.id \
              ORDER BY m.sort_at DESC, m.id DESC LIMIT 1), \
           MAX(t.updated_at, COALESCE((SELECT MAX(m.sort_at) FROM chat_peer_messages m \
              WHERE m.thread_id = t.id), 0)) AS last_at \
         FROM chat_threads t \
         WHERE {state_clause} AND (?2 IS NULL OR t.kind = ?2) \
         ORDER BY last_at DESC, t.id"
    );
    let rows = q.query_map(&sql, params![own_vault, kind], row0)?;
    rows.into_iter().map(|row| resolve_peer(q, row)).collect()
}

fn row0(row: &Row<'_>) -> haex_crdt::rusqlite::Result<Row0> {
    let kind: String = row.get(1)?;
    let state: String = row.get(2)?;
    let last_text: Option<String> = row.get(7)?;
    Ok(Row0 {
        id: row.get(0)?,
        kind: ConversationKind::parse(&kind)?,
        state: ConversationState::parse(&state)?,
        title: row.get(3)?,
        peer_vault: row.get(4)?,
        muted: row.get::<_, i64>(5)? != 0,
        unread: row.get(6)?,
        last_text: last_text.map(|text| preview(&text)),
        last_at: row.get(8)?,
    })
}

fn resolve_peer(q: &mut impl Query, row: Row0) -> Result<Conversation> {
    let mut peer_contact_id = None;
    let mut peer_card_name = None;
    let mut peer_without_relays = false;
    if let Some(vault) = row.peer_vault.as_deref().and_then(vault_bytes) {
        if let Some((contact_id, card)) = received::stored(q, &vault)? {
            peer_contact_id = Some(contact_id);
            peer_card_name = Some(card.card.name);
            peer_without_relays = received::addresses(q, &vault)?.nostr_relays.is_empty();
        }
    }
    Ok(Conversation {
        id: row.id,
        kind: row.kind,
        state: row.state,
        title: row.title,
        peer_vault: row.peer_vault,
        peer_contact_id,
        peer_card_name,
        peer_without_relays,
        muted: row.muted,
        unread: row.unread,
        last_text: row.last_text,
        last_at: row.last_at,
    })
}

pub(crate) fn preview(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default();
    match line.char_indices().nth(PREVIEW_CHARS) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_owned(),
    }
}

fn vault_bytes(hex: &str) -> Option<[u8; 32]> {
    decode_hex(hex).ok()?.try_into().ok()
}

/// Opens the direct conversation of `own_vault` with `peer_vault` (both hex), creating the thread
/// and its two member rows when there is none yet (FR-005). The id is derived, so a conversation
/// opened on two own devices before they synced is one conversation.
pub fn open_direct(
    tx: &mut CrdtTransaction<'_>,
    own_vault: &str,
    peer_vault: &str,
    now: i64,
) -> Result<Uuid> {
    if own_vault == peer_vault {
        return Err(HolziError::InvalidInput {
            reason: "a direct conversation needs another vault".into(),
        });
    }
    let id = ids::direct_thread_id(own_vault, peer_vault);
    if kind_of(tx, id)?.is_some() {
        return Ok(id);
    }
    tx.execute(
        "INSERT INTO chat_threads \
           (id, title, created_at, updated_at, kind, state, peer_vault) \
         VALUES (?1, '', ?2, ?2, 'direct', 'active', ?3)",
        params![id.to_string(), now, peer_vault],
    )?;
    for vault in [own_vault, peer_vault] {
        insert_member(tx, id, vault, own_vault, now)?;
    }
    Ok(id)
}

fn insert_member(
    tx: &mut CrdtTransaction<'_>,
    thread_id: Uuid,
    vault: &str,
    by: &str,
    now: i64,
) -> Result<()> {
    let member_id = ids::member_id(thread_id, vault);
    let present: Option<i64> = tx.query_row(
        "SELECT 1 FROM chat_members WHERE id = ?1",
        params![member_id.to_string()],
        |row| row.get(0),
    )?;
    if present.is_none() {
        tx.execute(
            "INSERT INTO chat_members \
               (id, thread_id, vault, state, state_at, state_op, state_by, joined_at) \
             VALUES (?1, ?2, ?3, 'member', ?4, 'open', ?5, ?4)",
            params![member_id.to_string(), thread_id.to_string(), vault, now, by],
        )?;
    }
    Ok(())
}

/// The kind of a conversation, `None` when there is none with this id.
pub fn kind_of(q: &mut impl Query, id: Uuid) -> Result<Option<ConversationKind>> {
    let raw: Option<String> = q.query_row(
        "SELECT kind FROM chat_threads WHERE id = ?1",
        params![id.to_string()],
        |row| row.get(0),
    )?;
    match raw {
        None => Ok(None),
        Some(kind) => Ok(Some(
            ConversationKind::parse(&kind).map_err(haex_crdt::Error::from)?,
        )),
    }
}

/// Mutes or unmutes a conversation of any kind (FR-008); false when there is none.
pub fn set_muted(tx: &mut CrdtTransaction<'_>, id: Uuid, muted: bool) -> Result<bool> {
    let changed = tx.execute(
        "UPDATE chat_threads SET muted = ?1 WHERE id = ?2",
        params![i64::from(muted), id.to_string()],
    )?;
    Ok(changed == 1)
}

/// Deletes a conversation of any kind with everything that belongs to it, on every own device
/// through the sync (FR-008); false when there is none. Leaving a group first is the caller's job
/// (US4).
pub fn delete(tx: &mut CrdtTransaction<'_>, id: Uuid) -> Result<bool> {
    let Some(kind) = kind_of(tx, id)? else {
        return Ok(false);
    };
    if kind == ConversationKind::Agent {
        return Ok(agent_threads::delete_thread_and_messages(tx, id)?);
    }
    let thread = id.to_string();
    for sql in [
        "DELETE FROM chat_deliveries WHERE message_id IN \
           (SELECT id FROM chat_peer_messages WHERE thread_id = ?1)",
        "DELETE FROM chat_attachments WHERE message_id IN \
           (SELECT id FROM chat_peer_messages WHERE thread_id = ?1)",
        "DELETE FROM chat_peer_messages WHERE thread_id = ?1",
        "DELETE FROM chat_members WHERE thread_id = ?1",
        "DELETE FROM chat_threads WHERE id = ?1",
    ] {
        tx.execute(sql, params![thread])?;
    }
    Ok(true)
}

fn invalid_text(column: &str, value: &str) -> haex_crdt::rusqlite::Error {
    haex_crdt::rusqlite::Error::FromSqlConversionFailure(
        0,
        haex_crdt::rusqlite::types::Type::Text,
        format!("{column} holds the unknown value {value:?}").into(),
    )
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
