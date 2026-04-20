//! Extension Core Queries
//!
//! Centralized SQL for extension CRUD. Follows the `lazy_static!` +
//! `format!` pattern established in `remote_storage/queries.rs`.
//!
//! Queries sourced from `manager.rs`, `installer.rs`, `loader.rs`,
//! `removal.rs`, and `migrations.rs`. The two near-duplicate
//! `UPDATE ... SET enabled = ? WHERE id = ?` patterns (manager/removal)
//! are consolidated into `SQL_UPDATE_EXTENSION_ENABLED`.

use crate::table_names::{
    COL_EXTENSIONS_AUTHOR, COL_EXTENSIONS_DESCRIPTION, COL_EXTENSIONS_DISPLAY_MODE,
    COL_EXTENSIONS_ENABLED, COL_EXTENSIONS_ENTRY, COL_EXTENSIONS_HOMEPAGE, COL_EXTENSIONS_I18N,
    COL_EXTENSIONS_ICON, COL_EXTENSIONS_ID, COL_EXTENSIONS_NAME, COL_EXTENSIONS_PUBLIC_KEY,
    COL_EXTENSIONS_SIGNATURE, COL_EXTENSIONS_SINGLE_INSTANCE, COL_EXTENSIONS_VERSION,
    COL_EXTENSION_MIGRATIONS_EXTENSION_ID, COL_EXTENSION_MIGRATIONS_EXTENSION_VERSION,
    COL_EXTENSION_MIGRATIONS_ID, COL_EXTENSION_MIGRATIONS_MIGRATION_NAME,
    COL_EXTENSION_MIGRATIONS_SQL_STATEMENT, COL_EXTENSION_PERMISSIONS_ACTION,
    COL_EXTENSION_PERMISSIONS_CONSTRAINTS, COL_EXTENSION_PERMISSIONS_EXTENSION_ID,
    COL_EXTENSION_PERMISSIONS_ID, COL_EXTENSION_PERMISSIONS_RESOURCE_TYPE,
    COL_EXTENSION_PERMISSIONS_STATUS, COL_EXTENSION_PERMISSIONS_TARGET, TABLE_EXTENSIONS,
    TABLE_EXTENSION_MIGRATIONS, TABLE_EXTENSION_PERMISSIONS,
};
use lazy_static::lazy_static;

// Column alias — loader.rs selects `dev_path` (not in the installer/manager writes).
use crate::table_names::COL_EXTENSIONS_DEV_PATH;

