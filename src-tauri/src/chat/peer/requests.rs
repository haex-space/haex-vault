//! Answers to a request (spec 099 FR-006 to FR-009, research R14). Accepting is reading the carried
//! card in through spec 060's preview: storing it ends the request ([`store::accept_requests_of`])
//! and notes the answer "accepted", so there is one import path. Declining puts the vault on spec
//! 051's block list with the card it carried, removes the conversation and notes the answer
//! "blocked" for the sender's devices. Switching requests of strangers off removes the open ones
//! on every own device, whether the switch was flipped here or arrived through the sync
//! ([`follow_switch`]).

use std::sync::Arc;
use std::time::Duration;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::Deserialize;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{envelope, gate, settings, store};
use crate::contacts::card::received;
use crate::error::{HaexError, Result};
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

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

/// While requests of strangers are off (spec 099 FR-012), removes every request on this device,
/// without notice and without block. Answers how many went.
pub fn remove_when_off(tx: &mut CrdtTransaction<'_>) -> Result<usize> {
    if settings::allow_strangers(tx)? {
        return Ok(0);
    }
    tx.execute(
        "UPDATE chat_members SET card = NULL WHERE card IS NOT NULL AND thread_id IN (\
           SELECT id FROM chat_threads WHERE kind = 'direct' AND state = 'request'\
         )",
        params![],
    )?;
    // An empty direct thread stays hidden. Keeping its ID and members lets accepted history
    // sync in later without an intervening tombstone for the conversation.
    Ok(tx.execute(
        "UPDATE chat_threads SET state = CASE WHEN EXISTS (\
           SELECT 1 FROM chat_peer_messages m WHERE m.thread_id = chat_threads.id\
         ) THEN 'active' ELSE 'closed' END, request_first_at = NULL, request_last_at = NULL \
         WHERE kind = 'direct' AND state = 'request'",
        params![],
    )?)
}

/// Runs [`remove_when_off`] at session start and on changes to preferences or chat threads.
/// A request from another own device may sync after that device's switch changed.
pub(crate) async fn follow_switch(
    db: VaultDb,
    mut changes: broadcast::Receiver<Arc<Vec<String>>>,
    token: CancellationToken,
) {
    let initial_retry_delay = Duration::from_secs(1);
    let mut retry_delay = initial_retry_delay;
    loop {
        let removed = match db.read(|r| Ok(settings::allow_strangers(r)?)).await {
            Ok(true) => Ok(0),
            Ok(false) => db.write(|tx| Ok(remove_when_off(tx)?)).await,
            Err(error) => Err(error),
        };
        match removed {
            Ok(0) => {}
            Ok(removed) => log::info!("chat: requests are off, removed {removed}"),
            Err(error) => {
                log::warn!("chat: removing the requests failed: {error}");
                tokio::select! {
                    () = token.cancelled() => return,
                    () = tokio::time::sleep(retry_delay) => {}
                }
                retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
                continue;
            }
        }
        retry_delay = initial_retry_delay;
        loop {
            let received = tokio::select! {
                () = token.cancelled() => return,
                received = changes.recv() => received,
            };
            match received {
                Ok(tables)
                    if tables
                        .iter()
                        .any(|table| matches!(table.as_str(), "preferences" | "chat_threads")) =>
                {
                    break;
                }
                Ok(_) => {}
                // Fell behind: the preferences may have changed.
                Err(RecvError::Lagged(_)) => break,
                Err(RecvError::Closed) => return,
            }
        }
    }
}

#[cfg(test)]
#[path = "requests_tests.rs"]
mod tests;
