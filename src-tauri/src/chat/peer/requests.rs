//! Answers to a request (spec 072 US5, FR-041, FR-042, research R10). Accepting is reading the
//! carried card in through spec 060's preview: storing it ends the request
//! ([`store::accept_requests_of`]), so there is one import path. Declining removes the
//! conversation and tells the sender nothing; a later message is a new request. Blocking puts the
//! vault on spec 051's block list and removes the conversation.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::Deserialize;
use uuid::Uuid;

use super::store;
use crate::contacts::card::received;
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

/// What the user does with a request (contracts/ipc.md `chat_request_answer`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Answer {
    Decline,
    Block,
}

/// The other vault (hex) of the direct request `thread_id`.
fn requester(q: &mut impl Query, thread_id: Uuid) -> Result<String> {
    let peer: Option<Option<String>> = q.query_row(
        "SELECT peer_vault FROM chat_threads \
         WHERE id = ?1 AND kind = 'direct' AND state = 'request'",
        params![thread_id.to_string()],
        |row| row.get(0),
    )?;
    peer.flatten().ok_or_else(|| HolziError::NotFound {
        name: "request".into(),
    })
}

/// The text of the card the sender of the request carried, for spec 060's preview.
pub fn card_text(q: &mut impl Query, thread_id: Uuid) -> Result<String> {
    store::request_card(q, thread_id)?
        .map(|card| card.text())
        .ok_or_else(|| HolziError::NotFound {
            name: "request".into(),
        })
}

/// Declines or blocks the request `thread_id`; its messages and members go with it.
pub fn answer(tx: &mut CrdtTransaction<'_>, thread_id: Uuid, answer: Answer) -> Result<()> {
    let peer = requester(tx, thread_id)?;
    if answer == Answer::Block {
        received::block(tx, &peer)?;
    }
    store::delete(tx, thread_id)?;
    Ok(())
}

#[cfg(test)]
#[path = "requests_tests.rs"]
mod tests;
