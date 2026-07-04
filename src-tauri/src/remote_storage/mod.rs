// src-tauri/src/remote_storage/mod.rs
//!
//! Remote Storage API
//!
//! Provides S3-compatible storage backends for extensions.
//! Extensions can upload/download data without CORS issues.
//!

pub mod backend;
pub mod commands;
pub mod error;
pub mod iam_admin_creds;
pub mod progress;
pub mod queries;
pub mod share_access_flags;
pub mod streaming;
pub mod types;

pub use commands::*;
pub use error::StorageError;
