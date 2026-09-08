//! Adapter implementations of haex-crdt's provider traits for the
//! haex-vault consumer.
//!
//! These wrappers translate between the fixed trait interfaces exposed by
//! [`haex_crdt`] and the existing haex-vault machinery:
//!
//! - [`device_id::get_or_create_device_id_from_store`] resolves the
//!   persistent device UUID that scopes HLC state, from the Tauri
//!   `instance.json` store. `haex-crdt` takes this as a plain `Uuid` at
//!   `HlcService::initialize_in_place` — there is no provider trait object
//!   for it any more.
//! - [`HaexVaultSignatureProvider`] round-trips per-column signatures. The
//!   real integration will bind this to `crate::crdt::column_sig::*` and the
//!   per-space `SpaceKeyCache`; the test-only constructor in this batch
//!   proves the trait shape is usable without that plumbing yet.
//! - [`HaexVaultMigrationSource`] exposes haex-vault's shipped schema
//!   migrations (drizzle + manual) to the crate's app-migration journal.
//!
//! **Batch scope:** `HaexVaultSignatureProvider` and `HaexVaultMigrationSource`
//! are additive — nothing here is wired into `AppState` construction or the
//! CRDT execution path yet; see the integration plan
//! (`docs/plans/2026-09-07-haex-crdt-integration.md`) for the sequence of
//! subsequent batches that perform the cutover. `device_id`'s resolver *is*
//! already wired directly into `database::open`'s HLC initialization.

pub mod device_id;
pub mod migration_source;
pub mod signature;

pub use migration_source::HaexVaultMigrationSource;
pub use signature::HaexVaultSignatureProvider;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
