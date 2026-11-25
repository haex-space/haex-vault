//! What happens to an outbox row after an attempt (spec 072 research R6, spec 095): delivery
//! states, parking and the explicit resends, also by vault for the answers of spec 099 (research
//! R6). Split from [`super`] at this state-machine change, as its header asked.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use super::{
    backoff, from_json, remove, resend_after, to_json, Entry, FailReason, GIVE_UP_MS, PARKED,
};
use crate::chat::peer::session::Me;
use crate::chat::peer::{ids, targets};
use crate::error::Result;
use crate::storage::query::Query;
use crate::sync::keys::hex;

/// Prepares sending `entry` again for the vaults still awaiting its receipt (spec 095 T031): a
/// vault whose delivery row has `delivered_at` leaves `awaiting` (the receipt may have come through
/// the sync between own devices); the rest drop their devices' wraps and are pending again, and
/// `resends` counts. Answers the entry to attempt, or `None` when the row ended: every receipt
/// came, or the message is gone (its conversation was deleted, here or on another own device).
pub fn start_resend(tx: &mut CrdtTransaction<'_>, me: &Me, entry: &Entry) -> Result<Option<Entry>> {
    let Some(message_id) = entry.message_id.as_deref() else {
        return Ok(None);
    };
    let row: Option<String> = tx.query_row(
        "SELECT awaiting FROM chat_outbox_no_sync WHERE id = ?1",
        params![entry.id],
        |row| row.get(0),
    )?;
    let Some(awaiting) = row else {
        return Ok(None);
    };
    let present: Option<i64> = tx.query_row(
        "SELECT 1 FROM chat_peer_messages WHERE id = ?1",
        params![message_id],
        |row| row.get(0),
    )?;
    let Ok(message) = message_id.parse() else {
        return Ok(None);
    };
    let mut awaiting: Vec<String> = from_json(&awaiting)?;
    let mut still = Vec::new();
    for vault in awaiting.drain(..) {
        let delivered: Option<Option<i64>> = tx.query_row(
            "SELECT delivered_at FROM chat_deliveries WHERE id = ?1",
            params![ids::delivery_id(message, &vault).to_string()],
            |row| row.get(0),
        )?;
        if delivered.flatten().is_none() {
            still.push(vault);
        }
    }
    if present.is_none() || still.is_empty() {
        remove(tx, &entry.id)?;
        return Ok(None);
    }
    for vault in &still {
        for device in targets::of(tx, me, vault)?.devices {
            tx.execute(
                "DELETE FROM chat_outbox_sealed_no_sync \
                 WHERE outbox_id = ?1 AND recipient_device = ?2",
                params![entry.id, hex(&device)],
            )?;
        }
    }
    let resends = entry.resends + 1;
    tx.execute(
        "UPDATE chat_outbox_no_sync SET pending = ?2, awaiting = ?2, attempts = 0, resends = ?3 \
         WHERE id = ?1",
        params![entry.id, to_json(&still)?, resends],
    )?;
    Ok(Some(Entry {
        pending: still.clone(),
        awaiting: still,
        attempts: 0,
        resends,
        ..entry.clone()
    }))
}

/// What one attempt at `entry` did: the vaults a Nostr-Relay accepted, those that failed, those
/// with stored wraps of which none was accepted, and the error of the rest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attempt {
    pub accepted: Vec<String>,
    /// Vaults with a partially accepted delivery that must stay pending.
    pub partial_accepted: Vec<String>,
    pub failed: Vec<(String, FailReason)>,
    pub unaccepted: Vec<String>,
    pub error: Option<String>,
}

