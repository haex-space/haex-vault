//! What this device still has to publish (spec 072 research R6, spec 095 data-model.md "Outbox"):
//! one row per event with the recipient vaults no Nostr-Relay accepted yet (`pending`), the vaults
//! whose delivery receipt has not come (`awaiting`), and per receiving device the finished wrap made
//! for it (`chat_outbox_sealed_no_sync`). A retry publishes that same wrap again: a second wrap
//! around the same ratchet message would reach the receiver with a used message key.
//!
//! An attempt that leaves recipients pending waits 5 s, 15 s, 1 min, 5 min, then every 15 min, with
//! no end (spec 095 T033). After 24 h a delivery whose wraps no Nostr-Relay accepted shows _failed_
//! (`noRelay`) and is still tried; an accepted wrap clears that. A vault that waits only for its
//! devices' keys is not failed. A vault whose card names no device, or an event too large, fails at
//! once and parks the row for "Erneut senden".
//!
//! A message row whose vaults were all accepted stays while a vault in `awaiting` has not sent its
//! delivery receipt; the receipt ends it ([`delivered`]). Until then it is encrypted again and
//! published again 1 day, 3 days and then every 7 days after its last attempt ([`start_resend`]).
//! Deleting the conversation ends it. `ponytail:` the outbox is device-local, so a message queued
//! on a device that never comes back stays _waiting_; upgrade: a synced claim lets another own
//! device take it over.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use nostr::event::Event;
use serde::{Deserialize, Serialize};

use super::ratchet::RESEND_AFTER_MS;
use super::session::Me;
use super::{ids, targets};
use crate::error::{HaexError, Result};
use crate::storage::query::Query;
use crate::sync::keys::hex;

/// The waits after the first attempts; every later one waits the last.
const BACKOFF_MS: [i64; 5] = [5_000, 15_000, 60_000, 300_000, 900_000];
/// How long a delivery may stay without any Nostr-Relay accepting it before it shows _failed_.
pub const GIVE_UP_MS: i64 = 24 * 60 * 60 * 1000;
/// `next_at` of a parked row: never due until "Erneut senden" or a reset.
const PARKED: i64 = i64::MAX;

/// Why a delivery failed (data-model.md `failed_reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailReason {
    /// No Nostr-Relay of the recipient (nor an own one) accepted it in time, or there is none; the
    /// row is still tried.
    NoRelay,
    /// The recipient's card names no device.
    NoDevices,
    /// The wrap would be over the size a Nostr-Relay accepts.
    TooLarge,
}

impl FailReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::NoRelay => "noRelay",
            Self::NoDevices => "noDevices",
            Self::TooLarge => "tooLarge",
        }
    }

    /// Whether the row parks for "Erneut senden": trying again changes nothing until the user acts.
    fn parks(self) -> bool {
        matches!(self, Self::NoDevices | Self::TooLarge)
    }
}

/// The rumor an outbox row wraps again on every attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rumor {
    pub kind: u16,
    pub content: String,
}

/// One row of the outbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub thread_id: String,
    /// The message the event carries; `None` for receipts.
    pub message_id: Option<String>,
    pub rumor: Rumor,
    /// Recipient vaults (hex) still to do; the own vault stands for the own other devices.
    pub pending: Vec<String>,
    /// Recipient vaults whose delivery receipt has not come; empty for receipts.
    pub awaiting: Vec<String>,
    pub attempts: i64,
    /// How often it was encrypted again for `awaiting` after a Nostr-Relay accepted it.
    pub resends: i64,
    pub created_at: i64,
}

impl Entry {
    /// A message whose vaults were all accepted and whose receipt is due: it is sent again.
    pub fn is_resend(&self) -> bool {
        self.message_id.is_some() && self.pending.is_empty() && !self.awaiting.is_empty()
    }
}

/// The wait after `attempts` attempts.
pub fn backoff(attempts: i64) -> i64 {
    let index = usize::try_from(attempts.saturating_sub(1)).unwrap_or(0);
    BACKOFF_MS[index.min(BACKOFF_MS.len() - 1)]
}

