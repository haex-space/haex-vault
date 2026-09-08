//! Tests for the space-scoped scanner, split by subject.
//!
//! These exercise the crate-backed wrappers through their public API, so
//! they are the evidence that composing `haex_crdt` preserves the
//! shared-space semantics vault owns.

mod emission;
mod fixtures;
mod origin;
mod owner;
mod registry;
mod whitelists;
