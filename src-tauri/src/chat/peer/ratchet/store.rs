//! The rows of this installation in the ratchet tables of migration `0063_chat_ratchet`
//! (data-model.md): its account, its sessions with peer devices and what it remembers about those
//! devices. Every function takes the installation and puts it into the `WHERE`, so no function reads,
//! changes or deletes a row of another installation (a copied vault file, SC-007).
//!
//! Pickles are held as [`Zeroizing`] strings, and the row types print no pickle in `Debug`.

use haex_crdt::rusqlite::{params, Row};
use haex_crdt::CrdtTransaction;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::error::Result;
use crate::storage::query::Query;

/// The ratchet account of an installation.
pub struct AccountRow {
    /// **Secret**: the pickled Olm account (JSON).
    pub account: Zeroizing<String>,
    pub start_key_made_at: i64,
    pub record_published_at: Option<i64>,
}

impl std::fmt::Debug for AccountRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountRow")
            .field("start_key_made_at", &self.start_key_made_at)
            .field("record_published_at", &self.record_published_at)
            .finish_non_exhaustive()
    }
}

/// A session of an installation with a peer device.
#[derive(Clone)]
pub struct SessionRow {
    /// The peer's device key, hex.
    pub peer_device: String,
    pub session_id: String,
    /// **Secret**: the pickled Olm session (JSON).
    pub session: Zeroizing<String>,
    /// The peer's ratchet identity key the session was opened with (base64).
    pub peer_identity_key: String,
    pub created_at: i64,
    pub last_received_at: Option<i64>,
    pub last_sent_at: Option<i64>,
    /// The ratchet key this device sends with (base64) and since when.
    pub own_ratchet_key: Option<String>,
    pub own_key_since: Option<i64>,
    /// When this device made the newest ratchet key the peer answered with a new one of its own.
    pub renewed_at: Option<i64>,
}

impl SessionRow {
    /// The newest of the times it was used; the least recently used session goes first.
    pub fn last_used(&self) -> i64 {
        self.created_at
            .max(self.last_received_at.unwrap_or(i64::MIN))
            .max(self.last_sent_at.unwrap_or(i64::MIN))
    }
}

impl std::fmt::Debug for SessionRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionRow")
            .field("peer_device", &self.peer_device)
            .field("session_id", &self.session_id)
            .field("created_at", &self.created_at)
            .field("last_received_at", &self.last_received_at)
            .field("last_sent_at", &self.last_sent_at)
            .field("own_key_since", &self.own_key_since)
            .field("renewed_at", &self.renewed_at)
            .finish_non_exhaustive()
    }
}

/// What an installation remembers about a peer device beside its sessions; no secret.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerRow {
    pub reset_sent_at: Option<i64>,
    pub reset_received_at: Option<i64>,
    pub recovering_since: Option<i64>,
    pub refetch_record: bool,
}

pub fn account(q: &mut impl Query, installation: Uuid) -> Result<Option<AccountRow>> {
    Ok(q.query_row(
        "SELECT account, start_key_made_at, record_published_at \
         FROM chat_ratchet_account_no_sync WHERE installation_uuid = ?1",
        params![installation.to_string()],
        |row| {
            Ok(AccountRow {
                account: Zeroizing::new(row.get(0)?),
                start_key_made_at: row.get(1)?,
                record_published_at: row.get(2)?,
            })
        },
    )?)
}

/// Writes the account row of `installation`, created the first time.
pub fn put_account(
    tx: &mut CrdtTransaction<'_>,
    installation: Uuid,
    row: &AccountRow,
) -> Result<()> {
    tx.execute(
        "INSERT INTO chat_ratchet_account_no_sync \
           (installation_uuid, account, start_key_made_at, record_published_at) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT (installation_uuid) DO UPDATE SET account = excluded.account, \
           start_key_made_at = excluded.start_key_made_at, \
           record_published_at = excluded.record_published_at",
        params![
            installation.to_string(),
            row.account.as_str(),
            row.start_key_made_at,
            row.record_published_at
        ],
    )?;
    Ok(())
}

const SESSION_COLUMNS: &str = "peer_device, session_id, session, peer_identity_key, created_at, \
     last_received_at, last_sent_at, own_ratchet_key, own_key_since, renewed_at";

