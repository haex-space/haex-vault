// src-tauri/src/extension/database/queries.rs
//!
//! SQL queries for extension database operations
//!

use crate::table_names::{
    // CRDT migrations table (local-only, tracks which migrations were applied on this device)
    COL_CRDT_MIGRATIONS_APPLIED_AT, COL_CRDT_MIGRATIONS_EXTENSION_ID, COL_CRDT_MIGRATIONS_ID,
    COL_CRDT_MIGRATIONS_MIGRATION_CONTENT, COL_CRDT_MIGRATIONS_MIGRATION_NAME,
    TABLE_CRDT_MIGRATIONS,
    // Extension migrations table (synced, stores migration definitions)
    COL_EXTENSION_MIGRATIONS_EXTENSION_ID, COL_EXTENSION_MIGRATIONS_EXTENSION_VERSION,
    COL_EXTENSION_MIGRATIONS_ID, COL_EXTENSION_MIGRATIONS_MIGRATION_NAME,
    COL_EXTENSION_MIGRATIONS_SQL_STATEMENT, TABLE_EXTENSION_MIGRATIONS,
    // Extensions table
    COL_EXTENSIONS_ID, COL_EXTENSIONS_NAME, COL_EXTENSIONS_PUBLIC_KEY, TABLE_EXTENSIONS,
};

lazy_static::lazy_static! {
    // ============================================================================
    // Extension Migrations Queries
    // ============================================================================

    /// Get pending migrations for an extension (not yet applied locally)
    pub static ref SQL_GET_PENDING_MIGRATIONS: String = format!(
        "SELECT m.{COL_EXTENSION_MIGRATIONS_MIGRATION_NAME}, m.{COL_EXTENSION_MIGRATIONS_SQL_STATEMENT} \
         FROM {TABLE_EXTENSION_MIGRATIONS} m \
         WHERE m.{COL_EXTENSION_MIGRATIONS_EXTENSION_ID} = ?1 \
           AND NOT EXISTS ( \
               SELECT 1 FROM {TABLE_CRDT_MIGRATIONS} c \
               WHERE c.{COL_CRDT_MIGRATIONS_EXTENSION_ID} = m.{COL_EXTENSION_MIGRATIONS_EXTENSION_ID} \
                 AND c.{COL_CRDT_MIGRATIONS_MIGRATION_NAME} = m.{COL_EXTENSION_MIGRATIONS_MIGRATION_NAME} \
           ) \
         ORDER BY m.{COL_EXTENSION_MIGRATIONS_MIGRATION_NAME} ASC"
    );

    /// Get count of already applied migrations for an extension
    pub static ref SQL_COUNT_APPLIED_MIGRATIONS: String = format!(
        "SELECT COUNT(*) FROM {TABLE_CRDT_MIGRATIONS} \
         WHERE {COL_CRDT_MIGRATIONS_EXTENSION_ID} = ?1"
    );

    /// Get all synced migrations that haven't been applied locally
    pub static ref SQL_GET_SYNCED_PENDING_MIGRATIONS: String = format!(
        "SELECT m.{COL_EXTENSION_MIGRATIONS_EXTENSION_ID}, m.{COL_EXTENSION_MIGRATIONS_MIGRATION_NAME}, \
         m.{COL_EXTENSION_MIGRATIONS_SQL_STATEMENT}, e.{COL_EXTENSIONS_PUBLIC_KEY}, e.{COL_EXTENSIONS_NAME} \
         FROM {TABLE_EXTENSION_MIGRATIONS} m \
         JOIN {TABLE_EXTENSIONS} e ON m.{COL_EXTENSION_MIGRATIONS_EXTENSION_ID} = e.{COL_EXTENSIONS_ID} \
         WHERE NOT EXISTS ( \
               SELECT 1 FROM {TABLE_CRDT_MIGRATIONS} c \
               WHERE c.{COL_CRDT_MIGRATIONS_EXTENSION_ID} = m.{COL_EXTENSION_MIGRATIONS_EXTENSION_ID} \
                 AND c.{COL_CRDT_MIGRATIONS_MIGRATION_NAME} = m.{COL_EXTENSION_MIGRATIONS_MIGRATION_NAME} \
           ) \
         ORDER BY m.{COL_EXTENSION_MIGRATIONS_EXTENSION_ID} ASC, m.{COL_EXTENSION_MIGRATIONS_MIGRATION_NAME} ASC"
    );

    /// Record a migration as applied locally (insert into CRDT migrations table)
    pub static ref SQL_INSERT_CRDT_MIGRATION: String = format!(
        "INSERT OR IGNORE INTO {TABLE_CRDT_MIGRATIONS} \
         ({COL_CRDT_MIGRATIONS_ID}, {COL_CRDT_MIGRATIONS_EXTENSION_ID}, {COL_CRDT_MIGRATIONS_MIGRATION_NAME}, \
          {COL_CRDT_MIGRATIONS_MIGRATION_CONTENT}, {COL_CRDT_MIGRATIONS_APPLIED_AT}) \
         VALUES (?1, ?2, ?3, ?4, datetime('now'))"
    );

    /// Store a migration in the synced migrations table
    pub static ref SQL_INSERT_EXTENSION_MIGRATION: String = format!(
        "INSERT OR IGNORE INTO {TABLE_EXTENSION_MIGRATIONS} \
         ({COL_EXTENSION_MIGRATIONS_ID}, {COL_EXTENSION_MIGRATIONS_EXTENSION_ID}, \
          {COL_EXTENSION_MIGRATIONS_EXTENSION_VERSION}, {COL_EXTENSION_MIGRATIONS_MIGRATION_NAME}, \
          {COL_EXTENSION_MIGRATIONS_SQL_STATEMENT}) \
         VALUES (?, ?, ?, ?, ?)"
    );
}
