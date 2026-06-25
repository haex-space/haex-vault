// src-tauri/src/database/migrations/mod.rs
// Core migration system for system tables

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[cfg(test)]
#[path = "../migrations_tests.rs"]
mod tests;

mod apply;
mod load;
mod pending_columns;
mod query;

pub use apply::*;
pub use pending_columns::*;
pub use query::*;

/// Drizzle migration journal entry
#[derive(Debug, Deserialize)]
pub(super) struct JournalEntry {
    pub(super) idx: u32,
    pub(super) tag: String,
}

/// Drizzle migration journal format (meta/_journal.json)
#[derive(Debug, Deserialize)]
pub(super) struct MigrationJournal {
    pub(super) entries: Vec<JournalEntry>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MigrationInfo {
    pub migration_name: String,
    pub migration_content: String,
    pub is_applied: bool,
    pub applied_at: Option<String>,
}

/// Represents a pending column that was skipped during sync
#[derive(Debug, Serialize, Clone, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PendingColumn {
    pub table_name: String,
    pub column_name: String,
}

/// Row-aware view of a pending marker: the column owed for one specific row.
/// Rust-internal (the P2P recovery path); the TS/HTTP path uses the
/// column-granular `PendingColumn`.
#[derive(Debug, Clone)]
pub struct PendingColumnRow {
    pub table_name: String,
    pub column_name: String,
    pub row_pks: String,
}
