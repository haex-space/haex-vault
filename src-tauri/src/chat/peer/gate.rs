//! The answers a vault gives to a sender (spec 099 research R5, R6): "accepted" when a request ends
//! because its vault became a contact or a vault is unblocked, "blocked" when a request is declined
//! or a contact blocked.
//!
//! The answer is noted in `chat_gate_notices_no_sync` in the transaction that gives it, which may run
//! without the chat session (the address book stores a card); this device's session turns the note
//! into a receipt with `answer` on the outbox before it publishes ([`send_due_notices`]). The note is
//! device-local, so only the device that gave the answer sends it. A later answer to the same vault
//! replaces an earlier one that was not sent yet.
//!
//! On the sender's side a received answer parks or resumes what waits for that vault
//! ([`apply_received`]).

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use super::envelope::{Answer, Receipt, To, RECEIPT_KIND};
use super::outbox::{self, Rumor};
use super::session::Me;
use super::{ids, send};
use crate::error::Result;
use crate::storage::query::Query;

/// Notes `answer` for `peer_vault` (hex), to be sent by this device's chat session.
pub fn write_notice(
    tx: &mut CrdtTransaction<'_>,
    peer_vault: &str,
    answer: Answer,
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO chat_gate_answer_sequences_no_sync (peer_vault, sequence) VALUES (?1, 1) \
         ON CONFLICT(peer_vault) DO UPDATE SET sequence = sequence + 1",
        params![peer_vault],
    )?;
    let sequence: i64 = tx.query_row(
        "SELECT sequence FROM chat_gate_answer_sequences_no_sync WHERE peer_vault = ?1",
        params![peer_vault],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO chat_gate_notices_no_sync (peer_vault, answer, created_at, sequence) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(peer_vault) DO UPDATE SET answer = excluded.answer, \
           created_at = excluded.created_at, sequence = excluded.sequence",
        params![peer_vault, answer.as_str(), now, sequence],
    )?;
    Ok(())
}

