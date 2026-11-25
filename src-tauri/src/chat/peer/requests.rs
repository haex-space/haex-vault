//! Answers to a request (spec 099 FR-006 to FR-009, research R14). Accepting is reading the carried
//! card in through spec 060's preview: storing it ends the request ([`store::accept_requests_of`])
//! and notes the answer "accepted", so there is one import path. Declining puts the vault on spec
//! 051's block list with the card it carried, removes the conversation and notes the answer
//! "blocked" for the sender's devices.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::Deserialize;
use uuid::Uuid;

use super::{envelope, gate, store};
use crate::contacts::card::received;
use crate::error::{HaexError, Result};
use crate::storage::query::Query;

/// What the user does with a request (contracts/ipc.md `chat_request_answer`); accepting runs
/// through the address book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Answer {
    Decline,
}

/// The other vault (hex) of the direct request `thread_id`.
fn requester(q: &mut impl Query, thread_id: Uuid) -> Result<String> {
    let peer: Option<Option<String>> = q.query_row(
        "SELECT peer_vault FROM chat_threads \
         WHERE id = ?1 AND kind = 'direct' AND state = 'request'",
        params![thread_id.to_string()],
        |row| row.get(0),
    )?;
    peer.flatten().ok_or_else(|| HaexError::NotFound {
        name: "request".into(),
    })
}

/// The text of the card the sender of the request carried, for spec 060's preview.
pub fn card_text(q: &mut impl Query, thread_id: Uuid) -> Result<String> {
    store::request_card(q, thread_id)?
        .map(|card| card.text())
        .ok_or_else(|| HaexError::NotFound {
            name: "request".into(),
        })
}

/// Declines the request `thread_id` (FR-009): the vault is blocked with the card it carried, the
/// conversation goes with its members, and the answer "blocked" waits for the sender's devices.
pub fn answer(
    tx: &mut CrdtTransaction<'_>,
    thread_id: Uuid,
    answer: Answer,
    now: i64,
) -> Result<()> {
    let Answer::Decline = answer;
    let peer = requester(tx, thread_id)?;
    let card = store::request_card(tx, thread_id)?;
    received::block_with_card(tx, &peer, card.as_ref())?;
    store::delete(tx, thread_id)?;
    gate::write_notice(tx, &peer, envelope::Answer::Blocked, now)?;
    Ok(())
}

#[cfg(test)]
#[path = "requests_tests.rs"]
mod tests;
