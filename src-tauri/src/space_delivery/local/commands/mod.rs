//! Tauri commands for the local delivery service.

mod invites;
mod lifecycle;
mod owner_sync;
mod peers;
mod push_invite;

// Glob re-exports so the `#[tauri::command]` macro-generated `__cmd__<name>`
// helper structs are visible alongside each command at the `commands::` path
// (required by `tauri::generate_handler!` in `lib.rs`).
pub use invites::*;
pub use lifecycle::*;
pub use owner_sync::*;
pub use peers::*;
pub use push_invite::*;

#[cfg(test)]
#[path = "../commands_tests.rs"]
mod tests;
