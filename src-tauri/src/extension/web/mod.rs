// src-tauri/src/extension/web/mod.rs
//!
//! Extension web module
//!
//! Provides HTTP fetch and URL opening capabilities for extensions.
//!

pub mod commands;
pub mod helpers;
#[cfg(test)]
mod tests;
pub mod types;

pub use commands::{extension_web_fetch, extension_web_open};
pub use types::{WebFetchRequest, WebFetchResponse};
