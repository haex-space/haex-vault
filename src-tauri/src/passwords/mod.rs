//! The password manager (spec 034-password-manager): entries, folders, tags, history, attachments,
//! passkeys as data, the generator presets, the import and the access check for every caller.
//! Table and column names follow haex-vault 2.x (Legacy) (`haex_passwords_*`, data-model.md); every access goes
//! through [`service`] (added with the stories), never around it.

/// The attachment limit (FR-020): 25 MiB per attachment, checked before the file is read.
pub const ATTACHMENT_LIMIT_BYTES: u64 = 25 * 1024 * 1024;
/// The maximum size of a password-import export buffered in memory. This is deliberately much
/// larger than an attachment so normal exports remain supported, while a document provider cannot
/// grow the parser's input without bound (spec 034, US7). KeePass key files are not subject to this
/// limit: KeePass supports hashing arbitrary key-file contents into the key.
pub const IMPORT_INPUT_LIMIT_BYTES: u64 = 512 * 1024 * 1024;
/// How long a binary stays after the last known link vanished, so a link of another device that
/// has not arrived yet does not run into nothing (FR-022, research R4).
pub const ORPHAN_GRACE_DAYS: i64 = 7;
/// The id of the trash folder (FR-015): a folder row with a fixed id, `name` stays empty.
pub const TRASH_GROUP_ID: &str = "trash";

pub mod access;
#[cfg(test)]
mod access_tests;
pub mod binaries;
#[cfg(test)]
mod binaries_tests;
pub mod clipboard;
#[cfg(test)]
mod clipboard_tests;
pub mod clock;
#[cfg(test)]
mod clock_tests;
pub mod commands;
pub mod copy;
#[cfg(test)]
mod copy_tests;
pub mod groups;
#[cfg(test)]
mod groups_tests;
pub mod ids;
#[cfg(test)]
mod ids_tests;
pub mod import;
pub mod items;
#[cfg(test)]
mod items_tests;
pub mod maintenance;
pub mod model;
pub mod model_passkeys;
pub mod model_references;
#[cfg(test)]
mod model_tests;
pub mod passkey_links;
#[cfg(test)]
mod passkey_links_tests;
pub mod passkeys;
pub mod passkeys_ops;
#[cfg(test)]
mod passkeys_tests;
pub mod presets;
#[cfg(test)]
mod presets_tests;
pub mod references;
pub mod references_db;
#[cfg(test)]
mod references_tests;
pub mod reveal;
#[cfg(test)]
mod reveal_tests;
pub mod service;
pub mod sets;
pub mod settings;
#[cfg(test)]
mod settings_tests;
pub mod snapshots;
#[cfg(test)]
mod snapshots_tests;
pub mod tags;
#[cfg(test)]
mod tags_tests;
#[cfg(test)]
pub(crate) mod test_support;
pub mod totp;
#[cfg(test)]
mod totp_tests;
pub mod trash;
#[cfg(test)]
mod trash_tests;
pub mod usage;
#[cfg(test)]
mod usage_tests;
pub mod webauthn;
#[cfg(test)]
mod webauthn_tests;
