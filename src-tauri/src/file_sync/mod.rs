//! Backend-agnostic file sync engine
//!
//! Provides a SyncProvider trait for different storage backends
//! and a diff engine that compares two FileState manifests.

pub mod cloud_provider;
pub mod commands;
pub mod crypto;
pub mod diff;
pub mod engine;
pub mod hashing;
pub mod local_provider;
pub mod peer_provider;
pub mod provider;
pub mod scoped_provider;
pub use scoped_provider::ScopedProvider;
pub mod types;
