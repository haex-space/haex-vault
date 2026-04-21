//! Logging Queries
//!
//! Centralized SQL for the logging subsystem. Follows the `lazy_static!` +
//! `format!` pattern established in `remote_storage/queries.rs`.
//!
//! Two queries in `logging/mod.rs` build dynamic WHERE/NOT IN clauses and
//! are kept inline with `format!()` using the same `table_names` constants
//! — see `query_logs` and the "exclude custom-retention extensions"
//! branch of `cleanup_logs`.

use crate::table_names::{
    COL_LOGS_DEVICE_ID, COL_LOGS_EXTENSION_ID, COL_LOGS_ID, COL_LOGS_LEVEL, COL_LOGS_MESSAGE,
    COL_LOGS_METADATA, COL_LOGS_SOURCE, COL_LOGS_TIMESTAMP, COL_VAULT_SETTINGS_KEY,
    COL_VAULT_SETTINGS_VALUE, TABLE_LOGS, TABLE_VAULT_SETTINGS,
};
use lazy_static::lazy_static;

const LOG_LEVEL_KEY: &str = "log_level";
const LOG_RETENTION_DAYS_KEY: &str = "log_retention_days";
const EXT_ID_COL: &str = COL_LOGS_EXTENSION_ID;

lazy_static! {
    // Log level lookup

    pub static ref SQL_GET_LOG_LEVEL_BY_EXTENSION: String = format!(
        "SELECT {COL_VAULT_SETTINGS_VALUE} FROM {TABLE_VAULT_SETTINGS} \
         WHERE {COL_VAULT_SETTINGS_KEY} = '{LOG_LEVEL_KEY}' AND {EXT_ID_COL} = ?1"
    );

    pub static ref SQL_GET_LOG_LEVEL_GLOBAL: String = format!(
        "SELECT {COL_VAULT_SETTINGS_VALUE} FROM {TABLE_VAULT_SETTINGS} \
         WHERE {COL_VAULT_SETTINGS_KEY} = '{LOG_LEVEL_KEY}' AND {EXT_ID_COL} IS NULL"
    );

    // Log retention lookup

    pub static ref SQL_GET_RETENTION_DAYS_BY_EXTENSION: String = format!(
        "SELECT {COL_VAULT_SETTINGS_VALUE} FROM {TABLE_VAULT_SETTINGS} \
         WHERE {COL_VAULT_SETTINGS_KEY} = '{LOG_RETENTION_DAYS_KEY}' AND {EXT_ID_COL} = ?1"
    );

    pub static ref SQL_GET_RETENTION_DAYS_GLOBAL: String = format!(
        "SELECT {COL_VAULT_SETTINGS_VALUE} FROM {TABLE_VAULT_SETTINGS} \
         WHERE {COL_VAULT_SETTINGS_KEY} = '{LOG_RETENTION_DAYS_KEY}' AND {EXT_ID_COL} IS NULL"
    );

    pub static ref SQL_LIST_CUSTOM_RETENTION_EXTENSIONS: String = format!(
        "SELECT {EXT_ID_COL}, {COL_VAULT_SETTINGS_VALUE} FROM {TABLE_VAULT_SETTINGS} \
         WHERE {COL_VAULT_SETTINGS_KEY} = '{LOG_RETENTION_DAYS_KEY}' AND {EXT_ID_COL} IS NOT NULL"
    );

    // Log inserts

    /// Minimal insert used by `log_to_db` — no extension_id, no metadata,
    /// device_id literal `'rust'`.
    pub static ref SQL_INSERT_LOG_MINIMAL: String = format!(
        "INSERT INTO {TABLE_LOGS} \
         ({COL_LOGS_ID}, {COL_LOGS_TIMESTAMP}, {COL_LOGS_LEVEL}, {COL_LOGS_SOURCE}, \
          {COL_LOGS_EXTENSION_ID}, {COL_LOGS_MESSAGE}, {COL_LOGS_METADATA}, {COL_LOGS_DEVICE_ID}) \
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, NULL, 'rust')"
    );

    /// Full insert with all optional fields bound as parameters.
    pub static ref SQL_INSERT_LOG_FULL: String = format!(
        "INSERT INTO {TABLE_LOGS} \
         ({COL_LOGS_ID}, {COL_LOGS_TIMESTAMP}, {COL_LOGS_LEVEL}, {COL_LOGS_SOURCE}, \
          {COL_LOGS_EXTENSION_ID}, {COL_LOGS_MESSAGE}, {COL_LOGS_METADATA}, {COL_LOGS_DEVICE_ID}) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    );

    // Log cleanup (CRDT tombstones via execute_with_crdt)

    pub static ref SQL_DELETE_CONSOLE_LOGS_BEFORE: String = format!(
        "DELETE FROM {TABLE_LOGS} \
         WHERE {COL_LOGS_SOURCE} = 'console' AND {COL_LOGS_EXTENSION_ID} IS NULL \
         AND {COL_LOGS_TIMESTAMP} < ?1"
    );

    pub static ref SQL_DELETE_EXTENSION_LOGS_BEFORE: String = format!(
        "DELETE FROM {TABLE_LOGS} \
         WHERE {COL_LOGS_EXTENSION_ID} = ?1 AND {COL_LOGS_TIMESTAMP} < ?2"
    );

    pub static ref SQL_DELETE_LOGS_EXCEPT_CONSOLE_BEFORE: String = format!(
        "DELETE FROM {TABLE_LOGS} \
         WHERE {COL_LOGS_SOURCE} != 'console' AND {COL_LOGS_TIMESTAMP} < ?1"
    );
}