/// The wait before a message without its receipt is sent again, after `resends` resends.
pub fn resend_after(resends: i64) -> i64 {
    let index = usize::try_from(resends).unwrap_or(0);
    RESEND_AFTER_MS[index.min(RESEND_AFTER_MS.len() - 1)]
}

/// Queues an event for `pending`, due at once; `awaiting` are the vaults whose delivery receipt
/// ends the row.
#[allow(clippy::too_many_arguments)]
pub fn enqueue(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    thread_id: &str,
    message_id: Option<&str>,
    rumor: &Rumor,
    pending: &[String],
    awaiting: &[String],
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO chat_outbox_no_sync \
           (id, thread_id, message_id, rumor, pending, awaiting, attempts, next_at, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?7)",
        params![
            id,
            thread_id,
            message_id,
            to_json(rumor)?,
            to_json(&pending)?,
            to_json(&awaiting)?,
            now
        ],
    )?;
    Ok(())
}

/// The rows due at `now`, oldest first.
pub fn due(q: &mut impl Query, now: i64) -> Result<Vec<Entry>> {
    let rows = q.query_map(
        "SELECT id, thread_id, message_id, rumor, pending, awaiting, attempts, resends, \
           created_at \
         FROM chat_outbox_no_sync WHERE next_at <= ?1 ORDER BY created_at, id",
        params![now],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
            ))
        },
    )?;
    rows.into_iter()
        .map(
            |(
                id,
                thread_id,
                message_id,
                rumor,
                pending,
                awaiting,
                attempts,
                resends,
                created_at,
            )| {
                Ok(Entry {
                    id,
                    thread_id,
                    message_id,
                    rumor: from_json(&rumor)?,
                    pending: from_json(&pending)?,
                    awaiting: from_json(&awaiting)?,
                    attempts,
                    resends,
                    created_at,
                })
            },
        )
        .collect()
}

/// The wrap made for one receiving device of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sealed {
    pub wrap: Event,
    /// When a Nostr-Relay accepted it; `None` before.
    pub accepted_at: Option<i64>,
}

/// The wrap of row `outbox_id` for `device` (hex), if one was made.
pub fn sealed(q: &mut impl Query, outbox_id: &str, device: &str) -> Result<Option<Sealed>> {
    let row: Option<(String, Option<i64>)> = q.query_row(
        "SELECT wrap, accepted_at FROM chat_outbox_sealed_no_sync \
         WHERE outbox_id = ?1 AND recipient_device = ?2",
        params![outbox_id, device],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    row.map(|(wrap, accepted_at)| {
        Ok(Sealed {
            wrap: Event::from_json(&wrap).map_err(|e| HaexError::InvalidInput {
                reason: format!("outbox: a stored wrap does not read: {e}"),
            })?,
            accepted_at,
        })
    })
    .transpose()
}

/// Stores the wrap made for `device` (hex) of row `outbox_id`, in the transaction that advanced the
/// session for it.
pub fn store_sealed(
    tx: &mut CrdtTransaction<'_>,
    outbox_id: &str,
    device: &str,
    wrap: &Event,
) -> Result<()> {
    tx.execute(
        "INSERT INTO chat_outbox_sealed_no_sync (outbox_id, recipient_device, wrap) \
         VALUES (?1, ?2, ?3)",
        params![outbox_id, device, wrap.as_json()],
    )?;
    Ok(())
}

/// Notes that a Nostr-Relay accepted the wrap of row `outbox_id` for `device` (hex).
pub fn mark_sealed_accepted(
    tx: &mut CrdtTransaction<'_>,
    outbox_id: &str,
    device: &str,
    now: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE chat_outbox_sealed_no_sync SET accepted_at = ?3 \
         WHERE outbox_id = ?1 AND recipient_device = ?2 AND accepted_at IS NULL",
        params![outbox_id, device, now],
    )?;
    Ok(())
}