/// The noted answers, oldest first.
pub fn notices(q: &mut impl Query) -> Result<Vec<(String, Answer, i64)>> {
    let rows: Vec<(String, String, i64)> = q.query_map(
        "SELECT peer_vault, answer, sequence FROM chat_gate_notices_no_sync ORDER BY created_at, peer_vault",
        &[],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    Ok(rows
        .into_iter()
        .filter_map(|(vault, answer, sequence)| {
            Answer::parse(&answer).map(|answer| (vault, answer, sequence))
        })
        .collect())
}

/// Turns every noted answer into a receipt on the outbox and removes the note. Answers how many.
/// An earlier answer to the same vault that no Nostr-Relay took yet goes first, so it can never
/// reach the vault after the newer one (contracts/nostr-chat-gate.md).
pub fn send_due_notices(tx: &mut CrdtTransaction<'_>, me: &Me, now: i64) -> Result<usize> {
    let due = notices(tx)?;
    for (vault, answer, sequence) in &due {
        drop_unsent_notices(tx, vault)?;
        send::notice(tx, me, vault, *answer, *sequence, now)?;
    }
    // Every note is on the outbox now; one whose answer did not parse is of no use to anyone.
    tx.execute("DELETE FROM chat_gate_notices_no_sync", &[])?;
    Ok(due.len())
}

/// Removes the answers to `vault` (hex) on the outbox that no Nostr-Relay took yet.
fn drop_unsent_notices(tx: &mut CrdtTransaction<'_>, vault: &str) -> Result<()> {
    let rows: Vec<(String, String, String)> = tx.query_map(
        "SELECT id, rumor, pending FROM chat_outbox_no_sync WHERE message_id IS NULL",
        &[],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    for (id, rumor, pending) in rows {
        let waits = serde_json::from_str::<Vec<String>>(&pending)
            .is_ok_and(|pending| pending.iter().any(|waiting| waiting == vault));
        if waits && serde_json::from_str::<Rumor>(&rumor).is_ok_and(|r| is_notice(&r)) {
            outbox::remove(tx, &id)?;
        }
    }
    Ok(())
}

/// Whether `rumor` is a receipt carrying an answer.
fn is_notice(rumor: &Rumor) -> bool {
    rumor.kind == RECEIPT_KIND.as_u16()
        && serde_json::from_str::<Receipt>(&rumor.content)
            .is_ok_and(|receipt| receipt.answer.is_some())
}

/// The vaults (hex) a blocked answer is still to be sent to: noted, or on the outbox and not yet
/// taken by a Nostr-Relay. Their devices stay reachable for that one rumor (research R5).
pub fn blocked_notice_vaults(q: &mut impl Query) -> Result<Vec<String>> {
    let mut vaults: Vec<String> = q.query_map(
        "SELECT peer_vault FROM chat_gate_notices_no_sync WHERE answer = ?1",
        params![Answer::Blocked.as_str()],
        |row| row.get(0),
    )?;
    let rows: Vec<(String, String)> = q.query_map(
        "SELECT rumor, pending FROM chat_outbox_no_sync WHERE message_id IS NULL AND pending <> '[]'",
        &[],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    for (rumor, pending) in rows {
        let Ok(rumor) = serde_json::from_str::<Rumor>(&rumor) else {
            continue;
        };
        if !is_blocked_notice(&rumor) {
            continue;
        }
        for vault in serde_json::from_str::<Vec<String>>(&pending).unwrap_or_default() {
            if !vaults.contains(&vault) {
                vaults.push(vault);
            }
        }
    }
    Ok(vaults)
}

/// Whether `rumor` is a blocked answer: the one rumor addressed to a blocked vault.
pub fn is_blocked_notice(rumor: &Rumor) -> bool {
    rumor.kind == RECEIPT_KIND.as_u16()
        && serde_json::from_str::<Receipt>(&rumor.content)
            .is_ok_and(|receipt| receipt.answer == Some(Answer::Blocked))
}

/// Applies an answer `receipt.from` gave to this vault (`own`, hex): "blocked" notes it on the
/// direct conversation and parks what waits for that vault (FR-010); "accepted" clears the note and
/// makes it due at once (FR-008). A receipt without answer, or not for a direct conversation with
/// this vault, changes nothing.
pub fn apply_received(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    receipt: &Receipt,
    sender_device: &str,
    now: i64,
) -> Result<()> {
    let own = &me.vault_hex;
    let Some(answer) = receipt.answer else {
        return Ok(());
    };
    if receipt.thread != To::Direct(own.to_owned()) {
        return Ok(());
    }
    let sequence = receipt.answer_sequence.unwrap_or(0);
    let last: Option<i64> = tx.query_row(
        "SELECT sequence FROM chat_gate_received_answers_no_sync \
         WHERE peer_vault = ?1 AND sender_device = ?2",
        params![receipt.from, sender_device],
        |row| row.get(0),
    )?;
    if last.is_some_and(|last| sequence <= last) {
        return Ok(());
    }
    let thread_id = ids::direct_thread_id(own, &receipt.from).to_string();
    match answer {
        Answer::Blocked => {
            tx.execute(
                "UPDATE chat_threads SET peer_blocked_at = ?2 \
                 WHERE id = ?1 AND peer_blocked_at IS NULL",
                params![thread_id, now],
            )?;
            outbox::park_vault(tx, &receipt.from, now)?;
        }
        Answer::Accepted => {
            tx.execute(
                "UPDATE chat_threads SET peer_blocked_at = NULL \
                 WHERE id = ?1 AND peer_blocked_at IS NOT NULL",
                params![thread_id],
            )?;
            outbox::resume_vault(tx, me, &receipt.from, now)?;
        }
    }
    tx.execute(
        "INSERT INTO chat_gate_received_answers_no_sync (peer_vault, sender_device, sequence) \
         VALUES (?1, ?2, ?3) ON CONFLICT(peer_vault, sender_device) \
         DO UPDATE SET sequence = excluded.sequence",
        params![receipt.from, sender_device, sequence],
    )?;
    Ok(())
}

/// Whether the other vault of the direct conversation with `peer` blocked this vault.
pub fn blocked_by_peer(q: &mut impl Query, own: &str, peer: &str) -> Result<bool> {
    let at: Option<Option<i64>> = q.query_row(
        "SELECT peer_blocked_at FROM chat_threads WHERE id = ?1",
        params![ids::direct_thread_id(own, peer).to_string()],
        |row| row.get(0),
    )?;
    Ok(at.flatten().is_some())
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
