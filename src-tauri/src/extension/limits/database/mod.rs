// src-tauri/src/extension/limits/database/mod.rs
//!
//! Database-specific limit enforcement

mod enforcer;

#[cfg(test)]
mod tests;

pub use enforcer::DatabaseLimitEnforcer;
