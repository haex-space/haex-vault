//! This device's copy of the vault as the sync sees it (spec 024).
//!
//! The sender reads its progress and then scans table by table; haex-crdt
//! locks its connection per call, not across the scan. Two things keep a
//! pull gap-free all the same:
//!
//! - Changes the sync applies from other devices take the exchange lock,
//!   together with the progress they advance, and so does serving a pull.
//!   A scan therefore never sees a foreign change without the progress
//!   that covers the changes before it.
//! - Own writes do not take the lock. The sender only serves changes up to
//!   the progress it read first ([`crate::sync::outbound`]), so an own
//!   write that lands during the scan waits for the next pull.

use std::sync::{Arc, Mutex, MutexGuard};

use haex_crdt::Database;
use tokio_util::task::task_tracker::TaskTrackerToken;

use crate::storage::query::{self, Query};
use crate::sync::progress::{self, Vector};

/// The vault is closing: work on its database does not start or go on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the vault is closing")]
pub struct Closing;

/// The vault's database and the lock between applying and serving.
pub struct Replica {
    db: Arc<Database>,
    /// The open vault's gate, so a close waits for long work on the database instead of
    /// ending the process under it (spec 013). `None` for a replica without a vault session.
    gate: Option<crate::vault_gate::VaultGate>,
    exchange: Mutex<()>,
    /// The newest own cell found so far; see [`Replica::progress`].
    own_latest: Mutex<Option<String>>,
}

impl Replica {
    /// Wraps the vault database with an exchange lock and an empty own-progress cache.
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            gate: None,
            exchange: Mutex::new(()),
            own_latest: Mutex::new(None),
        }
    }

    /// This device's progress: the stored rows, and for itself the newest
    /// cell of its own origin in a synced table, or a stored row beyond it.
    ///
    /// The first call looks at every column HLC; later calls only at rows
    /// written since, since a row's HLC is never older than its cells'.
    pub fn progress(&self) -> haex_crdt::Result<Vector> {
        let own = self.db.device_id();
        let mut own_latest = self.own_latest.lock().unwrap_or_else(|e| e.into_inner());
        let (mut vector, latest) = query::read(&self.db, |r| {
            let vector = progress::stored(r)?;
            let latest = newest_own_cell(r, own, own_latest.as_deref())?;
            Ok((vector, latest))
        })?;
        if let Some(latest) = latest {
            if progress::is_beyond(&latest, own_latest.as_ref()) {
                *own_latest = Some(latest);
            }
        }
        if let Some(latest) = own_latest.clone() {
            progress::raise(&mut vector, own, latest);
        }
        Ok(vector)
    }

    /// What a pull asks for: [`Self::progress`], lowered to right before every parked group to
    /// fetch again, and those groups ([`crate::sync::inbound::park::refetch`]). The stored
    /// progress stays; serving and presence use that.
    pub fn pull_vector(
        &self,
    ) -> haex_crdt::Result<(Vector, Vec<crate::sync::inbound::park::refetch::Floor>)> {
        let mut vector = self.progress()?;
        let floors = query::read(&self.db, |r| {
            crate::sync::inbound::park::refetch::floors(r, &mut vector)
        })?;
        Ok((vector, floors))
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    /// A replica of the open vault whose long work counts with `gate` ([`Self::hold`]).
    pub fn tracked(db: Arc<Database>, gate: crate::vault_gate::VaultGate) -> Self {
        Self {
            gate: Some(gate),
            ..Self::new(db)
        }
    }

    /// Counts one piece of work on the database with the vault's close until the token is
    /// dropped: the close waits for it, since a thread still writing when the process ends
    /// crashes it. `Err` once the close has started, so no new work begins.
    pub fn hold(&self) -> Result<Option<TaskTrackerToken>, Closing> {
        self.gate
            .as_ref()
            .map(|gate| gate.tracker_token().map_err(|_| Closing))
            .transpose()
    }

    /// Whether the vault's close has started; long work stops at its next step.
    pub fn closing(&self) -> bool {
        self.gate.as_ref().is_some_and(|gate| gate.is_closing())
    }

    /// Held while applying received changes with their progress, and while
    /// reading progress and scanning for a pull.
    pub(crate) fn exchange(&self) -> MutexGuard<'_, ()> {
        self.exchange.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The newest column HLC of origin `own` in the synced tables, looking only
/// at rows written after `since`.
///
/// `MAX` compares the HLC strings as text. The node suffix is the same for
/// every candidate, and the NTP64 time before it has 20 digits from 1973
/// until its wrap in 2036, so text order is time order.
fn newest_own_cell(
    q: &mut impl Query,
    own: uuid::Uuid,
    since: Option<&str>,
) -> haex_crdt::Result<Option<String>> {
    let own_suffix = format!("%/{:x}", u128::from_le_bytes(*own.as_bytes()));
    let mut newest: Option<String> = None;
    for table in synced_tables(q)? {
        let found: Option<Option<String>> = q.query_row(
            &format!(
                "SELECT MAX(j.value) FROM \"{table}\" AS t, json_each(t.haex_column_hlcs_no_sync) AS j \
                 WHERE (?1 IS NULL OR t.haex_hlc_no_sync > ?1) AND j.value LIKE ?2"
            ),
            haex_crdt::rusqlite::params![since, own_suffix],
            |r| r.get(0),
        )?;
        if let Some(Some(found)) = found {
            progress::raise_option(&mut newest, found);
        }
    }
    Ok(newest)
}

/// The tables that travel: every table with haex-crdt's metadata, the
/// delete log included, except device-local `_no_sync` tables (FR-016).
///
/// Read from the table definitions in `sqlite_master`, which name the
/// metadata column once haex-crdt added it; the read-only view does not
/// allow `pragma_table_info`.
pub fn synced_tables(q: &mut impl Query) -> haex_crdt::Result<Vec<String>> {
    let tables: Vec<String> = q.query_map(
        "SELECT name FROM sqlite_master \
         WHERE type = 'table' AND sql LIKE '%haex_hlc_no_sync%' ORDER BY name",
        &[],
        |r| r.get(0),
    )?;
    Ok(tables
        .into_iter()
        .filter(|table| !table.ends_with("_no_sync"))
        .collect())
}
