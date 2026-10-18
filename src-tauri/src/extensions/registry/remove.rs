//! Removing an extension from the vault (spec 017, FR-008, research R11). The removing device
//! keeps the `extensions` row as a tombstone with `state = removed`, `purge_data` and `purge_hlc`
//! (the HLC of this very write), and deletes the other registry rows the normal way, so the
//! deletions sync. The clear-up of tables and device-local state runs on every device, this one
//! included, once it sees the new `purge_hlc` ([`super::purge`], [`super::lifecycle`]).
//!
//! With "keep data" the migrations stay vault data, so every device, also one added later, can
//! still create the extension's tables and take over synced rows for them (FR-008, FR-037).
//! [`purge_kept_data`] deletes such kept data later, as a removal with "delete data" would have.
//!
//! BLOBs no file refers to any more get `orphaned_at`; they are collected after seven days
//! (data-model.md, [`super::blobs`]), so a reinstall in between needs no new transfer.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::vault_gate::VaultDb;

/// Sets `purge_data` and a new `purge_hlc` on the row of `extension_id`: the transformer stamps
/// the row with this transaction's HLC, which becomes the removal's.
fn stamp_removal(tx: &mut CrdtTransaction<'_>, ext: &str, delete_data: bool) -> Result<()> {
    tx.execute(
        "UPDATE extensions SET purge_data = ?2 WHERE id = ?1",
        params![ext, delete_data],
    )?;
    tx.execute(
        "UPDATE extensions SET purge_hlc = haex_hlc_no_sync WHERE id = ?1",
        params![ext],
    )?;
    Ok(())
}

fn has_row(tx: &mut CrdtTransaction<'_>, sql: &str, ext: &str) -> Result<bool> {
    Ok(tx
        .query_row(sql, params![ext], |r| r.get::<_, i64>(0))?
        .unwrap_or(0)
        > 0)
}

/// Removes `extension_id`; with `delete_data` every device also drops its tables. Removing an
/// extension that is not installed is refused.
pub fn remove(db: &VaultDb, extension_id: Uuid, delete_data: bool, now_ms: i64) -> Result<()> {
    db.write_blocking(move |tx| {
        let ext = extension_id.to_string();
        if !has_row(
            tx,
            "SELECT COUNT(*) FROM extensions WHERE id = ?1 AND state = 'installed'",
            &ext,
        )? {
            return Err(HolziError::ExtensionNotFound.into());
        }
        tx.execute(
            "UPDATE extensions SET state = 'removed', updated_at = ?2 WHERE id = ?1",
            params![ext, now_ms],
        )?;
        stamp_removal(tx, &ext, delete_data)?;
        for sql in [
            "DELETE FROM extension_bundle_files WHERE bundle_id IN \
             (SELECT id FROM extension_bundles WHERE extension_id = ?1)",
            "DELETE FROM extension_bundles WHERE extension_id = ?1",
            "DELETE FROM extension_permissions WHERE extension_id = ?1",
            "DELETE FROM extension_limits WHERE extension_id = ?1",
            "DELETE FROM extension_model_order WHERE extension_id = ?1",
            "DELETE FROM extension_device_status WHERE extension_id = ?1",
        ] {
            tx.execute(sql, params![ext])?;
        }
        if delete_data {
            tx.execute(
                "DELETE FROM extension_migrations WHERE extension_id = ?1",
                params![ext],
            )?;
        }
        tx.execute(
            "UPDATE extension_blobs SET orphaned_at = ?1 WHERE orphaned_at IS NULL \
             AND hash NOT IN (SELECT sha256 FROM extension_bundle_files)",
            params![now_ms],
        )?;
        Ok(())
    })
}

/// Deletes the data a removal with "keep data" kept: every device drops the extension's tables
/// once it sees the new `purge_hlc`. Refused for an extension that is installed or whose data is
/// already gone.
pub fn purge_kept_data(db: &VaultDb, extension_id: Uuid, now_ms: i64) -> Result<()> {
    db.write_blocking(move |tx| {
        let ext = extension_id.to_string();
        if !has_row(
            tx,
            "SELECT COUNT(*) FROM extensions \
             WHERE id = ?1 AND state = 'removed' AND purge_data = 0",
            &ext,
        )? {
            return Err(HolziError::ExtensionNotFound.into());
        }
        tx.execute(
            "UPDATE extensions SET updated_at = ?2 WHERE id = ?1",
            params![ext, now_ms],
        )?;
        stamp_removal(tx, &ext, true)?;
        tx.execute(
            "DELETE FROM extension_migrations WHERE extension_id = ?1",
            params![ext],
        )?;
        Ok(())
    })
}

/// Enables or disables an installed extension on every device (FR-007, FR-039): a disabled one
/// runs no host function (8002), leaves the launcher and closes its tabs; its data stays.
pub fn set_enabled(db: &VaultDb, extension_id: Uuid, enabled: bool, now_ms: i64) -> Result<()> {
    db.write_blocking(move |tx| {
        let ext = extension_id.to_string();
        if !has_row(
            tx,
            "SELECT COUNT(*) FROM extensions WHERE id = ?1 AND state = 'installed'",
            &ext,
        )? {
            return Err(HolziError::ExtensionNotFound.into());
        }
        tx.execute(
            "UPDATE extensions SET enabled = ?2, updated_at = ?3 \
             WHERE id = ?1 AND enabled <> ?2",
            params![ext, enabled, now_ms],
        )?;
        Ok(())
    })
}

#[cfg(test)]
#[path = "remove_tests.rs"]
mod tests;
