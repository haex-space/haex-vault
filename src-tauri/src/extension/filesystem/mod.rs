// src-tauri/src/extension/filesystem/mod.rs
//!
//! Filesystem Module for extensions
//!
//! Provides E2E encrypted file synchronization with support for multiple storage backends.
//!

pub mod commands;
pub mod encryption;
pub mod error;
pub mod storage;
pub mod sync;
pub mod types;

pub use error::FileSyncError;
pub use types::*;
