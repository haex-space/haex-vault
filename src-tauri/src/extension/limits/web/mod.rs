// src-tauri/src/extension/limits/web/mod.rs
//!
//! Web request-specific limit enforcement (placeholder for future implementation)

mod enforcer;

#[cfg(test)]
mod tests;

pub use enforcer::{WebLimitEnforcer, WebRequestGuard, WebRequestTracker};
