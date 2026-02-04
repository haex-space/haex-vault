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

pub use helpers::{
    execute_migration_statements,
    ExtensionSqlContext,
};
