//! Typed reads and writes of the vault tables (spec 024, research R19).
//!
//! Writes take haex-crdt's [`haex_crdt::CrdtTransaction`] and run inside
//! [`crate::vault_gate::VaultDb::write`]: the CRDT transformer stamps every
//! statement with the transaction HLC, so no helper sets
//! `haex_hlc_no_sync` itself (haex-crdt rejects that), and everything one
//! `write` does is one transaction group for sync. Tables ending in
//! `_no_sync` go through the same call and stay device-local. Reads take any
//! [`query::Query`], so they run on [`crate::vault_gate::VaultDb::read`] or
//! inside a `write` that reads, then writes.
//!
//! Bootstrap-time inserts (`HolziBootstrap`) are the exception: they run in
//! haex-crdt's own bootstrap transaction before the HLC exists, so those rows
//! carry a NULL row-level HLC and stay sync-invisible until a later write
//! touches them. See contract §"Vault identity and device model".

pub mod chat_messages;
#[cfg(test)]
mod chat_messages_tests;
pub mod chat_threads;
pub mod known_devices;
#[cfg(test)]
mod known_devices_tests;
pub mod maintenance;
#[cfg(test)]
mod maintenance_tests;
pub mod models;
pub mod preferences;
pub mod preferences_commands;
#[cfg(test)]
pub mod preferences_commands_tests;
#[cfg(test)]
pub mod preferences_tests;
pub mod providers;
pub mod query;
pub mod uuid;
pub mod voice_transcript;
#[cfg(test)]
mod voice_transcript_tests;
pub mod wm_session;
pub mod wm_session_commands;
#[cfg(test)]
mod wm_session_tests;