lazy_static! {
    // installer.rs — lookup + write

    pub static ref SQL_SELECT_EXTENSION_ID_BY_PUBKEY_NAME: String = format!(
        "SELECT {COL_EXTENSIONS_ID} FROM {TABLE_EXTENSIONS} \
         WHERE {COL_EXTENSIONS_PUBLIC_KEY} = ? AND {COL_EXTENSIONS_NAME} = ?"
    );

    /// Full update performed during install when an extension with the same
    /// public_key+name already exists (reinstall / upgrade path).
    pub static ref SQL_UPDATE_EXTENSION_ON_INSTALL: String = format!(
        "UPDATE {TABLE_EXTENSIONS} SET \
         {COL_EXTENSIONS_VERSION} = ?, {COL_EXTENSIONS_AUTHOR} = ?, {COL_EXTENSIONS_ENTRY} = ?, \
         {COL_EXTENSIONS_ICON} = ?, {COL_EXTENSIONS_SIGNATURE} = ?, {COL_EXTENSIONS_HOMEPAGE} = ?, \
         {COL_EXTENSIONS_DESCRIPTION} = ?, {COL_EXTENSIONS_ENABLED} = ?, \
         {COL_EXTENSIONS_SINGLE_INSTANCE} = ?, {COL_EXTENSIONS_DISPLAY_MODE} = ?, \
         {COL_EXTENSIONS_I18N} = ? \
         WHERE {COL_EXTENSIONS_ID} = ?"
    );

    pub static ref SQL_INSERT_EXTENSION: String = format!(
        "INSERT INTO {TABLE_EXTENSIONS} \
         ({COL_EXTENSIONS_ID}, {COL_EXTENSIONS_NAME}, {COL_EXTENSIONS_VERSION}, {COL_EXTENSIONS_AUTHOR}, \
          {COL_EXTENSIONS_ENTRY}, {COL_EXTENSIONS_ICON}, {COL_EXTENSIONS_PUBLIC_KEY}, {COL_EXTENSIONS_SIGNATURE}, \
          {COL_EXTENSIONS_HOMEPAGE}, {COL_EXTENSIONS_DESCRIPTION}, {COL_EXTENSIONS_ENABLED}, \
          {COL_EXTENSIONS_SINGLE_INSTANCE}, {COL_EXTENSIONS_DISPLAY_MODE}, {COL_EXTENSIONS_I18N}) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    );

    pub static ref SQL_INSERT_EXTENSION_PERMISSION: String = format!(
        "INSERT INTO {TABLE_EXTENSION_PERMISSIONS} \
         ({COL_EXTENSION_PERMISSIONS_ID}, {COL_EXTENSION_PERMISSIONS_EXTENSION_ID}, \
          {COL_EXTENSION_PERMISSIONS_RESOURCE_TYPE}, {COL_EXTENSION_PERMISSIONS_ACTION}, \
          {COL_EXTENSION_PERMISSIONS_TARGET}, {COL_EXTENSION_PERMISSIONS_CONSTRAINTS}, \
          {COL_EXTENSION_PERMISSIONS_STATUS}) \
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    );

    /// Lighter update used when only the manifest metadata changed (no toggles).
    pub static ref SQL_UPDATE_EXTENSION_METADATA: String = format!(
        "UPDATE {TABLE_EXTENSIONS} SET \
         {COL_EXTENSIONS_VERSION} = ?, {COL_EXTENSIONS_AUTHOR} = ?, {COL_EXTENSIONS_ENTRY} = ?, \
         {COL_EXTENSIONS_ICON} = ?, {COL_EXTENSIONS_SIGNATURE} = ?, {COL_EXTENSIONS_HOMEPAGE} = ?, \
         {COL_EXTENSIONS_DESCRIPTION} = ?, {COL_EXTENSIONS_I18N} = ? \
         WHERE {COL_EXTENSIONS_ID} = ?"
    );

    // loader.rs — read

    pub static ref SQL_LIST_EXTENSIONS: String = format!(
        "SELECT {COL_EXTENSIONS_ID}, {COL_EXTENSIONS_NAME}, {COL_EXTENSIONS_VERSION}, {COL_EXTENSIONS_AUTHOR}, \
                {COL_EXTENSIONS_ENTRY}, {COL_EXTENSIONS_ICON}, {COL_EXTENSIONS_PUBLIC_KEY}, {COL_EXTENSIONS_SIGNATURE}, \
                {COL_EXTENSIONS_HOMEPAGE}, {COL_EXTENSIONS_DESCRIPTION}, {COL_EXTENSIONS_ENABLED}, \
                {COL_EXTENSIONS_SINGLE_INSTANCE}, {COL_EXTENSIONS_DISPLAY_MODE}, {COL_EXTENSIONS_DEV_PATH}, \
                {COL_EXTENSIONS_I18N} \
         FROM {TABLE_EXTENSIONS}"
    );

    // manager.rs + removal.rs — toggles (consolidated)

    pub static ref SQL_UPDATE_EXTENSION_DISPLAY_MODE: String = format!(
        "UPDATE {TABLE_EXTENSIONS} SET {COL_EXTENSIONS_DISPLAY_MODE} = ? WHERE {COL_EXTENSIONS_ID} = ?"
    );

    /// Used by `manager::set_enabled` (value from caller) and by `removal::disable`
    /// (caller binds `0`). Previously duplicated across both sites.
    pub static ref SQL_UPDATE_EXTENSION_ENABLED: String = format!(
        "UPDATE {TABLE_EXTENSIONS} SET {COL_EXTENSIONS_ENABLED} = ? WHERE {COL_EXTENSIONS_ID} = ?"
    );

    // removal.rs — hard delete

    pub static ref SQL_DELETE_EXTENSION: String = format!(
        "DELETE FROM {TABLE_EXTENSIONS} WHERE {COL_EXTENSIONS_ID} = ?"
    );

    // migrations.rs — record applied migration

    pub static ref SQL_INSERT_EXTENSION_MIGRATION: String = format!(
        "INSERT OR IGNORE INTO {TABLE_EXTENSION_MIGRATIONS} \
         ({COL_EXTENSION_MIGRATIONS_ID}, {COL_EXTENSION_MIGRATIONS_EXTENSION_ID}, \
          {COL_EXTENSION_MIGRATIONS_EXTENSION_VERSION}, {COL_EXTENSION_MIGRATIONS_MIGRATION_NAME}, \
          {COL_EXTENSION_MIGRATIONS_SQL_STATEMENT}) \
         VALUES (?, ?, ?, ?, ?)"
    );
}
