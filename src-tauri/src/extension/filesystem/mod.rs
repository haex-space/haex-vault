// src-tauri/src/extension/filesystem/mod.rs
//!
//! Filesystem Module for extensions
//!
//! Provides local filesystem operations like file watching and unified file I/O.
//!

pub mod file_io;
#[cfg(test)]
mod tests;
pub mod watcher;
