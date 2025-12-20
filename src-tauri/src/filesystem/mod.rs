// src-tauri/src/filesystem/mod.rs
//!
//! Filesystem Module
//!
//! Provides generic filesystem operations used throughout the application.
//! Extension-specific filesystem commands with permission checks are in extension/filesystem/.

pub mod commands;

pub use commands::*;
