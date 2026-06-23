/// src-tauri/src/extension/mod.rs
pub mod core;
pub mod crypto;
pub mod database;
pub mod error;
pub mod filesystem;
pub mod limits;
pub mod logging;
pub mod mail;
pub mod notifications;
pub mod permissions;
pub mod remote_storage;
pub mod shell;
pub mod spaces;
pub mod utils;
pub mod web;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod webview;

#[cfg(test)]
mod tests;

mod dev_server;
mod info;
mod lifecycle;
mod permissions_api;
mod sync_tables;
mod webview_window;

pub use dev_server::*;
pub use info::*;
pub use lifecycle::*;
pub use permissions_api::*;
pub use sync_tables::*;
pub use webview_window::*;

// Re-export ExtensionManager at `crate::extension::ExtensionManager` so
// existing imports (e.g. `webview/manager.rs`) keep resolving.
#[cfg(desktop)]
pub use core::manager::ExtensionManager;
