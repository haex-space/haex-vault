//! The switch "Geräte-Dateizugriff" (spec 056 FR-014, research R12): whether other devices of this
//! vault may reach this device's files. It is on unless turned off here, and it lives in a
//! `_no_sync` table, so no other device can turn it back on.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use crate::error::Result;
use crate::storage::query::Query;

/// Whether other own devices may reach this device's files; on when never set.
pub fn enabled(q: &mut impl Query) -> Result<bool> {
    Ok(q.query_row(
        "SELECT enabled FROM haex_file_device_access_no_sync WHERE id = 1",
        &[],
        |r| r.get::<_, i64>(0),
    )?
    .is_none_or(|enabled| enabled != 0))
}

/// Turns the switch on or off for this device.
pub fn set_enabled(tx: &mut CrdtTransaction<'_>, enabled: bool, now: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO haex_file_device_access_no_sync (id, enabled, updated_at) VALUES (1, ?1, ?2) \
         ON CONFLICT(id) DO UPDATE SET enabled = excluded.enabled, updated_at = excluded.updated_at",
        params![i64::from(enabled), now],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "device_access_tests.rs"]
mod tests;
