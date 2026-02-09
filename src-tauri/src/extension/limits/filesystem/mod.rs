// src-tauri/src/extension/limits/filesystem/mod.rs
//!
//! Filesystem-specific limit enforcement (placeholder for future implementation)

mod enforcer;

#[cfg(test)]
mod tests;

pub use enforcer::FilesystemLimitEnforcer;
