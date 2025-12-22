// src-tauri/src/extension/database/mod.rs
//!
//! Extension database module
//!
//! Provides SQL execution and migration management for extensions.
//!

pub mod commands;
pub mod executor;
pub mod helpers;
pub mod planner;
pub mod queries;
#[cfg(test)]
mod tests;
pub mod types;

pub use commands::{
    apply_synced_extension_migrations, extension_database_execute,
    extension_database_query, extension_database_register_migrations,
};
pub use helpers::{
    execute_migration_statements, execute_sql_with_context, validate_sql_table_prefix,
    ExtensionSqlContext,
};
pub use types::{DatabaseQueryResult, MigrationResult};
