// src-tauri/src/extension/limits/mod.rs
//!
//! Extension resource limits
//!
//! This module provides limit configuration and enforcement for all extension
//! resource access types:
//! - Database queries (timeout, result size, concurrent queries, SQL size)
//! - Web requests (rate limiting, bandwidth, concurrent requests)
//! - Filesystem access (storage quota, file size, concurrent operations)
//!
//! Each resource type has its own submodule with types, enforcement logic, and tests.
//!
//! Limits are configured per-extension in the `haex_extension_limits` table.
//! Default limits are applied when no custom configuration exists.

pub mod commands;
pub mod database;
pub mod filesystem;
pub mod service;
pub mod types;
pub mod web;

pub use service::LimitsService;
pub use types::{ExtensionLimits, LimitError};