/// Writes a session row: a new one, or the advanced state over the existing one.
pub(super) const UPSERT_SESSION: &str = "INSERT INTO chat_ratchet_sessions_no_sync \
       (installation_uuid, peer_device, session_id, session, peer_identity_key, created_at, \
        last_received_at, last_sent_at, own_ratchet_key, own_key_since, renewed_at) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
     ON CONFLICT (installation_uuid, peer_device, session_id) DO UPDATE SET \
       session = excluded.session, peer_identity_key = excluded.peer_identity_key, \
       last_received_at = excluded.last_received_at, last_sent_at = excluded.last_sent_at, \
       own_ratchet_key = excluded.own_ratchet_key, own_key_since = excluded.own_key_since, \
       renewed_at = excluded.renewed_at";

/// Deletes one session row.
pub(super) const DELETE_SESSION: &str = "DELETE FROM chat_ratchet_sessions_no_sync \
     WHERE installation_uuid = ?1 AND peer_device = ?2 AND session_id = ?3";

fn session_row(row: &Row<'_>) -> haex_crdt::rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        peer_device: row.get(0)?,
        session_id: row.get(1)?,
        session: Zeroizing::new(row.get(2)?),
        peer_identity_key: row.get(3)?,
        created_at: row.get(4)?,
        last_received_at: row.get(5)?,
        last_sent_at: row.get(6)?,
        own_ratchet_key: row.get(7)?,
        own_key_since: row.get(8)?,
        renewed_at: row.get(9)?,
    })
}

/// The sessions of `installation` with `peer_device`, by session id (string order).
pub fn sessions(
    q: &mut impl Query,
    installation: Uuid,
    peer_device: &str,
) -> Result<Vec<SessionRow>> {
    Ok(q.query_map(
        &format!(
            "SELECT {SESSION_COLUMNS} FROM chat_ratchet_sessions_no_sync \
             WHERE installation_uuid = ?1 AND peer_device = ?2 ORDER BY session_id"
        ),
        params![installation.to_string(), peer_device],
        session_row,
    )?)
}

/// Writes a session row of `installation`: a new one or the advanced state of an existing one.
pub fn put_session(
    tx: &mut CrdtTransaction<'_>,
    installation: Uuid,
    row: &SessionRow,
) -> Result<()> {
    tx.execute(
        UPSERT_SESSION,
        params![
            installation.to_string(),
            row.peer_device,
            row.session_id,
            row.session.as_str(),
            row.peer_identity_key,
            row.created_at,
            row.last_received_at,
            row.last_sent_at,
            row.own_ratchet_key,
            row.own_key_since,
            row.renewed_at
        ],
    )?;
    Ok(())
}

pub fn delete_session(
    tx: &mut CrdtTransaction<'_>,
    installation: Uuid,
    peer_device: &str,
    session_id: &str,
) -> Result<()> {
    tx.execute(
        DELETE_SESSION,
        params![installation.to_string(), peer_device, session_id],
    )?;
    Ok(())
}

/// Deletes every session of `installation` with `peer_device`.
pub fn delete_sessions(
    tx: &mut CrdtTransaction<'_>,
    installation: Uuid,
    peer_device: &str,
) -> Result<()> {
    tx.execute(
        "DELETE FROM chat_ratchet_sessions_no_sync \
         WHERE installation_uuid = ?1 AND peer_device = ?2",
        params![installation.to_string(), peer_device],
    )?;
    Ok(())
}

/// What `installation` remembers about `peer_device`; the default when nothing.
pub fn peer(q: &mut impl Query, installation: Uuid, peer_device: &str) -> Result<PeerRow> {
    Ok(q.query_row(
        "SELECT reset_sent_at, reset_received_at, recovering_since, refetch_record \
             FROM chat_ratchet_peers_no_sync WHERE installation_uuid = ?1 AND peer_device = ?2",
        params![installation.to_string(), peer_device],
        |row| {
            Ok(PeerRow {
                reset_sent_at: row.get(0)?,
                reset_received_at: row.get(1)?,
                recovering_since: row.get(2)?,
                refetch_record: row.get::<_, i64>(3)? != 0,
            })
        },
    )?
    .unwrap_or_default())
}

pub fn put_peer(
    tx: &mut CrdtTransaction<'_>,
    installation: Uuid,
    peer_device: &str,
    row: &PeerRow,
) -> Result<()> {
    tx.execute(
        "INSERT INTO chat_ratchet_peers_no_sync (installation_uuid, peer_device, reset_sent_at, \
           reset_received_at, recovering_since, refetch_record) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT (installation_uuid, peer_device) DO UPDATE SET \
           reset_sent_at = excluded.reset_sent_at, \
           reset_received_at = excluded.reset_received_at, \
           recovering_since = excluded.recovering_since, \
           refetch_record = excluded.refetch_record",
        params![
            installation.to_string(),
            peer_device,
            row.reset_sent_at,
            row.reset_received_at,
            row.recovering_since,
            i64::from(row.refetch_record)
        ],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
