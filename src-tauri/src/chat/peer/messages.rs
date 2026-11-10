//! The messages of a contact conversation for the window (spec 072 US2, FR-021, FR-024,
//! contracts/ipc.md `chat_peer_messages_list`): ordered by `sort_at`, own ones with their delivery
//! state derived from the set-once columns (data-model.md "Derived state").

use haex_crdt::rusqlite::params;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use super::settings;
use super::store::{self, ConversationKind};
use crate::error::Result;
use crate::storage::query::Query;

/// Where an own message stands (data-model.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ChatDeliveryState"
)]
#[serde(rename_all = "camelCase")]
pub enum DeliveryState {
    Waiting,
    Sent,
    Delivered,
    Read,
    Failed,
}

/// One message of a contact conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ChatPeerMessage"
)]
#[serde(rename_all = "camelCase")]
pub struct PeerMessage {
    pub id: String,
    pub sender_vault: String,
    /// Written by this vault (on any own device).
    pub own: bool,
    pub text: Option<String>,
    #[ts(type = "number")]
    pub sent_at: i64,
    #[ts(type = "number")]
    pub sort_at: i64,
    /// Own messages only.
    pub delivery: Option<DeliveryState>,
    /// `noRelay`, `noDevices`, `tooLarge` when the delivery failed.
    pub failed_reason: Option<String>,
    /// The files of the message, in their order (US3).
    pub attachments: Vec<PeerAttachment>,
}

/// A file of a message and where its copy on this device stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ChatPeerAttachment"
)]
#[serde(rename_all = "camelCase")]
pub struct PeerAttachment {
    pub id: String,
    pub name: Option<String>,
    pub mime: Option<String>,
    #[ts(type = "number")]
    pub size: i64,
    /// On this device: `pending` (above the download limit), `downloading`, `stored`, `failed`,
    /// `gone` (no longer on the server); `None` until this device looked at it.
    pub state: Option<String>,
    #[ts(type = "number")]
    pub bytes_done: i64,
    /// The code of the last failure (`blossom:…`).
    pub error: Option<String>,
}

/// The columns of one delivery row.
#[derive(Debug, Clone, Copy, Default)]
struct Stamps {
    sent: bool,
    delivered: bool,
    read: bool,
    failed: bool,
}

impl Stamps {
    /// failed > read > delivered > sent > waiting; a later `sent_at` clears a failure.
    fn state(self, show_read: bool) -> DeliveryState {
        if self.failed && !self.sent {
            DeliveryState::Failed
        } else if self.read && show_read {
            DeliveryState::Read
        } else if self.delivered || self.read {
            DeliveryState::Delivered
        } else if self.sent {
            DeliveryState::Sent
        } else {
            DeliveryState::Waiting
        }
    }
}

/// How many messages one page holds when the window names no limit.
pub const PAGE: u32 = 200;

/// A page of messages: the newest `limit` before `sort_at` `before` (all when `None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    pub before: Option<i64>,
    pub limit: u32,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            before: None,
            limit: PAGE,
        }
    }
}

/// A page of the messages of `thread_id` for the vault `own` (hex), oldest first. The _read_ state
/// shows only while read receipts are on (FR-024, reciprocal).
pub fn list(
    q: &mut impl Query,
    own: &str,
    thread_id: Uuid,
    page: Page,
) -> Result<Vec<PeerMessage>> {
    let show_read = settings::read_receipts(q)?;
    // Notes are never delivered, so they have no delivery state at all (FR-002b).
    let note = store::kind_of(q, thread_id)? == Some(ConversationKind::Note);
    let rows = q.query_map(
        "SELECT m.id, m.sender_vault, m.text, m.sent_at, m.sort_at, \
           d.sent_at IS NOT NULL, d.delivered_at IS NOT NULL, d.read_at IS NOT NULL, \
           d.failed_at IS NOT NULL, d.failed_reason, d.id IS NOT NULL \
         FROM chat_peer_messages m \
         LEFT JOIN chat_deliveries d ON d.message_id = m.id \
         WHERE m.thread_id = ?1 AND (?2 IS NULL OR m.sort_at < ?2) \
         ORDER BY m.sort_at DESC, m.id DESC LIMIT ?3",
        params![thread_id.to_string(), page.before, page.limit],
        |row| {
            let sender_vault: String = row.get(1)?;
            let own_message = sender_vault == own;
            let stamps = Stamps {
                sent: row.get(5)?,
                delivered: row.get(6)?,
                read: row.get(7)?,
                failed: row.get(8)?,
            };
            let has_delivery: bool = row.get(10)?;
            let delivery = (own_message && has_delivery).then(|| stamps.state(show_read));
            Ok(PeerMessage {
                id: row.get(0)?,
                own: own_message,
                sender_vault,
                text: row.get(2)?,
                sent_at: row.get(3)?,
                sort_at: row.get(4)?,
                failed_reason: if delivery == Some(DeliveryState::Failed) {
                    row.get(9)?
                } else {
                    None
                },
                delivery: delivery.or((own_message && !note).then_some(DeliveryState::Waiting)),
                attachments: Vec::new(),
            })
        },
    )?;
    let mut rows = rows;
    rows.reverse();
    if !rows.is_empty() {
        let mut files = attachments(q, thread_id)?;
        for message in &mut rows {
            if let Some(found) = files.remove(&message.id) {
                message.attachments = found;
            }
        }
    }
    Ok(rows)
}

/// The attachments of a conversation's messages by message id, each in its order.
fn attachments(
    q: &mut impl Query,
    thread_id: Uuid,
) -> Result<std::collections::HashMap<String, Vec<PeerAttachment>>> {
    let rows = q.query_map(
        "SELECT a.message_id, a.id, a.name, a.mime, COALESCE(a.size, 0), f.state, \
           COALESCE(f.bytes_done, 0), f.error \
         FROM chat_attachments a \
         JOIN chat_peer_messages m ON m.id = a.message_id \
         LEFT JOIN chat_attachment_files_no_sync f ON f.attachment_id = a.id \
         WHERE m.thread_id = ?1",
        params![thread_id.to_string()],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                PeerAttachment {
                    id: row.get(1)?,
                    name: row.get(2)?,
                    mime: row.get(3)?,
                    size: row.get(4)?,
                    state: row.get(5)?,
                    bytes_done: row.get(6)?,
                    error: row.get(7)?,
                },
            ))
        },
    )?;
    let mut by_message: std::collections::HashMap<String, Vec<PeerAttachment>> =
        std::collections::HashMap::new();
    for (message_id, attachment) in rows {
        by_message.entry(message_id).or_default().push(attachment);
    }
    // The ids carry the index only inside their hash, so the order is the one of the message.
    for (message_id, found) in &mut by_message {
        let Ok(message) = Uuid::parse_str(message_id) else {
            continue;
        };
        let index_of = |id: &str| {
            (0..super::envelope::MAX_ATTACHMENTS)
                .find(|index| super::ids::attachment_id(message, *index).to_string() == id)
                .unwrap_or(usize::MAX)
        };
        found.sort_by_key(|attachment| index_of(&attachment.id));
    }
    Ok(by_message)
}

/// The newest `sort_at` of a conversation, 0 without messages.
pub fn newest(q: &mut impl Query, thread_id: Uuid) -> Result<i64> {
    Ok(q.query_row(
        "SELECT COALESCE(MAX(sort_at), 0) FROM chat_peer_messages WHERE thread_id = ?1",
        params![thread_id.to_string()],
        |row| row.get::<_, i64>(0),
    )?
    .unwrap_or(0))
}
