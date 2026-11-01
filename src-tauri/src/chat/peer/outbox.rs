//! What this device still has to publish (spec 072 research R6, data-model.md
//! `chat_outbox_no_sync`): one row per event with the recipient vaults no Nostr-Relay accepted yet.
//!
//! An attempt that leaves recipients pending waits 5 s, 15 s, 1 min, 5 min, then every 15 min.
//! After 24 h the remaining deliveries are _failed_ and the row is parked; "Erneut senden" wakes it
//! again. `ponytail:` the outbox is device-local, so a message queued on a device that never comes
//! back stays _waiting_; upgrade: a synced claim lets another own device take it over.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};

use super::ids;
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

/// The waits after the first attempts; every later one waits the last.
const BACKOFF_MS: [i64; 5] = [5_000, 15_000, 60_000, 300_000, 900_000];
/// How long an event may stay without any Nostr-Relay accepting it.
pub const GIVE_UP_MS: i64 = 24 * 60 * 60 * 1000;
/// `next_at` of a parked row: never due until "Erneut senden".
const PARKED: i64 = i64::MAX;

/// Why a delivery failed (data-model.md `failed_reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailReason {
    /// No Nostr-Relay of the recipient (nor an own one) accepted it in time.
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
    pub attempts: i64,
    pub created_at: i64,
}

/// The wait after `attempts` attempts.
pub fn backoff(attempts: i64) -> i64 {
    let index = usize::try_from(attempts.saturating_sub(1)).unwrap_or(0);
    BACKOFF_MS[index.min(BACKOFF_MS.len() - 1)]
}

/// Queues an event for `pending`, due at once.
pub fn enqueue(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    thread_id: &str,
    message_id: Option<&str>,
    rumor: &Rumor,
    pending: &[String],
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO chat_outbox_no_sync \
           (id, thread_id, message_id, rumor, pending, attempts, next_at, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6)",
        params![
            id,
            thread_id,
            message_id,
            to_json(rumor)?,
            to_json(&pending)?,
            now
        ],
    )?;
    Ok(())
}

/// The rows due at `now`, oldest first.
pub fn due(q: &mut impl Query, now: i64) -> Result<Vec<Entry>> {
    let rows = q.query_map(
        "SELECT id, thread_id, message_id, rumor, pending, attempts, created_at \
         FROM chat_outbox_no_sync WHERE next_at <= ?1 ORDER BY created_at, id",
        params![now],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        },
    )?;
    rows.into_iter()
        .map(
            |(id, thread_id, message_id, rumor, pending, attempts, created_at)| {
                Ok(Entry {
                    id,
                    thread_id,
                    message_id,
                    rumor: from_json(&rumor)?,
                    pending: from_json(&pending)?,
                    attempts,
                    created_at,
                })
            },
        )
        .collect()
}

/// What one attempt at `entry` did: the vaults a Nostr-Relay accepted, those that failed for good,
/// and the error of the rest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attempt {
    pub accepted: Vec<String>,
    pub failed: Vec<(String, FailReason)>,
    pub error: Option<String>,
}

/// Records an attempt: accepted vaults get `sent_at`, failed ones `failed_at`, the rest waits for
/// the next attempt, or fails as `noRelay` once [`GIVE_UP_MS`] passed. A row without anything
/// pending is removed unless a delivery of its message failed; that row, like one that gave up, is
/// parked for "Erneut senden".
pub fn record(
    tx: &mut CrdtTransaction<'_>,
    entry: &Entry,
    attempt: &Attempt,
    now: i64,
) -> Result<()> {
    for vault in &attempt.accepted {
        mark(tx, entry, vault, "sent_at", None, now)?;
    }
    for (vault, reason) in &attempt.failed {
        mark(tx, entry, vault, "failed_at", Some(*reason), now)?;
    }
    let pending: Vec<String> = entry
        .pending
        .iter()
        .filter(|vault| {
            !attempt.accepted.contains(vault)
                && !attempt.failed.iter().any(|(failed, _)| failed == *vault)
        })
        .cloned()
        .collect();
    let attempts = entry.attempts + 1;
    if pending.is_empty() {
        // A message with a failed delivery stays parked so "Erneut senden" finds its rumor.
        if attempt.failed.is_empty() || entry.message_id.is_none() {
            tx.execute(
                "DELETE FROM chat_outbox_no_sync WHERE id = ?1",
                params![entry.id],
            )?;
        } else {
            tx.execute(
                "UPDATE chat_outbox_no_sync SET pending = '[]', attempts = ?2, next_at = ?3 \
                 WHERE id = ?1",
                params![entry.id, attempts, PARKED],
            )?;
        }
        return Ok(());
    }
    let next_at = if now - entry.created_at >= GIVE_UP_MS {
        for vault in &pending {
            mark(
                tx,
                entry,
                vault,
                "failed_at",
                Some(FailReason::NoRelay),
                now,
            )?;
        }
        PARKED
    } else {
        now + backoff(attempts)
    };
    tx.execute(
        "UPDATE chat_outbox_no_sync SET pending = ?2, attempts = ?3, next_at = ?4, last_error = ?5 \
         WHERE id = ?1",
        params![entry.id, to_json(&pending)?, attempts, next_at, attempt.error],
    )?;
    Ok(())
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
    let Some(message_id) = entry.message_id.as_deref() else {
        return Ok(());
    };
    let Ok(message) = message_id.parse() else {
        return Ok(());
    };
    let id = ids::delivery_id(message, vault).to_string();
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

/// Makes every waiting row due at once (not the parked ones): after the vault opened or the
/// connections came back, the back-off of attempts that failed while offline does not apply.
pub fn due_now(tx: &mut CrdtTransaction<'_>, now: i64) -> Result<()> {
    tx.execute(
        "UPDATE chat_outbox_no_sync SET next_at = ?1 WHERE next_at > ?1 AND next_at < ?2",
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
    serde_json::to_string(value).map_err(|e| HolziError::InvalidInput {
        reason: format!("outbox: {e}"),
    })
}

fn from_json<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|e| HolziError::InvalidInput {
        reason: format!("outbox: {e}"),
    })
}

#[cfg(test)]
#[path = "outbox_tests.rs"]
mod tests;