/// The delivery receipt of `vault` for the message `message_id` came: the vault leaves `awaiting`,
/// and the row goes once nothing is pending or awaited.
pub fn delivered(tx: &mut CrdtTransaction<'_>, message_id: &str, vault: &str) -> Result<()> {
    let row: Option<(String, String, String)> = tx.query_row(
        "SELECT id, pending, awaiting FROM chat_outbox_no_sync WHERE message_id = ?1",
        params![message_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let Some((id, pending, awaiting)) = row else {
        return Ok(());
    };
    let mut awaiting: Vec<String> = from_json(&awaiting)?;
    if !awaiting.iter().any(|awaited| awaited == vault) {
        return Ok(());
    }
    awaiting.retain(|awaited| awaited != vault);
    let pending: Vec<String> = from_json(&pending)?;
    if pending.is_empty() && awaiting.is_empty() {
        return remove(tx, &id);
    }
    tx.execute(
        "UPDATE chat_outbox_no_sync SET awaiting = ?2 WHERE id = ?1",
        params![id, to_json(&awaiting)?],
    )?;
    Ok(())
}

/// Deletes a row with its stored wraps.
fn remove(tx: &mut CrdtTransaction<'_>, id: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM chat_outbox_sealed_no_sync WHERE outbox_id = ?1",
        params![id],
    )?;
    tx.execute("DELETE FROM chat_outbox_no_sync WHERE id = ?1", params![id])?;
    Ok(())
}

/// A reset from `device` (hex) of `vault` was accepted (spec 095 T029): its wraps that no
/// Nostr-Relay took yet are dropped, since they are on sessions it no longer has, and every row
/// awaiting `vault` drops that device's wrap and has `vault` pending again. Those rows are due now.
pub fn reseal_for_device(
    tx: &mut CrdtTransaction<'_>,
    device: &str,
    vault: &str,
    now: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE chat_outbox_no_sync SET next_at = ?2 WHERE id IN (SELECT outbox_id \
           FROM chat_outbox_sealed_no_sync WHERE recipient_device = ?1 AND accepted_at IS NULL)",
        params![device, now],
    )?;
    tx.execute(
        "DELETE FROM chat_outbox_sealed_no_sync WHERE recipient_device = ?1 AND accepted_at IS NULL",
        params![device],
    )?;
    let rows = tx.query_map(
        "SELECT id, pending, awaiting FROM chat_outbox_no_sync",
        &[],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )?;
    for (id, pending, awaiting) in rows {
        let awaiting: Vec<String> = from_json(&awaiting)?;
        if !awaiting.iter().any(|awaited| awaited == vault) {
            continue;
        }
        let mut pending: Vec<String> = from_json(&pending)?;
        if !pending.iter().any(|waiting| waiting == vault) {
            pending.push(vault.to_owned());
        }
        tx.execute(
            "DELETE FROM chat_outbox_sealed_no_sync WHERE outbox_id = ?1 AND recipient_device = ?2",
            params![id, device],
        )?;
        tx.execute(
            "UPDATE chat_outbox_no_sync SET pending = ?2, next_at = ?3 WHERE id = ?1",
            params![id, to_json(&pending)?, now],
        )?;
    }
    Ok(())
}

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
    for (vault, reason) in &attempt.failed {
        if *reason == FailReason::NoRelay {
            fail_unsent(tx, entry, vault, now)?;
        } else {
            mark(tx, entry, vault, "failed_at", Some(*reason), now)?;
        }
    }
    if !attempt.accepted.is_empty() {
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
        let failed =
            entry.message_id.is_some() && attempt.failed.iter().any(|(_, reason)| reason.parks());
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

fn to_json<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| HaexError::InvalidInput {
        reason: format!("outbox: {e}"),
    })
}

fn from_json<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|e| HaexError::InvalidInput {
        reason: format!("outbox: {e}"),
    })
}

#[cfg(test)]
#[path = "outbox_tests.rs"]
mod tests;