/// Records an attempt: accepted vaults get `sent_at` and lose a failure, failed ones `failed_at`.
/// `noDevices` and `tooLarge` take the vault out of `pending`; the rest waits for the next
/// attempt, with no end. Once [`GIVE_UP_MS`] passed, a vault with stored wraps of which none was
/// accepted shows `noRelay` (never one that was sent already). The first accepted vault sets the
/// row's `accepted_at`. A row with nothing pending ends when no vault is awaited and no delivery
/// of its message failed; one with a failed delivery parks for "Erneut senden"; one awaiting a
/// receipt is due again after [`resend_after`].
pub fn record(
    tx: &mut CrdtTransaction<'_>,
    entry: &Entry,
    attempt: &Attempt,
    now: i64,
) -> Result<()> {
    // A receipt may have come since `entry` was read.
    let awaiting: Option<String> = tx.query_row(
        "SELECT awaiting FROM chat_outbox_no_sync WHERE id = ?1",
        params![entry.id],
        |row| row.get(0),
    )?;
    let Some(awaiting) = awaiting else {
        return Ok(());
    };
    let awaiting: Vec<String> = from_json(&awaiting)?;
    for vault in &attempt.accepted {
        mark(tx, entry, vault, "sent_at", None, now)?;
        clear_failure(tx, entry, vault)?;
    }
    for vault in &attempt.partial_accepted {
        clear_failure(tx, entry, vault)?;
    }
    for (vault, reason) in &attempt.failed {
        if *reason == FailReason::NoRelay {
            fail_unsent(tx, entry, vault, now)?;
        } else {
            mark(tx, entry, vault, "failed_at", Some(*reason), now)?;
        }
    }
    if !attempt.accepted.is_empty() || !attempt.partial_accepted.is_empty() {
        tx.execute(
            "UPDATE chat_outbox_no_sync SET accepted_at = ?2 WHERE id = ?1 AND accepted_at IS NULL",
            params![entry.id, now],
        )?;
    }
    let pending: Vec<String> = entry
        .pending
        .iter()
        .filter(|vault| {
            !attempt.accepted.contains(vault)
                && !attempt
                    .failed
                    .iter()
                    .any(|(failed, reason)| failed == *vault && reason.parks())
        })
        .cloned()
        .collect();
    let attempts = entry.attempts + 1;
    if pending.is_empty() {
        // A message with a failed delivery stays parked so "Erneut senden" finds its rumor.
        let retained_parking_failure = if let Some(message_id) = entry.message_id.as_deref() {
            tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM chat_deliveries WHERE message_id = ?1 \
                   AND failed_at IS NOT NULL AND failed_reason IN ('noDevices', 'tooLarge'))",
                params![message_id],
                |row| row.get::<_, bool>(0),
            )?
            .unwrap_or(false)
        } else {
            false
        };
        let failed =
            retained_parking_failure || attempt.failed.iter().any(|(_, reason)| reason.parks());
        if awaiting.is_empty() && !failed {
            remove(tx, &entry.id)?;
            return Ok(());
        }
        let next_at = if failed {
            PARKED
        } else {
            now + resend_after(entry.resends)
        };
        tx.execute(
            "UPDATE chat_outbox_no_sync SET pending = '[]', attempts = ?2, next_at = ?3 \
             WHERE id = ?1",
            params![entry.id, attempts, next_at],
        )?;
        return Ok(());
    }
    if now - entry.created_at >= GIVE_UP_MS {
        for vault in attempt.unaccepted.iter().filter(|v| pending.contains(v)) {
            fail_unsent(tx, entry, vault, now)?;
        }
    }
    tx.execute(
        "UPDATE chat_outbox_no_sync SET pending = ?2, attempts = ?3, next_at = ?4, last_error = ?5 \
         WHERE id = ?1",
        params![
            entry.id,
            to_json(&pending)?,
            attempts,
            now + backoff(attempts),
            attempt.error
        ],
    )?;
    Ok(())
}

/// Marks the delivery of `vault` failed with `noRelay`, once, unless it was sent already.
fn fail_unsent(tx: &mut CrdtTransaction<'_>, entry: &Entry, vault: &str, now: i64) -> Result<()> {
    let Some(id) = delivery_of(entry, vault) else {
        return Ok(());
    };
    tx.execute(
        "UPDATE chat_deliveries SET failed_at = ?2, failed_reason = ?3 \
         WHERE id = ?1 AND failed_at IS NULL AND sent_at IS NULL",
        params![id, now, FailReason::NoRelay.code()],
    )?;
    Ok(())
}

