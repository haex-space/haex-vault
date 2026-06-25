//! Tests for [`super::inbound_sync`] — the single authorisation choke
//! point for inbound CRDT pushes from space peers.
//!
//! The pipeline is tested at three levels:
//!
//! - **`validate_and_attribute`** (pure transform): table whitelist,
//!   `space_id` column scope, `authored_by_did` strip + re-injection.
//! - **`authorize_inbound_sync_push`** (pipeline): capability gate,
//!   active-membership gate, per-row space scope, per-row ownership.
//! - **`enforce_row_space_scope`** (cross-space attack surface): a member
//!   of two spaces cannot rewrite a foreign-space row by omitting the
//!   `space_id` column from the change set.
//!
//! Test DBs are built with `setup_authz_db()` — schemas mirror production
//! but **skip CRDT triggers**: the authorisation pipeline reads only via
//! `read_existing_column` (a plain `SELECT`), so HLC tracking and
//! trigger-driven column-HLC bookkeeping are orthogonal to what these
//! tests assert. Seeding helpers (`insert_identity`, `insert_member` …)
//! go through `database::core::execute`, the same trigger-bypass path
//! production uses for system inserts, so the seed data is shaped exactly
//! like a row applied via a CRDT push would be.

#![cfg(test)]

mod authorize_membership;
mod authorize_row_ownership;
mod authorize_row_space_scope;
mod helpers;
mod validate_and_attribute;
