//! Backend-agnostic file sync engine
//!
//! Provides a SyncProvider trait for different storage backends
//! and a diff engine that compares two FileState manifests.

pub mod diff;
pub mod provider;
pub mod types;
