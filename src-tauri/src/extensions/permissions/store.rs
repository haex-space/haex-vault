//! Remembered permissions in `extension_permissions` (data-model.md). Row ids derive from the
//! natural key (`ids::permission_id`), so two devices that remember the same permission write the
//! same row. Writes are check-then-write like the other CRDT tables (`storage/preferences.rs`).
//!
//! A development version (US12, research R16) keeps its permissions in
//! `dev_extension_permissions_no_sync` on this device. Its id comes from another namespace than an
//! installed extension's, so its rows never collide with those of `extension_permissions`.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::{Permission, PermissionKind, VAULT_WIDE};
use crate::error::Result;
use crate::extensions::ids::permission_id;
use crate::storage::query::Query;

/// One stored row as text; [`super::Permission::from_row`] reads it for decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRow {
    pub id: Uuid,
    pub kind: String,
    pub action: String,
    pub target: String,
    pub status: String,
    pub declared: bool,
    pub vault_device_uuid: Uuid,
}

impl PermissionRow {
    /// Whether this row is about the same (kind, action, target).
    pub fn is_about(&self, kind: &str, action: &str, target: &str) -> bool {
        self.kind == kind && self.action == action && self.target == target
    }
}

const SYNCED: &str = "extension_permissions";
const DEV: &str = "dev_extension_permissions_no_sync";

/// The tables that hold remembered permissions: a write to one can change what an extension may
/// read.
pub const TABLES: [&str; 2] = [SYNCED, DEV];

/// The table that holds the permissions of `extension_id`.
fn table_of(q: &mut impl Query, extension_id: Uuid) -> Result<&'static str> {
    let dev = q
        .query_row(
            "SELECT COUNT(*) FROM dev_extensions_no_sync WHERE id = ?1",
            &[&extension_id.to_string()],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    Ok(if dev { DEV } else { SYNCED })
}

/// Every row of an extension, on every device.
pub fn rows_of(q: &mut impl Query, extension_id: Uuid) -> Result<Vec<PermissionRow>> {
    let table = table_of(q, extension_id)?;
    let rows = q.query_map(
        &format!(
            "SELECT id, kind, action, target, status, declared, vault_device_uuid \
             FROM {table} WHERE extension_id = ?1"
        ),
        &[&extension_id.to_string()],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
            ))
        },
    )?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, kind, action, target, status, declared, device)| {
            Some(PermissionRow {
                id: Uuid::parse_str(&id).ok()?,
                kind,
                action,
                target,
                status,
                declared: declared != 0,
                vault_device_uuid: Uuid::parse_str(&device).ok()?,
            })
        })
        .collect())
}

/// The remembered permissions of one kind that can hold on `device`: vault-wide or this device's.
/// Rows haex-vault cannot read are absent (FR-022). A vault-wide allow of a device-scoped kind dates from
/// before the clarification of 2026-10-06 (FR-018) and counts nowhere, so every device asks again;
/// a vault-wide deny of it still holds.
pub fn candidates(
    q: &mut impl Query,
    extension_id: Uuid,
    kind: PermissionKind,
    device: Uuid,
) -> Result<Vec<Permission>> {
    Ok(rows_of(q, extension_id)?
        .into_iter()
        .filter(|r| r.kind == kind.as_str())
        .filter(|r| r.vault_device_uuid == VAULT_WIDE || r.vault_device_uuid == device)
        .filter(|r| {
            !(kind.is_device_scoped() && !kind.fits(r.vault_device_uuid) && r.status == "granted")
        })
        .filter_map(|r| {
            Permission::from_row(
                &r.kind,
                &r.action,
                &r.target,
                &r.status,
                r.vault_device_uuid,
            )
        })
        .collect())
}

/// What to write for one permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPermission<'a> {
    pub kind: &'a str,
    pub action: &'a str,
    pub target: &'a str,
    pub status: &'a str,
    pub declared: bool,
    pub vault_device_uuid: Uuid,
}

/// Writes a permission: a new row, or the state and `declared` of the row with the same key.
pub fn put(
    tx: &mut CrdtTransaction<'_>,
    extension_id: Uuid,
    permission: &NewPermission<'_>,
    now_ms: i64,
) -> Result<Uuid> {
    let id = permission_id(
        extension_id,
        permission.kind,
        permission.action,
        permission.target,
        permission.vault_device_uuid,
    );
    let key = id.to_string();
    let table = table_of(tx, extension_id)?;
    let known = tx
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE id = ?1"),
            params![key],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    if known {
        tx.execute(
            &format!(
                "UPDATE {table} SET status = ?2, declared = ?3, updated_at = ?4 WHERE id = ?1"
            ),
            params![key, permission.status, permission.declared, now_ms],
        )?;
    } else {
        tx.execute(
            &format!(
                "INSERT INTO {table} (id, extension_id, kind, action, target, status, \
                 declared, vault_device_uuid, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
            ),
            params![
                key,
                extension_id.to_string(),
                permission.kind,
                permission.action,
                permission.target,
                permission.status,
                permission.declared,
                permission.vault_device_uuid.to_string(),
                now_ms
            ],
        )?;
    }
    Ok(id)
}

/// Marks a row as declared by the manifest, keeping its state. The id is in one table only.
pub fn mark_declared(tx: &mut CrdtTransaction<'_>, id: Uuid, now_ms: i64) -> Result<()> {
    for table in [SYNCED, DEV] {
        tx.execute(
            &format!("UPDATE {table} SET declared = 1, updated_at = ?2 WHERE id = ?1"),
            params![id.to_string(), now_ms],
        )?;
    }
    Ok(())
}

/// Deletes a row; the id is in one table only.
pub fn delete(tx: &mut CrdtTransaction<'_>, id: Uuid) -> Result<()> {
    for table in [SYNCED, DEV] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE id = ?1"),
            params![id.to_string()],
        )?;
    }
    Ok(())
}
