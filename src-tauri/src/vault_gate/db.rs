//! `VaultDb`: the database handle that carries a tracker token (data-model.md), and the one way
//! requests read and write the vault (spec 024, research R19).
//!
//! [`VaultDb::write`] runs a closure in one [`Database::write`] transaction: every statement goes
//! through haex-crdt's CRDT transformer and shares the transaction's HLC, so haex-vault never stamps
//! an HLC by hand and the whole closure is one transaction group for sync. [`VaultDb::read`]
//! runs a closure on haex-crdt's read-only view. Both run on the blocking pool; the closure owns
//! a clone of the handle, so the tracker stays busy until the SQL is done even if the awaiting
//! request is dropped.

use std::ops::Deref;
use std::sync::Arc;

use haex_crdt::{CrdtTransaction, Database, GuardedWriteOptions, ReadOnlyConnection, SqlGuard};
use tokio::sync::Notify;
use tokio_util::task::task_tracker::TaskTrackerToken;

use crate::error::{HaexError, Result};
use crate::storage::query::Reader;

/// A handle to the vault database that keeps the gate's tracker non-empty while any clone is
/// alive. The tracker is empty exactly when no request still uses the database, so the drain can
/// then take the database out of the app state and drop it, which closes the connection.
#[derive(Clone)]
pub struct VaultDb {
    db: Arc<Database>,
    sync_notify: Arc<Notify>,
    _token: TaskTrackerToken,
}

impl VaultDb {
    pub(super) fn new(
        db: Arc<Database>,
        sync_notify: Arc<Notify>,
        token: TaskTrackerToken,
    ) -> Self {
        Self {
            db,
            sync_notify,
            _token: token,
        }
    }

    /// Runs `f` in one CRDT write transaction and commits when it returns `Ok`; on `Err` nothing
    /// is written. After a commit it wakes the sync service ([`super::VaultGate::sync_notify`]).
    pub async fn write<R, F>(&self, f: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut CrdtTransaction<'_>) -> haex_crdt::Result<R> + Send + 'static,
    {
        let this = self.clone();
        tauri::async_runtime::spawn_blocking(move || this.write_blocking(f))
            .await
            .map_err(|e| HaexError::CrdtInit {
                reason: format!("vault write join: {e}"),
            })?
    }

    /// Runs `f` on a read-only view of the vault.
    pub async fn read<R, F>(&self, f: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut Reader<'_, '_>) -> haex_crdt::Result<R> + Send + 'static,
    {
        let this = self.clone();
        tauri::async_runtime::spawn_blocking(move || this.read_blocking(f))
            .await
            .map_err(|e| HaexError::CrdtInit {
                reason: format!("vault read join: {e}"),
            })?
    }

    /// [`Self::write`] for callers already on a blocking thread.
    pub fn write_blocking<R>(
        &self,
        f: impl FnOnce(&mut CrdtTransaction<'_>) -> haex_crdt::Result<R>,
    ) -> Result<R> {
        let value = self.db.write(f)?;
        self.sync_notify.notify_one();
        Ok(value)
    }

    /// [`Self::read`] for callers already on a blocking thread.
    pub fn read_blocking<R>(
        &self,
        f: impl FnOnce(&mut Reader<'_, '_>) -> haex_crdt::Result<R>,
    ) -> Result<R> {
        Ok(crate::storage::query::read(&self.db, f)?)
    }

    /// [`Self::write_blocking`] with a [`SqlGuard`] around the caller's statements (spec 017:
    /// SQL of extensions), in schema or local mode as `options` say.
    pub fn write_guarded_blocking<R>(
        &self,
        guard: &SqlGuard,
        options: GuardedWriteOptions,
        f: impl FnOnce(&mut CrdtTransaction<'_>) -> haex_crdt::Result<R>,
    ) -> haex_crdt::Result<R> {
        let value = self.db.write_guarded_with(guard, options, f)?;
        self.sync_notify.notify_one();
        Ok(value)
    }

    /// [`Self::read_blocking`] with a [`SqlGuard`] around the caller's statements.
    pub fn read_guarded_blocking<R>(
        &self,
        guard: &SqlGuard,
        f: impl FnOnce(&ReadOnlyConnection<'_>) -> haex_crdt::Result<R>,
    ) -> haex_crdt::Result<R> {
        self.db.read_guarded(guard, f)
    }

    /// The underlying database handle, for building an owner of the vault
    /// for the session's lifetime (spec 024: [`crate::sync::replica::Replica`]).
    /// Cloning this `Arc` does not extend the tracker token's lifetime; the
    /// caller's own task must still observe the gate's cancellation.
    pub fn database(&self) -> Arc<Database> {
        Arc::clone(&self.db)
    }
}

impl Deref for VaultDb {
    type Target = Database;

    fn deref(&self) -> &Database {
        &self.db
    }
}

#[cfg(test)]
#[path = "db_tests.rs"]
mod db_tests;
