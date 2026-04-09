// src-tauri/src/filesystem/mod.rs
//!
//! Filesystem Module
//!
//! Provides generic filesystem operations used throughout the application.
//! Extension-specific filesystem commands with permission checks are in extension/filesystem/.

pub mod commands;
pub mod path_validation;

pub use commands::*;
pub use path_validation::{check_relative_path, reject_path_traversal};
