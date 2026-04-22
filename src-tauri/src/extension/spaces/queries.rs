//! Extension Spaces Queries
//!
//! Centralized SQL for `haex_shared_space_sync`. Follows the `lazy_static!` +
//! `format!` pattern established in `extension/core/queries.rs`.
//!
//! The table maps rows in any CRDT-synced table to a shared space and —
//! optionally — records the *portable* identity of the owning extension via
//! a composite FK `(extension_public_key, extension_name) -> haex_extensions`.
//! No local UUIDs here: identity must survive cross-vault CRDT replication.

use crate::table_names::{
    COL_SHARED_SPACE_SYNC_CREATED_AT, COL_SHARED_SPACE_SYNC_EXTENSION_NAME,
    COL_SHARED_SPACE_SYNC_EXTENSION_PUBLIC_KEY, COL_SHARED_SPACE_SYNC_GROUP_ID,
    COL_SHARED_SPACE_SYNC_ID, COL_SHARED_SPACE_SYNC_LABEL, COL_SHARED_SPACE_SYNC_ROW_PKS,
    COL_SHARED_SPACE_SYNC_SPACE_ID, COL_SHARED_SPACE_SYNC_TABLE_NAME, COL_SHARED_SPACE_SYNC_TYPE,
    TABLE_SHARED_SPACE_SYNC,
};
use lazy_static::lazy_static;

lazy_static! {
    /// Column list for every SELECT against `haex_shared_space_sync`.
    /// Order must match the index lookups in `commands::extension_space_get_assignments`.
    pub static ref SQL_SHARED_SPACE_SYNC_SELECT_COLS: String = format!(
        "SELECT {COL_SHARED_SPACE_SYNC_ID}, {COL_SHARED_SPACE_SYNC_TABLE_NAME}, \
                {COL_SHARED_SPACE_SYNC_ROW_PKS}, {COL_SHARED_SPACE_SYNC_SPACE_ID}, \
                {COL_SHARED_SPACE_SYNC_EXTENSION_PUBLIC_KEY}, {COL_SHARED_SPACE_SYNC_EXTENSION_NAME}, \
                {COL_SHARED_SPACE_SYNC_GROUP_ID}, {COL_SHARED_SPACE_SYNC_TYPE}, \
                {COL_SHARED_SPACE_SYNC_LABEL}, {COL_SHARED_SPACE_SYNC_CREATED_AT} \
         FROM {TABLE_SHARED_SPACE_SYNC}"
    );

    /// Assign rows to a shared space with the portable extension identity.
    /// Params (in order):
    ///   1 id, 2 table_name, 3 row_pks, 4 space_id,
    ///   5 extension_public_key, 6 extension_name,
    ///   7 group_id, 8 type, 9 label.
    ///
    /// Security: columns 5+6 MUST be filled from the authenticated extension
    /// manifest, never from caller-provided strings. See `extension_space_assign`.
    pub static ref SQL_INSERT_SHARED_SPACE_SYNC: String = format!(
        "INSERT OR IGNORE INTO {TABLE_SHARED_SPACE_SYNC} \
         ({COL_SHARED_SPACE_SYNC_ID}, {COL_SHARED_SPACE_SYNC_TABLE_NAME}, \
          {COL_SHARED_SPACE_SYNC_ROW_PKS}, {COL_SHARED_SPACE_SYNC_SPACE_ID}, \
          {COL_SHARED_SPACE_SYNC_EXTENSION_PUBLIC_KEY}, {COL_SHARED_SPACE_SYNC_EXTENSION_NAME}, \
          {COL_SHARED_SPACE_SYNC_GROUP_ID}, {COL_SHARED_SPACE_SYNC_TYPE}, \
          {COL_SHARED_SPACE_SYNC_LABEL}) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    );

    /// Unassign by (table_name, row_pks, space_id) — matches the uniqueness
    /// index; extension-prefix check is done in the caller before running this.
    pub static ref SQL_DELETE_SHARED_SPACE_SYNC: String = format!(
        "DELETE FROM {TABLE_SHARED_SPACE_SYNC} \
         WHERE {COL_SHARED_SPACE_SYNC_TABLE_NAME} = ?1 \
           AND {COL_SHARED_SPACE_SYNC_ROW_PKS} = ?2 \
           AND {COL_SHARED_SPACE_SYNC_SPACE_ID} = ?3"
    );
}
