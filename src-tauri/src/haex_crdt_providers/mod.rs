//! Adapter implementations of the three haex-crdt provider traits for the
//! haex-vault consumer.
//!
//! These wrappers translate between the fixed trait interfaces exposed by
//! [`haex_crdt`] and the existing haex-vault machinery:
//!
//! - [`HaexVaultDeviceIdProvider`] resolves the persistent device UUID that
//!   scopes HLC state. In haex-vault the source of truth is the file at
//!   `<app_data>/device_id` (see `crate::device`).
//! - [`HaexVaultSignatureProvider`] round-trips per-column signatures. The
//!   real integration will bind this to `crate::crdt::column_sig::*` and the
//!   per-space `SpaceKeyCache`; the test-only constructor in this batch
//!   proves the trait shape is usable without that plumbing yet.
//! - [`HaexVaultMigrationSource`] exposes haex-vault's shipped schema
//!   migrations (drizzle + manual) to the crate's app-migration journal.
//!
//! **Batch scope:** this module is additive. Nothing here is wired into
//! `AppState` construction or the CRDT execution path yet — see the
//! integration plan (`docs/plans/2026-09-07-haex-crdt-integration.md`) for
//! the sequence of subsequent batches that perform the cutover.

pub mod device_id;
pub mod migration_source;
pub mod signature;

pub use device_id::HaexVaultDeviceIdProvider;
pub use migration_source::HaexVaultMigrationSource;
pub use signature::HaexVaultSignatureProvider;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
