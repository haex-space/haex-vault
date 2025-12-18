// src-tauri/src/extension/filesync/mod.rs
//!
//! FileSync Module for extensions
//!
//! Provides E2E encrypted file synchronization with support for multiple storage backends.
//!

pub mod commands;
pub mod encryption;
pub mod error;
pub mod file_io;
pub mod helpers;
pub mod queries;
pub mod scanner;
pub mod storage;
#[cfg(test)]
mod tests;
pub mod types;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod webview_commands;