/// A Nostr-Relay accepted for `vault`: a failure shown for its delivery is gone.
fn clear_failure(tx: &mut CrdtTransaction<'_>, entry: &Entry, vault: &str) -> Result<()> {
    let Some(id) = delivery_of(entry, vault) else {
        return Ok(());
    };
    tx.execute(
        "UPDATE chat_deliveries SET failed_at = NULL, failed_reason = NULL \
         WHERE id = ?1 AND failed_at IS NOT NULL",
        params![id],
    )?;
    Ok(())
}

/// The delivery row id of `vault` for the message of `entry`; `None` for a receipt.
fn delivery_of(entry: &Entry, vault: &str) -> Option<String> {
    let message = entry.message_id.as_deref()?.parse().ok()?;
    Some(ids::delivery_id(message, vault).to_string())
}

/// Sets a delivery state of `vault` for the message of `entry` once (a set column stays).
fn mark(
    tx: &mut CrdtTransaction<'_>,
    entry: &Entry,
    vault: &str,
    column: &str,
    reason: Option<FailReason>,
    now: i64,
) -> Result<()> {
    let Some(id) = delivery_of(entry, vault) else {
        return Ok(());
    };
    tx.execute(
        &format!("UPDATE chat_deliveries SET {column} = ?2 WHERE id = ?1 AND {column} IS NULL"),
        params![id, now],
    )?;
    if let Some(reason) = reason {
        tx.execute(
            "UPDATE chat_deliveries SET failed_reason = ?2 WHERE id = ?1",
            params![id, reason.code()],
        )?;
    }
    Ok(())
}

