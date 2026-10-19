//! The host for haextensions (spec 017, ADR-0004, ADR-0008): signed web bundles that run in
//! sandboxed frames and reach holzi only through one checkpoint in Rust.
//!
//! This module grows with the deliveries of the spec (research R1). The foundation holds the
//! identifiers, the error codes of the bridge and the pure permission model.

pub mod agent_tools;
pub mod bridge;
pub mod bundle;
pub mod commands;
pub mod contacts;
pub mod desktop;
pub mod dev;
pub mod dev_csp;
pub mod encrypted_folder;
pub mod encrypted_folder_choose;
pub mod error;
pub mod fs;
pub mod host;
pub mod ids;
pub mod kv;
pub mod logs;
pub mod mail;
pub mod mime;
pub mod notifications;
pub mod passwords;
pub mod permissions;
pub mod protocol;
pub mod registry;
pub mod remote_storage;
pub mod remote_storage_dialog;
pub mod remote_storage_endpoint;
pub mod remote_storage_keys;
pub mod remote_storage_manage;
pub mod remote_storage_propose;
#[cfg(test)]
pub(crate) mod remote_storage_test_support;
mod remote_storage_tree;
pub mod shell;
pub mod sql;
pub mod web;

// The limits of a bundle (size, entries, ratio) belong to the bundle format and live in the crate
// `haex-bundle` (`haex_bundle::format::limits`), the one implementation shared with the `haex` tool.

/// Days an unreferenced blob is kept before it is freed, as for attachments in spec 034: another
/// device may be about to reference it.
pub const BLOB_ORPHAN_GRACE_DAYS: u64 = 7;

/// Default limits of an extension when `extension_limits` has no row (data-model.md).
pub mod default_limits {
    pub const MAX_ROWS: u64 = 10_000;
    pub const MAX_CONCURRENT: u64 = 20;
    pub const MAX_SQL_BYTES: u64 = 1_000_000;
    pub const TIMEOUT_MS: u64 = 5_000;
    pub const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
    /// Run time of one model job in milliseconds (spec 055 R9): local image models need up to
    /// 130 s a page.
    pub const MODEL_JOB_TIMEOUT_MS: u64 = 300_000;
    /// Run-time limit of one migration (contracts/sql-policy.md §Migrationen).
    pub const MIGRATION_TIMEOUT_MS: u64 = 60_000;
}

#[cfg(test)]
#[path = "capabilities_tests.rs"]
mod capabilities_tests;
