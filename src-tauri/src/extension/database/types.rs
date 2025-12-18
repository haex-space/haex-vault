// src-tauri/src/extension/database/types.rs
//!
//! Types for extension database operations
//!

use serde::Serialize;

/// Result of applying extension migrations
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationResult {
    pub applied_count: usize,
    pub already_applied_count: usize,
    pub applied_migrations: Vec<String>,
}
