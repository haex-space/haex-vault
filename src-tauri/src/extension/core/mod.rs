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

pub use context::*;
pub use manager::*;
pub use manifest::*;
pub use migrations::register_bundle_migrations;
pub use path_utils::{find_icon, validate_path_in_directory};
pub use protocol::*;
