// src-tauri/src/extension/core/mod.rs

pub mod context;
pub mod installer;
pub mod loader;
pub mod manager;
pub mod manifest;
pub mod migrations;
pub mod path_utils;
pub mod protocol;
pub mod removal;
pub mod types;

pub use manager::*;
pub use manifest::*;
pub use path_utils::find_icon;
pub use protocol::*;
