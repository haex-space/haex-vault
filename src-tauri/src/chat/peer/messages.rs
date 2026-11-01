//! The messages of a contact conversation for the window (spec 072 US2, FR-021, FR-024,
//! contracts/ipc.md `chat_peer_messages_list`): ordered by `sort_at`, own ones with their delivery
//! state derived from the set-once columns (data-model.md "Derived state").

use haex_crdt::rusqlite::params;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use super::settings;
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
                delivery: delivery.or(own_message.then_some(DeliveryState::Waiting)),
            })
        },
    )?;
    let mut rows = rows;
    rows.reverse();
    Ok(rows)
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
