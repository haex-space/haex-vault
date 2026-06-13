//! Shared test fixtures for `space_delivery::local` tests.
//!
//! This module is `#[cfg(test)]` only. It exposes a small helper surface
//! sufficient for `auth_gate_tests` — primarily a minimal in-memory DB
//! seeded with `haex_identities` + `haex_space_members` (the two tables
//! `is_active_space_member` joins over), plus a `make_ucan` builder.
//!
//! The schema deliberately mirrors a subset of `inbound_sync_tests::
//! setup_authz_db`: only the columns the membership-check SQL touches.
//! Seeding goes through `database::core::execute` so the rows are shaped
//! the same way a CRDT-applied row would be (CRDT-helper-friendly).
//!
//! ## Why not bypass CRDT helpers for test seeding?
//!
//! `haex_space_members` is a sync table — per project convention every
//! mutation must flow through `core::execute_with_crdt` / `core::execute`
//! so that triggers/HLC tracking happen consistently. The inbound_sync
//! test suite already follows this pattern (see `insert_identity` /
//! `insert_member`); we match it here for the same reason.

#![cfg(test)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde_json::json;

use crate::crdt::hlc::HlcService;
use crate::database::connection_context::ConnectionContext;
use crate::database::core::{self, install_tx_hlc_hooks, register_current_hlc_udf};
use crate::database::DbConnection;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};
use crate::ucan::{CapabilityLevel, ValidatedUcan};

/// In-memory DB with the minimum schemas `is_active_space_member` reads:
/// `haex_identities` + `haex_space_members`, plus the CRDT bookkeeping
/// tables the HLC hooks require.
///
/// ## Schema parity with production
///
/// The columns below are a deliberate subset of the production Drizzle
/// schemas (`src/database/schemas/identity.ts` for `haex_identities`,
/// `src/database/schemas/spaces.ts` for `haex_space_members`). We mirror
/// every `NOT NULL` column production declares — even the ones our tests
/// never read — so that `insert_identity` / `insert_member` exercise the
/// same constraints production code does.
///
/// Mirrored from `haex_identities`: `id`, `did` (UNIQUE), `name` (NOT NULL),
/// `source` (NOT NULL DEFAULT 'contact'), `private_key`, `created_at`.
/// Deliberately omitted from `haex_identities`: `avatar`, `avatar_options`,
/// `notes` — purely optional UI columns the membership-check SQL never
/// touches.
///
/// Mirrored from `haex_space_members`: `id`, `space_id` (NOT NULL),
/// `identity_id` (NOT NULL), `role` (NOT NULL DEFAULT 'read'),
/// `authored_by_did`, `joined_at`. No production columns omitted —
/// the table is small enough that we keep it at full parity.
///
/// CRDT-helper columns (e.g. `haex_tombstone`, HLC timestamps) are added
/// by `core::execute` at write-time, not by the migration, so they don't
/// appear in this CREATE TABLE.
pub(crate) fn setup_membership_db() -> DbConnection {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    let hlc = HlcService::new_for_testing("test-device");
    let ctx = ConnectionContext::new();
    register_current_hlc_udf(&conn, hlc, ctx.clone()).expect("register hlc udf");
    install_tx_hlc_hooks(&conn, ctx).expect("install hlc hooks");

    conn.execute_batch(&format!(
        "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL);",
        TABLE_CRDT_CONFIGS
    ))
    .expect("create crdt_configs");
    conn.execute_batch(&format!(
        "CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT);",
        TABLE_CRDT_DIRTY_TABLES
    ))
    .expect("create crdt_dirty_tables");

    conn.execute_batch(
        "CREATE TABLE haex_identities (
            id TEXT PRIMARY KEY,
            did TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'contact',
            private_key TEXT,
            created_at TEXT
        );

        CREATE TABLE haex_space_members (
            id TEXT PRIMARY KEY,
            space_id TEXT NOT NULL,
            identity_id TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'read',
            authored_by_did TEXT,
            joined_at TEXT
        );",
    )
    .expect("create membership schema");

    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

/// Insert an identity row keyed by `identity_id` with public DID `did`.
///
/// `name` and `source` are `NOT NULL` in production, so we provide
/// sensible defaults (`"Test Identity"` / `"own"`) even though the
/// membership-check SQL never reads them. Keeps the insert at parity
/// with what real code paths produce.
pub(crate) fn insert_identity(db: &DbConnection, identity_id: &str, did: &str) {
    core::execute(
        "INSERT INTO haex_identities (id, did, name, source) VALUES (?1, ?2, ?3, ?4)"
            .to_string(),
        vec![
            json!(identity_id),
            json!(did),
            json!("Test Identity"),
            json!("own"),
        ],
        db,
    )
    .expect("seed identity");
}

/// Insert a space-membership row joining `space_id` and `identity_id`.
/// The presence of this row is what `is_active_space_member` checks; with
/// the delete-log model (no `haex_tombstone` column) revocation = absence.
pub(crate) fn insert_member(
    db: &DbConnection,
    member_row_id: &str,
    space_id: &str,
    identity_id: &str,
    role: &str,
) {
    core::execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id, role) \
         VALUES (?1, ?2, ?3, ?4)"
            .to_string(),
        vec![
            json!(member_row_id),
            json!(space_id),
            json!(identity_id),
            json!(role),
        ],
        db,
    )
    .expect("seed membership");
}

/// Build a `ValidatedUcan` with a single capability entry. Mirrors the
/// helper in `inbound_sync_tests` — kept in sync deliberately because
/// the AuthGate consumes the same shape produced by `validate_token`.
pub(crate) fn make_ucan(
    audience: &str,
    space_id: &str,
    level: CapabilityLevel,
) -> ValidatedUcan {
    let mut capabilities = HashMap::new();
    capabilities.insert(space_id.to_string(), level);
    ValidatedUcan {
        issuer: "did:key:zIssuer".to_string(),
        audience: audience.to_string(),
        capabilities,
        expires_at: u64::MAX,
    }
}
