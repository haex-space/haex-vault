//! Clearing up on this device after an extension was removed (spec 017, T079, research R11).
//!
//! The removing device sets `purge_hlc` (and `purge_data` for "delete data") on the extension's
//! row. Every device, the removing one included, clears up once per new `purge_hlc` in one local
//! write: always this device's key-value rows and the logs; with `purge_data` also the prefixed
//! tables, the migration journal and the parked sync groups older than the removal. Newer groups
//! were parked by the sync receiver while this clear-up was due and apply after the migrations of
//! a reinstall. The trigger is `purge_hlc`, not `state`: a device that was offline while the
//! extension was removed and installed again only sees `state = installed`, yet must clear up.
//! Changes older than `purge_hlc` that arrive later are dropped by the sync receiver
//! (`sync::inbound::park`).

use std::sync::{Arc, PoisonError};

use haex_crdt::rusqlite::params;
use haex_crdt::{compare_hlc_strings, AuthContext, Authorization, GuardedWriteOptions, SqlGuard};
use uuid::Uuid;

use crate::error::Result;
use crate::extensions::ids::{ExtensionTable, TablePrefix};
use crate::extensions::sql::authorizer::MIGRATION_JOURNAL;
use crate::extensions::sql::migrate::applying;
use crate::storage::query::Query;
use crate::sync::inbound::park;
use crate::vault_gate::VaultDb;

/// The last `purge_hlc` this device cleared up for, per extension.
pub const PURGES_APPLIED: &str = "extension_purges_applied_no_sync";

/// A removal as the extension's row records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    pub extension_id: Uuid,
    pub prefix: TablePrefix,
    pub purge_data: bool,
    pub purge_hlc: String,
}

/// Whether `removal` is newer than the last clear-up of its extension on this device.
pub fn due(q: &mut impl Query, removal: &Removal) -> haex_crdt::Result<bool> {
    let applied: Option<String> = q.query_row(
        &format!("SELECT purge_hlc FROM {PURGES_APPLIED} WHERE extension_id = ?1"),
        &[&removal.extension_id.to_string()],
        |r| r.get(0),
    )?;
    Ok(applied.is_none_or(|applied| {
        compare_hlc_strings(&removal.purge_hlc, &applied) == std::cmp::Ordering::Greater
    }))
}

/// The tables and views of `prefix`, device-local ones included.
pub(crate) fn prefixed(
    q: &mut impl Query,
    prefix: &TablePrefix,
) -> haex_crdt::Result<Vec<(String, String)>> {
    let objects: Vec<(String, String)> = q.query_map(
        "SELECT type, name FROM sqlite_master WHERE type IN ('table', 'view')",
        &[],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(objects
        .into_iter()
        .filter(|(_, name)| ExtensionTable::parse(name).is_ok_and(|t| t.prefix == *prefix))
        .collect())
}

pub(crate) fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Clears up for `removal` on `device` in one write and records its `purge_hlc`. Holds the
/// migration lock and reads the tables inside the write, so a migration of a starting frame cannot
/// add a table that would outlive the cleared journal. Blocking.
pub fn run(db: &VaultDb, removal: &Removal, device: Uuid) -> Result<()> {
    let _applying = applying().lock().unwrap_or_else(PoisonError::into_inner);
    // haex-vault's own statements on names read from sqlite_master; schema mode switches foreign keys
    // off, so the tables of one extension can go in any order.
    let guard = SqlGuard {
        authorizer: Arc::new(|_: &AuthContext<'_>| Authorization::Allow),
        progress: None,
        max_value_bytes: None,
    };
    let removal = removal.clone();
    db.write_guarded_blocking(
        &guard,
        GuardedWriteOptions {
            schema_mode: true,
            local: false,
        },
        move |tx| {
            let ext = removal.extension_id.to_string();
            tx.execute(
                "DELETE FROM extension_kv WHERE extension_id = ?1 AND vault_device_uuid = ?2",
                params![ext, device.to_string()],
            )?;
            tx.execute(
                "DELETE FROM extension_logs_no_sync WHERE extension_id = ?1",
                params![ext],
            )?;
            if removal.purge_data {
                for (kind, name) in prefixed(tx, &removal.prefix)? {
                    let kind = if kind == "view" { "VIEW" } else { "TABLE" };
                    tx.execute(&format!("DROP {kind} IF EXISTS {}", quoted(&name)), &[])?;
                }
                tx.execute(
                    &format!("DELETE FROM {MIGRATION_JOURNAL} WHERE extension_id = ?1"),
                    params![ext],
                )?;
                park::discard(tx, &removal.prefix, &removal.purge_hlc)?;
            }
            // A purge only runs for a newer `purge_hlc`, so the new "delete data" one is the highest.
            tx.execute(
                &format!(
                    "INSERT INTO {PURGES_APPLIED} (extension_id, purge_hlc, data_purge_hlc) \
                     VALUES (?1, ?2, CASE WHEN ?3 THEN ?2 END) \
                     ON CONFLICT(extension_id) DO UPDATE SET purge_hlc = excluded.purge_hlc, \
                     data_purge_hlc = COALESCE(excluded.data_purge_hlc, data_purge_hlc)"
                ),
                params![ext, removal.purge_hlc, removal.purge_data],
            )?;
            Ok(())
        },
    )?;
    Ok(())
}
