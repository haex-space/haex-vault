//! Core passwords module.
//!
//! Absorbed from the haex-pass extension into haex-vault as a first-class
//! feature on 2026-04-19. Extensions access the password vault exclusively
//! through bridge commands in `commands.rs` — direct access to
//! `haex_passwords_*` system tables is forbidden (system tables are
//! off-limits to extensions by policy).
//!
//! Access scoping is performed via tags: the permission's `target` field
//! restricts an extension to items carrying a specific tag ("calendar",
//! "mail", ...), or grants `*` for full access.

pub mod commands;
pub mod types;
