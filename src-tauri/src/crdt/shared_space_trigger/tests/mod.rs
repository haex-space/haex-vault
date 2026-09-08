//! Tests for the shared-space trigger layer, split by subject.
//!
//! These exercise the composed wrappers through their public API, so they are
//! the evidence that stacking vault's shared-space DDL on top of `haex_crdt`
//! preserves the per-space delete-propagation semantics vault owns.

mod business_delete;
mod composition;
mod crdt_columns;
mod fixtures;
mod register_fanout;