/// "Erneut senden" (US2 scenario 4): clears the failure of the message's deliveries and makes its
/// row due at once with a fresh 24 h; the failed vaults are pending again. `false` when this device
/// has no row of the message (it was sent from another own device).
pub fn resend(tx: &mut CrdtTransaction<'_>, message_id: &str, now: i64) -> Result<bool> {
    let row: Option<(String, String)> = tx.query_row(
        "SELECT id, pending FROM chat_outbox_no_sync WHERE message_id = ?1",
        params![message_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let Some((id, pending)) = row else {
        return Ok(false);
    };
    let mut pending: Vec<String> = from_json(&pending)?;
    let failed = tx.query_map(
        "SELECT recipient_vault FROM chat_deliveries \
         WHERE message_id = ?1 AND failed_at IS NOT NULL AND sent_at IS NULL",
        params![message_id],
        |row| row.get::<_, String>(0),
    )?;
    for vault in failed {
        if !pending.contains(&vault) {
            pending.push(vault);
        }
    }
    tx.execute(
        "UPDATE chat_deliveries SET failed_at = NULL, failed_reason = NULL \
         WHERE message_id = ?1 AND failed_at IS NOT NULL",
        params![message_id],
    )?;
    tx.execute(
        "UPDATE chat_outbox_no_sync SET pending = ?2, attempts = 0, next_at = ?3, created_at = ?3, \
           last_error = NULL WHERE id = ?1",
        params![id, to_json(&pending)?, now],
    )?;
    Ok(true)
}

/// Makes every row with pending vaults due at once (not the parked ones): after the vault opened
/// or the connections came back, the back-off of attempts that failed while offline does not
/// apply. A row that only awaits a receipt keeps its resend time.
pub fn due_now(tx: &mut CrdtTransaction<'_>, now: i64) -> Result<()> {
    tx.execute(
        "UPDATE chat_outbox_no_sync SET next_at = ?1 \
         WHERE next_at > ?1 AND next_at < ?2 AND pending <> '[]'",
        params![now, PARKED],
    )?;
    Ok(())
}

/// When the next row is due, for the service's timer.
pub fn next_due(q: &mut impl Query) -> Result<Option<i64>> {
    Ok(q.query_row(
        "SELECT MIN(next_at) FROM chat_outbox_no_sync WHERE next_at < ?1",
        params![PARKED],
        |row| row.get::<_, Option<i64>>(0),
    )?
    .flatten())
}

/// A row that awaits a vault: its id, message id and pending vaults.
type AwaitingRow = (String, Option<String>, Vec<String>);

/// The rows that await `vault`.
fn awaiting_rows(tx: &mut CrdtTransaction<'_>, vault: &str) -> Result<Vec<AwaitingRow>> {
    let rows = tx.query_map(
        "SELECT id, message_id, pending, awaiting FROM chat_outbox_no_sync",
        &[],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    let mut found = Vec::new();
    for (id, message_id, pending, awaiting) in rows {
        let awaiting: Vec<String> = from_json(&awaiting)?;
        if awaiting.iter().any(|awaited| awaited == vault) {
            found.push((id, message_id, from_json(&pending)?));
        }
    }
    Ok(found)
}

/// `vault` answered that it takes no messages from this vault (spec 099 FR-010): every row awaiting
/// it stops sending to it, its undelivered deliveries fail with `blocked`, and a row with nothing
/// else pending parks. The vault stays awaited, so [`resume_vault`] finds the rows again.
pub fn park_vault(tx: &mut CrdtTransaction<'_>, vault: &str, now: i64) -> Result<()> {
    for (id, message_id, mut pending) in awaiting_rows(tx, vault)? {
        park_row(tx, &id, message_id.as_deref(), &mut pending, vault, now)?;
    }
    Ok(())
}

/// Parks one outbox row for a blocked vault. Used when a new message is queued after a block, so
/// sending each message does not rescan every older row in the outbox.
pub fn park_one(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    message_id: &str,
    vault: &str,
    now: i64,
) -> Result<()> {
    let pending: Option<String> = tx.query_row(
        "SELECT pending FROM chat_outbox_no_sync WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    let Some(pending) = pending else {
        return Ok(());
    };
    let mut pending: Vec<String> = from_json(&pending)?;
    park_row(tx, id, Some(message_id), &mut pending, vault, now)
}

fn park_row(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    message_id: Option<&str>,
    pending: &mut Vec<String>,
    vault: &str,
    now: i64,
) -> Result<()> {
    pending.retain(|waiting| waiting != vault);
    let next_at: Option<i64> = pending.is_empty().then_some(PARKED);
    tx.execute(
        "UPDATE chat_outbox_no_sync SET pending = ?2, next_at = COALESCE(?3, next_at) \
         WHERE id = ?1",
        params![id, to_json(pending)?, next_at],
    )?;
    if let Some(message) = message_id.and_then(|id| id.parse().ok()) {
        tx.execute(
            "UPDATE chat_deliveries SET failed_at = COALESCE(failed_at, ?2), failed_reason = ?3 \
             WHERE id = ?1 AND delivered_at IS NULL",
            params![
                ids::delivery_id(message, vault).to_string(),
                now,
                FailReason::Blocked.code()
            ],
        )?;
    }
    Ok(())
}

/// `vault` answered that this vault may write (spec 099 FR-008): every row awaiting it is due at
/// once, and the `blocked` failures of its deliveries are gone. The row goes out as a resend
/// ([`start_resend`]), encrypted again: the vault may have read the wraps it was sent already (a
/// knock), so publishing those once more would reach it with used message keys.
pub fn resume_vault(tx: &mut CrdtTransaction<'_>, vault: &str, now: i64) -> Result<()> {
    for (id, message_id, _) in awaiting_rows(tx, vault)? {
        tx.execute(
            "UPDATE chat_outbox_no_sync SET attempts = 0, next_at = ?2, created_at = ?2, \
               last_error = NULL WHERE id = ?1",
            params![id, now],
        )?;
        if let Some(message) = message_id.and_then(|id| id.parse().ok()) {
            tx.execute(
                "UPDATE chat_deliveries SET failed_at = NULL, failed_reason = NULL \
                 WHERE id = ?1 AND failed_reason = ?2",
                params![
                    ids::delivery_id(message, vault).to_string(),
                    FailReason::Blocked.code()
                ],
            )?;
        }
    }
    Ok(())
}
